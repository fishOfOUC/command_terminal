//! Keep native console events while accepting SGR reports from Windows PTY hosts.
#[cfg(windows)]
use crossterm::event::{
    KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
};
#[cfg(windows)]
use std::{collections::VecDeque, io::Write, time::Instant};
use std::{io, time::Duration};

pub(super) fn enable_mouse() -> io::Result<()> {
    execute!(io::stdout(), EnableMouseCapture)?;
    // Crossterm's Windows command sets console flags but does not request SGR from a PTY host.
    #[cfg(windows)]
    {
        io::stdout().write_all(b"\x1b[?1000h\x1b[?1006h")?;
        io::stdout().flush()?;
    }
    Ok(())
}

pub(super) fn disable_mouse() {
    #[cfg(windows)]
    {
        let _ = io::stdout().write_all(b"\x1b[?1006l\x1b[?1000l");
        let _ = io::stdout().flush();
    }
    let _ = execute!(io::stdout(), DisableMouseCapture);
}

#[derive(Default)]
pub(super) struct Input {
    #[cfg(windows)]
    pending: VecDeque<Event>,
}

impl Input {
    pub fn next(&mut self, timeout: Duration) -> io::Result<Option<Event>> {
        #[cfg(windows)]
        if let Some(event) = self.pending.pop_front() {
            return Ok(Some(event));
        }
        if !event::poll(timeout)? {
            return Ok(None);
        }
        let event = event::read()?;
        #[cfg(windows)]
        if matches!(&event, Event::Key(key) if key.code == KeyCode::Esc && key.kind != KeyEventKind::Release)
        {
            return self.sgr_or_escape(event).map(Some);
        }
        Ok(Some(event))
    }

    #[cfg(windows)]
    fn sgr_or_escape(&mut self, escape: Event) -> io::Result<Event> {
        let mut original = VecDeque::from([escape]);
        let mut text = String::new();
        let deadline = Instant::now() + Duration::from_millis(75);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() || !event::poll(remaining)? {
                break;
            }
            let event = event::read()?;
            if matches!(&event, Event::Key(key) if key.kind == KeyEventKind::Release) {
                continue;
            }
            let ch = match &event {
                Event::Key(key)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    if let KeyCode::Char(ch) = key.code {
                        Some(ch)
                    } else {
                        None
                    }
                }
                _ => None,
            };
            original.push_back(event);
            let Some(ch) = ch else {
                break;
            };
            text.push(ch);
            if text.ends_with(['M', 'm']) {
                if let Some(mouse) = parse_sgr(&text) {
                    return Ok(Event::Mouse(mouse));
                }
                break;
            }
            if !sgr_prefix(&text) {
                break;
            }
        }
        // A lone Escape, unrelated keys and incomplete sequences retain their original meaning.
        let first = original.pop_front().expect("initial Escape exists");
        self.pending.extend(original);
        Ok(first)
    }
}

#[cfg(windows)]
fn sgr_prefix(text: &str) -> bool {
    text == "["
        || (text.starts_with("[<")
            && text.len() <= 32
            && text[2..].bytes().all(|c| c.is_ascii_digit() || c == b';'))
}

#[cfg(windows)]
fn parse_sgr(text: &str) -> Option<MouseEvent> {
    let release = text.ends_with('m');
    let body = text
        .strip_prefix("[<")?
        .strip_suffix(if release { 'm' } else { 'M' })?;
    let mut fields = body.split(';');
    let code: u16 = fields.next()?.parse().ok()?;
    let column = fields.next()?.parse::<u16>().ok()?.checked_sub(1)?;
    let row = fields.next()?.parse::<u16>().ok()?.checked_sub(1)?;
    if fields.next().is_some() || code >= 128 {
        return None;
    }
    let mut modifiers = KeyModifiers::NONE;
    if code & 4 != 0 {
        modifiers |= KeyModifiers::SHIFT;
    }
    if code & 8 != 0 {
        modifiers |= KeyModifiers::ALT;
    }
    if code & 16 != 0 {
        modifiers |= KeyModifiers::CONTROL;
    }
    let button = match code & 3 {
        1 => MouseButton::Middle,
        2 => MouseButton::Right,
        _ => MouseButton::Left,
    };
    let kind = if code & 64 != 0 {
        match code & 3 {
            0 => MouseEventKind::ScrollUp,
            1 => MouseEventKind::ScrollDown,
            2 => MouseEventKind::ScrollLeft,
            _ => MouseEventKind::ScrollRight,
        }
    } else if code & 32 != 0 {
        if code & 3 == 3 {
            MouseEventKind::Moved
        } else {
            MouseEventKind::Drag(button)
        }
    } else if release || code & 3 == 3 {
        MouseEventKind::Up(button)
    } else {
        MouseEventKind::Down(button)
    };
    Some(MouseEvent {
        kind,
        column,
        row,
        modifiers,
    })
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn sgr_preserves_coordinates_modifiers_and_rejects_invalid_reports() {
        let click = parse_sgr("[<0;50;4M").unwrap();
        assert_eq!(
            (click.column, click.row, click.kind),
            (49, 3, MouseEventKind::Down(MouseButton::Left))
        );
        assert_eq!(
            parse_sgr("[<0;50;4m").unwrap().kind,
            MouseEventKind::Up(MouseButton::Left)
        );
        assert_eq!(
            parse_sgr("[<65;50;4M").unwrap().kind,
            MouseEventKind::ScrollDown
        );
        assert_eq!(
            parse_sgr("[<28;50;4M").unwrap().modifiers,
            KeyModifiers::SHIFT | KeyModifiers::ALT | KeyModifiers::CONTROL
        );
        for invalid in [
            "[<0;0;4M",
            "[<0;4;0M",
            "[<0;999999;4M",
            "[<0;4M",
            "[<0;4;4;4M",
            "[<128;4;4M",
            "[A",
        ] {
            assert!(parse_sgr(invalid).is_none());
        }
        assert!(sgr_prefix("[<65;50;"));
        assert!(!sgr_prefix("[<a"));
    }
}
