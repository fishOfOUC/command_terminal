use arboard::Clipboard;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};
use runbook_core::{
    model::{value_text, Command, ParameterType, Project},
    shell,
    store::default_data_dir,
    ApiError, Result, Store,
};
use serde_json::Value;
use std::{collections::BTreeMap, io::IsTerminal, path::PathBuf, time::Duration};
mod input;
mod mouse;
use mouse::{MouseAction, MouseMap, Target};

#[derive(Clone, Copy, PartialEq)]
enum Focus {
    Projects,
    Commands,
}

fn comparable_path(path: &str) -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(path.replace('\\', "/").to_lowercase())
    } else {
        PathBuf::from(path)
    }
}

struct App {
    projects: Vec<Project>,
    project: ListState,
    command: ListState,
    focus: Focus,
    queries: [String; 2],
    searching: bool,
    editing: Option<usize>,
    values: BTreeMap<String, Value>,
    status: String,
    preview_scroll: u16,
    mouse: MouseMap,
}

impl App {
    fn new(mut projects: Vec<Project>, initial: Option<&str>) -> Result<Self> {
        projects.sort_by_key(|p| (p.archived, !p.starred, p.name.to_lowercase()));
        let selected = if let Some(id) = initial {
            Some(
                projects
                    .iter()
                    .position(|p| p.id == id)
                    .ok_or_else(|| ApiError::not_found("project", id))?,
            )
        } else {
            // Prefer the most specific registered working directory, without creating a project.
            std::env::current_dir()
                .ok()
                .and_then(|cwd| {
                    let cwd = runbook_core::store::directory(&cwd.to_string_lossy()).ok()?;
                    let cwd = comparable_path(&cwd);
                    projects
                        .iter()
                        .enumerate()
                        .filter(|(_, p)| !p.archived && cwd.starts_with(comparable_path(&p.path)))
                        .max_by_key(|(_, p)| p.path.len())
                        .map(|(i, _)| i)
                })
                .or(if projects.is_empty() { None } else { Some(0) })
        };
        Ok(Self {
            projects,
            project: ListState::default().with_selected(selected),
            command: ListState::default().with_selected(Some(0)),
            focus: Focus::Commands,
            queries: Default::default(),
            searching: false,
            editing: None,
            values: BTreeMap::new(),
            status: "选择命令后按 Enter 复制并退出；c 复制后继续选择".into(),
            preview_scroll: 0,
            mouse: MouseMap::default(),
        })
    }

