//! Explicit local acceptance check: a real Windows ConPTY plus the system clipboard.
#![cfg(windows)]

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use runbook_core::{model::CommandCreate, model::ProjectCreate, shell, Store};
use serde_json::json;
use std::{
    io::{Read, Write},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

struct ChildGuard(Box<dyn portable_pty::Child + Send + Sync>);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
    }
}

struct ClipboardGuard(arboard::Clipboard, Option<String>);
impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        if let Some(text) = self.1.take() {
            let _ = self.0.set_text(text);
        }
    }
}

#[test]
#[ignore = "Uses real ConPTY and system clipboard; run explicitly on a Windows desktop"]
fn tui_select_edit_copy_and_restore_terminal() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open_application(temp.path().join("data")).unwrap();
    let project = store
        .create_project(ProjectCreate {
            id: Some("tui-smoke".into()),
            name: "中文复制项目".into(),
            path: temp.path().to_string_lossy().into_owned(),
            description: String::new(),
            starred: false,
        })
        .unwrap();
    let command = store
        .create_command(
            &project.id,
            CommandCreate {
                id: Some("copy-me".into()),
                name: "中文命令".into(),
                base: "Write-Output 中文".into(),
                description: "仅复制，不执行".into(),
                params: serde_json::from_value(json!([
                    {"id":"path","label":"目录","type":"text","required":true,"value":""}
                ]))
                .unwrap(),
            },
        )
        .unwrap();
    let before = serde_json::to_value(store.get(&project.id).unwrap()).unwrap();
    let mut cb = arboard::Clipboard::new().unwrap();
    let saved = cb.get_text().ok();
    let mut clipboard = ClipboardGuard(cb, saved);
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 32,
            cols: 110,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let executable = std::env::var("RUNBOOK_TUI_TEST_EXE")
        .unwrap_or_else(|_| env!("CARGO_BIN_EXE_runbook").into());
    let powershell = std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap())
        .join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let mut cmd = CommandBuilder::new(powershell);
    cmd.args(["-NoLogo", "-NoProfile", "-Command"]);
    // A host prompt in the original screen must reappear after leaving the TUI.
    cmd.arg(format!(
        "[Console]::WriteLine('RUNBOOK_HOST_BEFORE'); & '{}' --data-dir '{}' tui --project tui-smoke; $tuiExit = $LASTEXITCODE; [Console]::WriteLine('RUNBOOK_HOST_AFTER'); exit $tuiExit",
        executable.replace('\'', "''"), temp.path().join("data").to_string_lossy().replace('\'', "''")
    ));
    let mut child = ChildGuard(pair.slave.spawn_command(cmd).unwrap());
    drop(pair.slave);
    let output = Arc::new(Mutex::new(Vec::new()));
    let captured = output.clone();
    let mut reader = pair.master.try_clone_reader().unwrap();
    let input = Arc::new(Mutex::new(pair.master.take_writer().unwrap()));
    let terminal_input = input.clone();
    std::thread::spawn(move || {
        let mut bytes = [0; 8192];
        let mut answered = 0;
        while let Ok(len) = reader.read(&mut bytes) {
            if len == 0 {
                break;
            }
            let mut output = captured.lock().unwrap();
            output.extend_from_slice(&bytes[..len]);
            let requests = output.windows(4).filter(|w| *w == b"\x1b[6n").count();
            drop(output);
            // ConPTY asks its host for the initial cursor position before starting the app.
            while answered < requests {
                let mut input = terminal_input.lock().unwrap();
                let _ = input.write_all(b"\x1b[1;1R");
                let _ = input.flush();
                answered += 1;
            }
        }
    });
    let wait_for = |needle: &str| {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let text = String::from_utf8_lossy(&output.lock().unwrap()).into_owned();
            if text.contains(needle) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Missing {needle:?} in PTY output: {text}"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    };
    let send = |data: &[u8]| {
        let mut writer = input.lock().unwrap();
        writer.write_all(data).unwrap();
        writer.flush().unwrap();
    };
    let send_mouse = |kind: &str| {
        // Windows 10's system ConPTY does not forward raw SGR reports. Deliver host
        // records through its real console input queue to exercise both encodings.
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "inject_console_mouse"])
            .env(
                "RUNBOOK_TEST_CONSOLE_PID",
                child.0.process_id().unwrap().to_string(),
            )
            .env("RUNBOOK_TEST_MOUSE_KIND", kind)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "Mouse injection failed: {} {}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    };
    wait_for("命令选择与复制");
    send(b"\r");
    wait_for("复制失败"); // Required values prevent copying and keep the TUI open.
    send(b"/\r"); // Open and close the search editor.
    send(b"e");
    wait_for("临时参数");
    send("目录 A'B".as_bytes());
    send(b"\rc");
    wait_for("已复制到剪贴板");
    let expected = shell::render(&command, &[("path".into(), json!("目录 A'B"))].into()).unwrap();
    assert_eq!(clipboard.0.get_text().unwrap(), expected);
    clipboard.0.set_text("mouse-copy-pending").unwrap();
    send_mouse("native"); // Click the Copy toolbar button.
    let deadline = Instant::now() + Duration::from_secs(5);
    while clipboard.0.get_text().unwrap() != expected {
        assert!(
            Instant::now() < deadline,
            "Mouse toolbar copy did not reach the clipboard"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    clipboard.0.set_text("double-click-pending").unwrap();
    send_mouse("sgr"); // Double-click a command with SGR key records.
    let deadline = Instant::now() + Duration::from_secs(5);
    while clipboard.0.get_text().unwrap() != expected {
        assert!(
            Instant::now() < deadline,
            "Double-click copy did not reach the clipboard"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    send(b"\r");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(Instant::now() < deadline, "TUI did not exit after copying");
        std::thread::sleep(Duration::from_millis(25));
    }
    wait_for("RUNBOOK_HOST_AFTER");
    let text = String::from_utf8_lossy(&output.lock().unwrap()).into_owned();
    assert!(
        text.matches("RUNBOOK_HOST_BEFORE").count() >= 2,
        "Original terminal screen was not restored"
    );
    assert_eq!(
        serde_json::to_value(store.get(&project.id).unwrap()).unwrap(),
        before
    );
    drop(input);
    drop(pair.master);
}

#[test]
#[ignore = "Subprocess helper for the explicit mouse acceptance test"]
fn inject_console_mouse() {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::Console::*;
    let Ok(pid) = std::env::var("RUNBOOK_TEST_CONSOLE_PID") else {
        return;
    };
    let kind = std::env::var("RUNBOOK_TEST_MOUSE_KIND").unwrap();
    // Attach only this short-lived test helper to the spawned test console.
    unsafe {
        FreeConsole();
        assert_ne!(
            AttachConsole(pid.parse().unwrap()),
            0,
            "{}",
            std::io::Error::last_os_error()
        );
    }
    let console = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("CONIN$")
        .unwrap();
    let handle = console.as_raw_handle();
    let mut mode = 0;
    unsafe {
        assert_ne!(GetConsoleMode(handle, &mut mode), 0);
    }
    assert_ne!(
        mode & ENABLE_MOUSE_INPUT,
        0,
        "TUI did not enable mouse capture"
    );
    let records: Vec<INPUT_RECORD> = if kind == "native" {
        [1, 0]
            .into_iter()
            .map(|button| INPUT_RECORD {
                EventType: MOUSE_EVENT as u16,
                Event: INPUT_RECORD_0 {
                    MouseEvent: MOUSE_EVENT_RECORD {
                        dwMousePosition: COORD { X: 1, Y: 1 },
                        dwButtonState: button,
                        dwControlKeyState: 0,
                        dwEventFlags: 0,
                    },
                },
            })
            .collect()
    } else {
        "\x1b[<0;50;4M\x1b[<0;50;4m\x1b[<0;50;4M\x1b[<0;50;4m"
            .encode_utf16()
            .map(|ch| INPUT_RECORD {
                EventType: KEY_EVENT as u16,
                Event: INPUT_RECORD_0 {
                    KeyEvent: KEY_EVENT_RECORD {
                        bKeyDown: 1,
                        wRepeatCount: 1,
                        wVirtualKeyCode: if ch == 27 { 27 } else { 0 },
                        wVirtualScanCode: 0,
                        uChar: KEY_EVENT_RECORD_0 { UnicodeChar: ch },
                        dwControlKeyState: 0,
                    },
                },
            })
            .collect()
    };
    let mut written = 0;
    unsafe {
        assert_ne!(
            WriteConsoleInputW(handle, records.as_ptr(), records.len() as u32, &mut written),
            0
        );
    }
    assert_eq!(written as usize, records.len());
}
