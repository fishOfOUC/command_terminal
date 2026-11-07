use super::*;
use ratatui::{backend::TestBackend, Terminal};
use serde_json::json;

fn project(id: &str, name: &str) -> Project {
    serde_json::from_value(json!({
        "id":id,"name":name,"path":std::env::temp_dir().to_string_lossy(),
        "revision":1,"createdAt":0,"updatedAt":0,
        "commands":[
            {"id":"hello","name":"中文输出","base":"echo 中文","revision":1,"createdAt":0,"updatedAt":0},
            {"id":"git","name":"Git status","base":"git status","revision":1,"createdAt":0,"updatedAt":0,
             "params":[
                 {"id":"path","label":"目录","type":"text","required":true,"value":""},
                 {"id":"verbose","label":"详细","flag":"--verbose","type":"boolean","value":false},
                 {"id":"count","label":"数量","flag":"--count","type":"number","min":1,"max":10,"value":2}
             ]}
        ]
    })).unwrap()
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn screen_text(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..buffer.area.height {
        let mut x = 0;
        while x < buffer.area.width {
            let symbol = buffer[(x, y)].symbol();
            text.push_str(symbol);
            // Wide glyphs occupy two cells; their continuation is not rendered.
            x += Span::raw(symbol).width().max(1) as u16;
        }
        text.push('\n');
    }
    text
}

#[test]
fn selection_uses_ids_and_searches_chinese_and_command_text() {
    let mut app = App::new(vec![project("b", "B"), project("a", "A")], Some("b")).unwrap();
    assert_eq!(app.selected_project().unwrap().id, "b");
    app.search_key(key(KeyCode::Char('中')));
    assert_eq!(app.command_indices().len(), 1);
    assert_eq!(app.selected_command().unwrap().id, "hello");
    app.search_key(key(KeyCode::Esc));
    app.move_selection(1);
    assert_eq!(app.selected_command().unwrap().id, "git");
    app.search_key(key(KeyCode::Char('无')));
    app.move_selection(1);
    assert!(app.selected_command().is_none());
    assert!(app.rendered().is_err());
    app.search_key(key(KeyCode::Esc));
    app.focus = Focus::Projects;
    app.search_key(key(KeyCode::Char('A')));
    assert_eq!(app.selected_project().unwrap().id, "a");
    assert_eq!(app.selected_command().unwrap().id, "hello");
    assert!(App::new(vec![], Some("missing")).is_err());
}

#[test]
fn temporary_parameters_validate_and_quote_without_mutating_saved_values() {
    let mut app = App::new(vec![project("a", "A")], Some("a")).unwrap();
    app.move_selection(1);
    assert!(app.rendered().is_err());
    app.editing = Some(0);
    for c in "目录 A'B".chars() {
        app.edit_key(key(KeyCode::Char(c)));
    }
    app.edit_key(key(KeyCode::Tab));
    app.edit_key(key(KeyCode::Char(' ')));
    app.edit_key(key(KeyCode::Tab));
    app.edit_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    app.edit_key(key(KeyCode::Char('0')));
    assert!(app.rendered().is_err());
    app.edit_key(key(KeyCode::Backspace));
    app.edit_key(key(KeyCode::Char('3')));
    app.edit_key(key(KeyCode::Enter));
    assert!(app.editing.is_none());
    assert_eq!(
        app.rendered().unwrap(),
        format!(
            "git status {} --verbose --count '3'",
            shell::quote("目录 A'B")
        )
    );
    assert_eq!(app.selected_command().unwrap().params[0].value, "");
    app.move_selection(-1);
    assert!(app.values.is_empty());
}

#[test]
fn renderer_handles_empty_filtered_small_and_parameter_views() {
    for (width, height) in [(100, 32), (60, 16), (40, 12)] {
        for projects in [vec![], vec![project("a", "中文项目")]] {
            let mut app = App::new(projects, None).unwrap();
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|f| draw(f, &mut app)).unwrap();
            if width >= 60 && !app.projects.is_empty() {
                let screen = screen_text(&terminal);
                assert!(screen.contains("RUNBOOK"));
                assert!(screen.replace(' ', "").contains("中文项目"));
                assert!(screen.replace(' ', "").contains("Enter复制并退出"));
                app.move_selection(1);
                app.editing = Some(2);
                terminal.draw(|f| draw(f, &mut app)).unwrap();
                let screen = screen_text(&terminal);
                assert!(
                    screen.replace(' ', "").contains("临时参数"),
                    "{width}x{height}: selected={:?}, screen={screen}",
                    app.command.selected()
                );
            }
            app.queries[1] = "no such command".into();
            app.editing = None;
            terminal.draw(|f| draw(f, &mut app)).unwrap();
        }
    }
}