    fn project_indices(&self) -> Vec<usize> {
        let q = self.queries[0].to_lowercase();
        self.projects
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                format!("{} {} {}", p.name, p.path, p.description)
                    .to_lowercase()
                    .contains(&q)
            })
            .map(|(i, _)| i)
            .collect()
    }

    fn selected_project(&self) -> Option<&Project> {
        self.project
            .selected()
            .and_then(|i| self.project_indices().get(i).copied())
            .map(|i| &self.projects[i])
    }

    fn command_indices(&self) -> Vec<usize> {
        let q = self.queries[1].to_lowercase();
        self.selected_project()
            .map(|p| {
                p.commands
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| {
                        format!("{} {} {}", c.name, c.base, c.description)
                            .to_lowercase()
                            .contains(&q)
                    })
                    .map(|(i, _)| i)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn selected_command(&self) -> Option<&Command> {
        let p = self.selected_project()?;
        self.command
            .selected()
            .and_then(|i| self.command_indices().get(i).copied())
            .map(|i| &p.commands[i])
    }

    fn rendered(&self) -> Result<String> {
        let c = self
            .selected_command()
            .ok_or_else(|| ApiError::new("NOT_FOUND", "当前没有可复制的命令"))?;
        shell::render(c, &self.values)
    }

    fn reset_command(&mut self) {
        self.values.clear();
        self.preview_scroll = 0;
        self.status.clear();
        self.mouse.last_click = None;
    }

    fn move_selection(&mut self, delta: isize) {
        let len = match self.focus {
            Focus::Projects => self.project_indices().len(),
            Focus::Commands => self.command_indices().len(),
        };
        let state = match self.focus {
            Focus::Projects => &mut self.project,
            Focus::Commands => &mut self.command,
        };
        let next = if len == 0 {
            None
        } else {
            Some(
                (state.selected().unwrap_or(0) as isize + delta).clamp(0, len as isize - 1)
                    as usize,
            )
        };
        if state.selected() != next {
            state.select(next);
            if self.focus == Focus::Projects {
                self.command.select(Some(0));
            }
            self.reset_command();
        }
    }

    fn query_mut(&mut self) -> &mut String {
        &mut self.queries[usize::from(self.focus == Focus::Commands)]
    }

    fn search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                self.searching = false;
                return;
            }
            KeyCode::Esc => {
                self.searching = false;
                self.query_mut().clear();
            }
            KeyCode::Backspace => {
                self.query_mut().pop();
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.query_mut().clear()
            }
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.query_mut().push(c)
            }
            _ => return,
        }
        if self.focus == Focus::Projects {
            self.project.select(Some(0));
        }
        self.command.select(Some(0));
        self.reset_command();
    }

    fn edit_key(&mut self, key: KeyEvent) {
        let Some(index) = self.editing else {
            return;
        };
        let Some(c) = self.selected_command() else {
            self.editing = None;
            return;
        };
        let len = c.params.len();
        let p = c.params[index].clone();
        match key.code {
            KeyCode::Esc | KeyCode::Enter => self.editing = None,
            KeyCode::Tab | KeyCode::Down => self.editing = Some((index + 1) % len),
            KeyCode::BackTab | KeyCode::Up => self.editing = Some((index + len - 1) % len),
            KeyCode::Char(' ') if p.kind == ParameterType::Boolean => {
                let value = self
                    .values
                    .get(&p.id)
                    .unwrap_or(&p.value)
                    .as_bool()
                    .unwrap_or(false);
                self.values.insert(p.id, Value::Bool(!value));
            }
            KeyCode::Char('u')
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && p.kind != ParameterType::Boolean =>
            {
                self.values.insert(p.id, Value::String(String::new()));
            }
            KeyCode::Backspace if p.kind != ParameterType::Boolean => {
                let mut text = value_text(self.values.get(&p.id).unwrap_or(&p.value));
                text.pop();
                self.values.insert(p.id, Value::String(text));
            }
            KeyCode::Char(ch)
                if p.kind != ParameterType::Boolean
                    && !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                let mut text = value_text(self.values.get(&p.id).unwrap_or(&p.value));
                if text.chars().count() < 16384 {
                    text.push(ch);
                }
                self.values.insert(p.id, Value::String(text));
            }
            _ => {}
        }
    }

    fn copy(&mut self, clipboard: &mut Option<Clipboard>) -> bool {
        let result = self.rendered().and_then(|text| {
            if clipboard.is_none() {
                *clipboard = Some(
                    Clipboard::new()
                        .map_err(|e| ApiError::new("CLIPBOARD_ERROR", e.to_string()))?,
                );
            }
            clipboard
                .as_mut()
                .expect("clipboard initialized")
                .set_text(text)
                .map_err(|e| ApiError::new("CLIPBOARD_ERROR", e.to_string()))
        });
        match result {
            Ok(()) => {
                self.status = "已复制到剪贴板".into();
                true
            }
            Err(e) => {
                self.status = if e.code == "VALIDATION_ERROR" {
                    format!("复制失败：{}；按 e 填写参数后重试", e.message)
                } else {
                    format!("复制失败：{}", e.message)
                };
                false
            }
        }
    }
}

fn panel(title: String, active: bool) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(if active { Color::Cyan } else { Color::DarkGray }))
}

fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    app.mouse.clear_regions();
    if area.width < 60 || area.height < 16 {
        frame.render_widget(
            Paragraph::new("请将终端放大至至少 60 列 × 16 行\n按 q 或 Ctrl+C 退出"),
            area,
        );
        return;
    }
    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(4),
        Constraint::Length(7),
        Constraint::Length(3),
    ])
    .split(area);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " RUNBOOK ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("命令选择与复制"),
        ])),
        rows[0],
    );
    app.mouse.buttons(
        frame,
        Rect::new(rows[0].x, rows[0].y + 1, rows[0].width, 1),
        &[
            ("[复制]", Target::Copy),
            ("[参数]", Target::Edit),
            ("[退出]", Target::Quit),
        ],
    );
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(rows[1]);
    let projects: Vec<ListItem> = app
        .project_indices()
        .into_iter()
        .map(|i| {
            let p = &app.projects[i];
            ListItem::new(format!(
                "{}{}{}",
                if p.starred { "★ " } else { "" },
                p.name,
                if p.archived { " [已归档]" } else { "" }
            ))
        })
        .collect();
    let commands: Vec<ListItem> = app
        .command_indices()
        .into_iter()
        .map(|i| {
            let c = &app.selected_project().expect("project exists").commands[i];
            ListItem::new(vec![
                Line::from(c.name.clone()),
                Line::styled(c.base.clone(), Style::default().fg(Color::DarkGray)),
            ])
        })
        .collect();
    let project_title = format!(" 项目 ({}) /{} ", projects.len(), app.queries[0]);
    let command_title = format!(" 命令 ({}) /{} ", commands.len(), app.queries[1]);
    let project_heights: Vec<u16> = projects.iter().map(|item| item.height() as u16).collect();
    let command_heights: Vec<u16> = commands.iter().map(|item| item.height() as u16).collect();
    let highlight = Style::default()
        .bg(Color::DarkGray)
        .fg(Color::White)
        .add_modifier(Modifier::BOLD);
    frame.render_stateful_widget(
        List::new(projects)
            .block(panel(project_title, app.focus == Focus::Projects))
            .highlight_style(highlight)
            .highlight_symbol("› "),
        cols[0],
        &mut app.project,
    );
    frame.render_stateful_widget(
        List::new(commands)
            .block(panel(command_title, app.focus == Focus::Commands))
            .highlight_style(highlight)
            .highlight_symbol("› "),
        cols[1],
        &mut app.command,
    );
    app.mouse.list(
        cols[0],
        &project_heights,
        app.project.offset(),
        Target::Projects,
        Target::Project,
    );
    app.mouse.list(
        cols[1],
        &command_heights,
        app.command.offset(),
        Target::Commands,
        Target::Command,
    );
    app.mouse.region(rows[2], Target::Preview);
    if app.command_indices().is_empty() {
        frame.render_widget(
            Paragraph::new("没有命令；可切换项目或清空搜索。"),
            panel(String::new(), false).inner(cols[1]),
        );
    }
    let mut preview = Vec::new();
    if let Some(p) = app.selected_project() {
        preview.push(Line::styled(
            p.path.clone(),
            Style::default().fg(Color::DarkGray),
        ));
    }
    if let Some(c) = app.selected_command() {
        if !c.description.is_empty() {
            preview.push(Line::from(c.description.clone()));
        }
        match app.rendered() {
            Ok(text) => preview.extend(
                text.lines()
                    .map(|s| Line::styled(s.to_owned(), Style::default().fg(Color::Cyan))),
            ),
            Err(e) => {
                preview.push(Line::styled(
                    format!("{}（按 e 填写参数）", e.message),
                    Style::default().fg(Color::Yellow),
                ));
                preview.push(Line::from(c.base.clone()));
            }
        }
    }
    let preview = Paragraph::new(preview).wrap(Wrap { trim: false });
    let max_scroll = preview
        .line_count(rows[2].width.saturating_sub(2))
        .saturating_sub(rows[2].height.saturating_sub(2) as usize)
        .min(u16::MAX as usize) as u16;
    app.preview_scroll = app.preview_scroll.min(max_scroll);
    frame.render_widget(
        preview
            .scroll((app.preview_scroll, 0))
            .block(panel(" 完整命令预览 · PgUp/PgDn 滚动 ".into(), false)),
        rows[2],
    );
    let help = if app.searching {
        "搜索中：输入文字过滤 · Enter 完成 · Esc 清空"
    } else {
        "↑↓/鼠标选择  Tab 切换  / 搜索  e 参数  F5 刷新  q 退出\nEnter 复制并退出  c/双击复制  滚轮滚动  Esc 清空搜索"
    };
    frame.render_widget(Paragraph::new(format!("{help}\n{}", app.status)), rows[3]);

    if let Some(index) = app.editing {
        if let Some(c) = app.selected_command() {
            let width = area.width.saturating_sub(8).min(90);
            let height = area.height.saturating_sub(4).min(18);
            let popup = Rect::new(
                (area.width - width) / 2,
                (area.height - height) / 2,
                width,
                height,
            );
            let items: Vec<ListItem> = c
                .params
                .iter()
                .map(|p| {
                    let v = app.values.get(&p.id).unwrap_or(&p.value);
                    let value = if p.kind == ParameterType::Boolean {
                        if v.as_bool() == Some(true) {
                            "[x]".into()
                        } else {
                            "[ ]".into()
                        }
                    } else {
                        value_text(v)
                    };
                    ListItem::new(format!(
                        "{}{} {}: {}",
                        p.label,
                        if p.required { " *" } else { "" },
                        p.flag,
                        value
                    ))
                })
                .collect();
            frame.render_widget(Clear, popup);
            let parts = Layout::vertical([Constraint::Min(1), Constraint::Length(3)]).split(popup);
            let heights: Vec<u16> = items.iter().map(|item| item.height() as u16).collect();
            let mut parameter_state = ListState::default().with_selected(Some(index));
            frame.render_stateful_widget(
                List::new(items)
                    .block(panel(" 临时参数 · 不保存到数据库 ".into(), true))
                    .highlight_style(highlight)
                    .highlight_symbol("› "),
                parts[0],
                &mut parameter_state,
            );
            frame.render_widget(Paragraph::new("输入追加 · Backspace 删除 · Ctrl+U 清空 · 空格切换开关\nTab/↑↓/鼠标切换参数 · Enter/Esc 返回预览"), parts[1]);
            // A modal owns all pointer input, including the area outside its visible border.
            app.mouse.clear_regions();
            app.mouse.list(
                parts[0],
                &heights,
                parameter_state.offset(),
                Target::Parameters,
                Target::Parameter,
            );
            app.mouse.buttons(
                frame,
                Rect::new(parts[1].x, parts[1].y + 2, parts[1].width, 1),
                &[("[返回预览]", Target::CloseEditor)],
            );
        }
    }
}

