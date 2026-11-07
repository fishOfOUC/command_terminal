use super::{App, Focus};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    layout::{Position, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use runbook_core::model::ParameterType;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Target {
    Projects,
    Commands,
    Preview,
    Parameters,
    Project(usize),
    Command(usize),
    Parameter(usize),
    Copy,
    Edit,
    Quit,
    CloseEditor,
}

pub(super) enum MouseAction {
    None,
    Copy,
    Quit,
}

#[derive(Default)]
pub(super) struct MouseMap {
    regions: Vec<(Rect, Target)>,
    pub last_click: Option<(Target, Instant)>,
}

impl MouseMap {
    pub fn clear_regions(&mut self) {
        self.regions.clear();
    }

    pub fn region(&mut self, area: Rect, target: Target) {
        self.regions.push((area, target));
    }

    pub fn list(
        &mut self,
        area: Rect,
        heights: &[u16],
        offset: usize,
        pane: Target,
        item: fn(usize) -> Target,
    ) {
        self.region(area, pane);
        let inner = Block::default().borders(Borders::ALL).inner(area);
        let mut y = inner.y;
        for (i, height) in heights.iter().enumerate().skip(offset) {
            if y >= inner.bottom() {
                break;
            }
            let visible_height = (*height).min(inner.bottom() - y);
            self.region(Rect::new(inner.x, y, inner.width, visible_height), item(i));
            y += visible_height;
        }
    }

    pub fn buttons(&mut self, frame: &mut Frame, area: Rect, labels: &[(&str, Target)]) {
        let mut spans = Vec::new();
        let mut x = area.x;
        for (label, target) in labels {
            let span = Span::styled(*label, Style::default().fg(Color::Cyan));
            let width = (span.width() as u16).min(area.right().saturating_sub(x));
            self.region(Rect::new(x, area.y, width, 1), *target);
            x = x.saturating_add(width + 2);
            spans.extend([span, Span::raw("  ")]);
        }
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }

    fn target(&self, mouse: MouseEvent) -> Option<Target> {
        let point = Position::new(mouse.column, mouse.row);
        self.regions
            .iter()
            .rev()
            .find(|(area, _)| area.contains(point))
            .map(|(_, target)| *target)
    }
}

impl App {
    fn select_at(&mut self, focus: Focus, index: usize) {
        self.focus = focus;
        self.searching = false;
        let len = match focus {
            Focus::Projects => self.project_indices().len(),
            Focus::Commands => self.command_indices().len(),
        };
        let state = match focus {
            Focus::Projects => &mut self.project,
            Focus::Commands => &mut self.command,
        };
        if index >= len {
            return;
        }
        if state.selected() != Some(index) {
            state.select(Some(index));
            if focus == Focus::Projects {
                self.command.select(Some(0));
            }
            self.reset_command();
        }
    }

    pub fn mouse_event(&mut self, mouse: MouseEvent, now: Instant) -> MouseAction {
        let target = self.mouse.target(mouse);
        let delta = match mouse.kind {
            MouseEventKind::ScrollUp => -1,
            MouseEventKind::ScrollDown => 1,
            _ => 0,
        };
        if delta != 0 {
            self.mouse.last_click = None;
            match target {
                Some(Target::Project(_) | Target::Projects) => {
                    self.focus = Focus::Projects;
                    self.move_selection(delta);
                }
                Some(Target::Command(_) | Target::Commands) => {
                    self.focus = Focus::Commands;
                    self.move_selection(delta);
                }
                Some(Target::Preview) => {
                    self.preview_scroll =
                        self.preview_scroll.saturating_add_signed(delta as i16 * 3)
                }
                Some(Target::Parameter(_) | Target::Parameters) => {
                    if let (Some(index), Some(c)) = (self.editing, self.selected_command()) {
                        self.editing = Some(
                            (index as isize + delta)
                                .clamp(0, c.params.len().saturating_sub(1) as isize)
                                as usize,
                        );
                    }
                }
                _ => {}
            }
            return MouseAction::None;
        }
        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return MouseAction::None;
        }
        let previous = self.mouse.last_click.take();
        match target {
            Some(Target::Project(index)) => self.select_at(Focus::Projects, index),
            Some(Target::Command(index)) => {
                self.select_at(Focus::Commands, index);
                if previous.is_some_and(|(last, time)| {
                    last == Target::Command(index)
                        && now.saturating_duration_since(time) <= Duration::from_millis(450)
                }) {
                    return MouseAction::Copy;
                }
                self.mouse.last_click = Some((Target::Command(index), now));
            }
            Some(Target::Projects) => {
                self.focus = Focus::Projects;
                self.searching = false;
            }
            Some(Target::Commands) => {
                self.focus = Focus::Commands;
                self.searching = false;
            }
            Some(Target::Parameter(index)) => {
                self.editing = Some(index);
                if self.selected_command().is_some_and(|c| {
                    c.params
                        .get(index)
                        .is_some_and(|p| p.kind == ParameterType::Boolean)
                }) {
                    self.edit_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
                }
            }
            Some(Target::Copy) => {
                self.searching = false;
                return MouseAction::Copy;
            }
            Some(Target::Quit) => return MouseAction::Quit,
            Some(Target::Edit) => {
                self.searching = false;
                if self
                    .selected_command()
                    .is_some_and(|c| !c.params.is_empty())
                {
                    self.editing = Some(0);
                } else {
                    self.status = "当前命令没有参数".into();
                }
            }
            Some(Target::CloseEditor) => self.editing = None,
            _ => {}
        }
        MouseAction::None
    }
}