fn pointer(kind: crossterm::event::MouseEventKind, x: u16, y: u16) -> crossterm::event::MouseEvent {
    crossterm::event::MouseEvent {
        kind,
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }
}

#[test]
fn mouse_clicks_follow_scrolled_rows_and_double_click_only_the_same_command() {
    use crossterm::event::{MouseButton, MouseEventKind};
    let mut p = project("a", "A");
    p.commands = (0..30)
        .map(|i| {
            let mut c = p.commands[0].clone();
            c.id = format!("command-{i}");
            c.name = format!("命令 {i}");
            c
        })
        .collect();
    let mut app = App::new(vec![p], Some("a")).unwrap();
    let mut terminal = Terminal::new(TestBackend::new(100, 32)).unwrap();
    app.move_selection(29);
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let offset = app.command.offset();
    assert!(offset > 0);
    let now = std::time::Instant::now();
    let click = |y| pointer(MouseEventKind::Down(MouseButton::Left), 40, y);
    assert!(matches!(app.mouse_event(click(3), now), MouseAction::None));
    assert_eq!(app.command.selected(), Some(offset));
    // Both the title and the command's second line belong to the same row.
    assert!(matches!(
        app.mouse_event(click(4), now + Duration::from_millis(100)),
        MouseAction::Copy
    ));
    assert!(matches!(
        app.mouse_event(click(5), now + Duration::from_millis(200)),
        MouseAction::None
    ));
    assert_eq!(app.command.selected(), Some(offset + 1));
    assert!(matches!(
        app.mouse_event(click(5), now + Duration::from_secs(1)),
        MouseAction::None
    ));
    assert!(matches!(
        app.mouse_event(
            pointer(MouseEventKind::Down(MouseButton::Right), 40, 5),
            now
        ),
        MouseAction::None
    ));
    app.mouse_event(pointer(MouseEventKind::ScrollUp, 5, 4), now);
    assert!(app.focus == Focus::Projects);
    assert_eq!(app.command.selected(), Some(offset + 1));
    app.mouse_event(pointer(MouseEventKind::ScrollDown, 40, 5), now);
    assert!(app.focus == Focus::Commands);
    assert_eq!(app.command.selected(), Some(offset + 2));
}

#[test]
fn mouse_modal_blocks_background_and_selects_and_toggles_parameters() {
    use crossterm::event::{MouseButton, MouseEventKind};
    let mut app = App::new(vec![project("a", "A"), project("b", "B")], Some("a")).unwrap();
    app.move_selection(1);
    let mut terminal = Terminal::new(TestBackend::new(100, 32)).unwrap();
    let now = std::time::Instant::now();
    let click = |x, y| pointer(MouseEventKind::Down(MouseButton::Left), x, y);
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    app.mouse_event(click(9, 1), now); // Parameter toolbar button.
    assert_eq!(app.editing, Some(0));
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    app.mouse_event(click(17, 1), now); // Background quit button is blocked.
    assert_eq!(app.editing, Some(0));
    app.mouse_event(click(8, 9), now); // Boolean parameter.
    assert_eq!(app.editing, Some(1));
    assert_eq!(app.values["verbose"], true);
    app.mouse_event(pointer(MouseEventKind::ScrollDown, 8, 9), now);
    assert_eq!(app.editing, Some(2));
    app.mouse_event(click(8, 24), now); // Return to preview.
    assert_eq!(app.editing, None);
    assert_eq!(app.values["verbose"], true);
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    assert!(matches!(
        app.mouse_event(click(1, 1), now),
        MouseAction::Copy
    ));
    assert!(matches!(
        app.mouse_event(click(17, 1), now),
        MouseAction::Quit
    ));
    app.mouse_event(pointer(MouseEventKind::ScrollDown, 40, 25), now);
    assert_eq!(app.preview_scroll, 3);
    app.mouse_event(pointer(MouseEventKind::ScrollUp, 40, 25), now);
    assert_eq!(app.preview_scroll, 0);
    for _ in 0..10 {
        app.mouse_event(pointer(MouseEventKind::ScrollDown, 40, 25), now);
    }
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    assert_eq!(app.preview_scroll, 0); // A short preview cannot be scrolled into empty space.
    app.mouse_event(click(5, 4), now); // Switch project.
    assert_eq!(app.selected_project().unwrap().id, "b");
    assert!(app.values.is_empty());
    terminal.backend_mut().resize(40, 12);
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    assert!(matches!(
        app.mouse_event(click(1, 1), now),
        MouseAction::None
    ));
}