struct RestoreTerminal;
impl Drop for RestoreTerminal {
    fn drop(&mut self) {
        input::disable_mouse();
        ratatui::restore();
    }
}

pub fn run(data_dir: Option<PathBuf>, initial: Option<&str>) -> Result<()> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err(ApiError::new(
            "INVALID_ARGUMENT",
            "runbook tui 需要交互式终端；请直接在 PowerShell 或终端中运行",
        ));
    }
    let _runtime_guard = runbook_update::RuntimeGuard::acquire(
        &runbook_update::application_dir()
            .map_err(|e| ApiError::new("UPDATE_ERROR", format!("{e:#}")))?,
    )
    .map_err(|e| ApiError::new("UPDATE_ERROR", format!("{e:#}")))?;
    let store = Store::open_application(data_dir.map(Ok).unwrap_or_else(default_data_dir)?)?;
    let mut app = App::new(store.list()?, initial)?;
    // The guard also restores the screen on draw/read errors; Ratatui installs a panic hook.
    let _restore = RestoreTerminal;
    let mut terminal = ratatui::try_init()?;
    // Restore capture on panics as well as normal exits and I/O errors.
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        input::disable_mouse();
        previous_hook(info);
    }));
    input::enable_mouse()?;
    let mut clipboard = None;
    let mut input = input::Input::default();
    loop {
        terminal.draw(|frame| draw(frame, &mut app))?;
        let Some(event) = input.next(Duration::from_millis(250))? else {
            continue;
        };
        let key = match event {
            Event::Mouse(mouse) => {
                match app.mouse_event(mouse, std::time::Instant::now()) {
                    MouseAction::Copy => {
                        app.copy(&mut clipboard);
                    }
                    MouseAction::Quit => break,
                    MouseAction::None => {}
                }
                continue;
            }
            Event::Key(key) => key,
            Event::Resize(..) => {
                app.mouse.last_click = None;
                continue;
            }
            _ => continue,
        };
        if key.kind == KeyEventKind::Release {
            continue;
        }
        app.mouse.last_click = None;
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            break;
        }
        if app.editing.is_some() {
            app.edit_key(key);
            continue;
        }
        if app.searching {
            app.search_key(key);
            continue;
        }
        match key.code {
            KeyCode::Char('q') => break,
            KeyCode::Esc => {
                if !app.query_mut().is_empty() {
                    app.query_mut().clear();
                    if app.focus == Focus::Projects {
                        app.project.select(Some(0));
                    }
                    app.command.select(Some(0));
                    app.reset_command();
                }
            }
            KeyCode::Tab | KeyCode::BackTab => {
                app.focus = if app.focus == Focus::Projects {
                    Focus::Commands
                } else {
                    Focus::Projects
                }
            }
            KeyCode::Left => app.focus = Focus::Projects,
            KeyCode::Right => app.focus = Focus::Commands,
            KeyCode::Up | KeyCode::Char('k') => app.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => app.move_selection(1),
            KeyCode::Home => app.move_selection(isize::MIN / 2),
            KeyCode::End => app.move_selection(isize::MAX / 2),
            KeyCode::PageUp => app.preview_scroll = app.preview_scroll.saturating_sub(3),
            KeyCode::PageDown => app.preview_scroll = app.preview_scroll.saturating_add(3),
            KeyCode::Char('/') => app.searching = true,
            KeyCode::Enter if app.focus == Focus::Projects => app.focus = Focus::Commands,
            KeyCode::Enter | KeyCode::Char('c') if app.focus == Focus::Commands => {
                if app.copy(&mut clipboard) && key.code == KeyCode::Enter {
                    break;
                }
            }
            KeyCode::Char('e') => {
                if app.selected_command().is_some_and(|c| !c.params.is_empty()) {
                    app.editing = Some(0);
                } else {
                    app.status = "当前命令没有参数".into();
                }
            }
            KeyCode::F(5) => {
                match store.list().and_then(|projects| {
                    let id = app.selected_project().map(|p| p.id.as_str());
                    App::new(projects.clone(), id).or_else(|_| App::new(projects, None))
                }) {
                    Ok(updated) => {
                        app = updated;
                        app.status = "已刷新命令库".into();
                    }
                    Err(e) => app.status = e.to_string(),
                }
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
