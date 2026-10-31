use base64::{engine::general_purpose::STANDARD, Engine};
use runbook_core::{
    dispatch,
    model::{CommandCreate, ProjectCreate},
    shell,
    terminal::{SessionInfo, StartRequest, TerminalManager},
    Request, Store,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

fn request(store: &Store, value: Value) -> runbook_core::Result<Value> {
    dispatch(store, serde_json::from_value::<Request>(value)?)
}
fn setup() -> (tempfile::TempDir, Store, String) {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().join("data")).unwrap();
    let project = store
        .create_project(ProjectCreate {
            id: Some("test-project".into()),
            name: "测试项目".into(),
            path: temp.path().to_string_lossy().into_owned(),
            description: String::new(),
            starred: false,
        })
        .unwrap();
    (temp, store, project.id)
}
fn command(store: &Store, project: &str, base: &str) -> String {
    store
        .create_command(
            project,
            CommandCreate {
                id: None,
                name: "测试命令".into(),
                base: base.into(),
                description: String::new(),
                params: vec![],
            },
        )
        .unwrap()
        .id
}
#[test]
fn crud_revisions_and_rollback() {
    let (_temp, store, p) = setup();
    let c = command(&store, &p, "echo hello");
    let old = store.command(&p, &c).unwrap();
    let updated=request(&store,json!({"op":"command_update","project":p,"id":c,"patch":{"name":"更新后"},"ifRevision":old.revision})).unwrap();
    assert_eq!(updated["base"], "echo hello");
    assert_eq!(updated["revision"], 2);
    let conflict = request(
        &store,
        json!({"op":"command_delete","project":p,"id":c,"ifRevision":1}),
    )
    .unwrap_err();
    assert_eq!(conflict.code, "REVISION_CONFLICT");
    assert_eq!(store.get(&p).unwrap().commands.len(), 1);
    let before = store.get(&p).unwrap().revision;
    assert!(request(
        &store,
        json!({"op":"command_update","project":p,"id":c,"patch":{"base":""}})
    )
    .is_err());
    assert_eq!(store.command(&p, &c).unwrap().base, "echo hello");
    assert_eq!(store.get(&p).unwrap().revision, before);
    request(
        &store,
        json!({"op":"command_delete","project":p,"id":c,"ifRevision":2}),
    )
    .unwrap();
    assert_eq!(store.command(&p, &c).unwrap_err().code, "NOT_FOUND");
}
#[test]
fn concurrent_writers_preserve_other_commands_and_reject_stale_patches() {
    let (_temp, store, p) = setup();
    let c = command(&store, &p, "echo original");
    let mut threads = vec![];
    for i in 0..8 {
        let store = store.clone();
        let p = p.clone();
        threads.push(std::thread::spawn(move || {
            command(&store, &p, &format!("echo {i}"))
        }));
    }
    for thread in threads {
        thread.join().unwrap();
    }
    assert_eq!(store.get(&p).unwrap().commands.len(), 9);
    let outcomes:Vec<_>=(0..2).map(|i|{let s=store.clone();let p=p.clone();let c=c.clone();std::thread::spawn(move||request(&s,json!({"op":"command_update","project":p,"id":c,"ifRevision":1,"patch":{"name":format!("writer {i}")}})))}).collect();
    let results: Vec<_> = outcomes.into_iter().map(|t| t.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| r
                .as_ref()
                .err()
                .is_some_and(|e| e.code == "REVISION_CONFLICT"))
            .count(),
        1
    );
}
#[test]
fn path_validation_unique_directories_and_strict_inputs() {
    let (temp, store, p) = setup();
    let duplicate = request(
        &store,
        json!({"op":"project_create","input":{"name":"duplicate","path":temp.path()}}),
    )
    .unwrap_err();
    assert_eq!(duplicate.code, "ALREADY_EXISTS");
    assert!(request(
        &store,
        json!({"op":"project_update","id":p,"patch":{"path":temp.path().join("missing")}})
    )
    .is_err());
    assert!(serde_json::from_value::<Request>(json!({"op":"command_create","project":p,"input":{"name":"x","base":"echo ok","unexpected":true}})).is_err());
    assert!(request(&store,json!({"op":"command_create","project":p,"input":{"id":"<bad>","name":"x","base":"echo ok"}})).is_err());
}
#[test]
fn parameters_are_validated_and_quoted_as_literal_values() {
    let (_temp, store, p) = setup();
    let created=request(&store,json!({"op":"command_create","project":p,"input":{"name":"参数测试","base":"echo","params":[{"id":"text","label":"文字","flag":"","type":"text","value":"","required":true},{"id":"port","label":"端口","flag":"--port","type":"number","value":"5173","min":1,"max":65535},{"id":"open","label":"打开","flag":"--open","type":"boolean","value":false}]}})).unwrap();
    let c = store.command(&p, created["id"].as_str().unwrap()).unwrap();
    assert!(shell::render(&c, &BTreeMap::new()).is_err());
    let dangerous = "a'; Write-Output injected; $HOME `whoami` & echo b";
    let values = BTreeMap::from([
        ("text".into(), json!(dangerous)),
        ("open".into(), json!(true)),
    ]);
    let rendered = shell::render(&c, &values).unwrap();
    assert!(rendered.contains(&shell::quote(dangerous)));
    assert!(rendered.ends_with("--open"));
    let invalid = BTreeMap::from([("unknown".into(), json!(1))]);
    assert!(shell::render(&c, &invalid).is_err());
    let invalid = BTreeMap::from([("text".into(), json!("ok")), ("port".into(), json!(0))]);
    assert!(shell::render(&c, &invalid).is_err());
}
fn wait(manager: &TerminalManager, info: &SessionInfo, timeout: Duration) -> (SessionInfo, String) {
    let start = Instant::now();
    let mut seq = 0;
    let mut bytes = vec![];
    loop {
        let snapshot = manager.snapshot(&info.id, seq).unwrap();
        seq = snapshot.next_sequence;
        for chunk in snapshot.chunks {
            let data = STANDARD.decode(chunk).unwrap();
            if data.windows(4).any(|w| w == b"\x1b[6n") {
                let _ = manager.write(&info.id, "\x1b[1;1R");
            }
            bytes.extend(data);
        }
        if snapshot.session.finished {
            return (
                snapshot.session,
                String::from_utf8_lossy(&bytes).into_owned(),
            );
        }
        if start.elapsed() > timeout {
            manager.close_all();
            panic!(
                "PTY did not finish; output={}",
                String::from_utf8_lossy(&bytes)
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
#[test]
fn real_pty_output_unicode_cwd_and_exit_status() {
    let (temp, store, p) = setup();
    let base = if cfg!(windows) {
        "Write-Output '终端中文✓'; [Console]::WriteLine((Get-Location).Path); exit 7"
    } else {
        "printf '终端中文✓\\n'; pwd; exit 7"
    };
    let c = command(&store, &p, base);
    let manager = TerminalManager::default();
    let info = manager
        .start(
            &store,
            StartRequest {
                project: p.clone(),
                command: Some(c),
                values: BTreeMap::new(),
                if_revision: Some(1),
                cols: 100,
                rows: 30,
            },
        )
        .unwrap();
    let (end, output) = wait(&manager, &info, Duration::from_secs(15));
    assert_eq!(end.exit_code, Some(7));
    assert!(output.contains("终端中文✓"), "{output}");
    assert!(
        output.contains(temp.path().file_name().unwrap().to_str().unwrap()),
        "{output}"
    );
    manager.close_all();
}
#[test]
fn interactive_pty_input_resize_and_force_stop() {
    let (_temp, store, p) = setup();
    let manager = TerminalManager::default();
    let info = manager
        .start(
            &store,
            StartRequest {
                project: p.clone(),
                command: None,
                values: BTreeMap::new(),
                if_revision: None,
                cols: 100,
                rows: 30,
            },
        )
        .unwrap();
    manager.resize(&info.id, 80, 24).unwrap();
    manager.write(&info.id, "\x1b[1;1R").unwrap();
    manager
        .write(&info.id, "echo interactive-ok\r\nexit\r\n")
        .unwrap();
    let (_end, output) = wait(&manager, &info, Duration::from_secs(15));
    assert!(output.contains("interactive-ok"), "{output}");
    let c = command(
        &store,
        &p,
        if cfg!(windows) {
            "Start-Sleep -Seconds 60"
        } else {
            "sleep 60"
        },
    );
    let running = manager
        .start(
            &store,
            StartRequest {
                project: p.clone(),
                command: Some(c),
                values: BTreeMap::new(),
                if_revision: None,
                cols: 100,
                rows: 30,
            },
        )
        .unwrap();
    manager.write(&running.id, "\x1b[1;1R").unwrap();
    manager.stop(&running.id, true).unwrap();
    let (end, _) = wait(&manager, &running, Duration::from_secs(10));
    assert!(end.stopped);
    assert_ne!(end.exit_code, Some(0));
    manager.close_all();
}

#[test]
fn parameter_shell_syntax_stays_literal_during_real_execution() {
    let (temp, store, p) = setup();
    let sentinel = temp.path().join("parameter-must-not-execute.txt");
    let literal = if cfg!(windows) {
        format!(
            "@x,a'; Set-Content '{}' injected; # $HOME",
            sentinel.display()
        )
    } else {
        format!("x'; touch '{}'; # $HOME", sentinel.display())
    };
    let created=request(&store,json!({"op":"command_create","project":p,"input":{"name":"literal","base":if cfg!(windows){"Write-Output"}else{"printf '%s\\n'"},"params":[{"id":"value","label":"value","flag":"","type":"text","value":literal}]}})).unwrap();
    let manager = TerminalManager::default();
    let info = manager
        .start(
            &store,
            StartRequest {
                project: p,
                command: Some(created["id"].as_str().unwrap().into()),
                values: BTreeMap::new(),
                if_revision: None,
                cols: 500,
                rows: 30,
            },
        )
        .unwrap();
    let (end, output) = wait(&manager, &info, Duration::from_secs(15));
    assert_eq!(end.exit_code, Some(0));
    assert!(output.contains(&literal), "{output}");
    assert!(!sentinel.exists());
    manager.close_all();
}

#[cfg(windows)]
#[test]
fn stopping_a_session_terminates_its_child_process_tree() {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::System::Threading::*;
    let (temp, store, p) = setup();
    let marker = temp.path().join("child.pid");
    let base=format!("$child = Start-Process -FilePath (Join-Path $PSHOME 'pwsh.exe') -ArgumentList '-NoProfile','-Command','Start-Sleep -Seconds 60' -WindowStyle Hidden -PassThru; Set-Content -LiteralPath {} -Value $child.Id; Start-Sleep -Seconds 60",shell::quote(&marker.to_string_lossy()));
    // This machine has pwsh; fall back to Windows PowerShell when testing a machine without it.
    let base = if std::env::var_os("PATH")
        .is_some_and(|v| std::env::split_paths(&v).any(|p| p.join("pwsh.exe").is_file()))
    {
        base
    } else {
        base.replace("'pwsh.exe'", "'powershell.exe'")
    };
    let c = command(&store, &p, &base);
    let manager = TerminalManager::default();
    let info = manager
        .start(
            &store,
            StartRequest {
                project: p,
                command: Some(c),
                values: BTreeMap::new(),
                if_revision: None,
                cols: 100,
                rows: 30,
            },
        )
        .unwrap();
    manager.write(&info.id, "\x1b[1;1R").unwrap();
    let started = Instant::now();
    while !marker.exists() && started.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(20));
    }
    let pid: u32 = std::fs::read_to_string(&marker)
        .expect("Child process should write PID")
        .trim()
        .trim_start_matches('\u{feff}')
        .parse()
        .unwrap();
    // SAFETY: PID is from this test's child; keep a handle so PID reuse cannot affect the assertion.
    let child = unsafe {
        let raw = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        assert!(!raw.is_null());
        OwnedHandle::from_raw_handle(raw)
    };
    manager.stop(&info.id, true).unwrap();
    wait(&manager, &info, Duration::from_secs(10));
    let mut exit = 259;
    // SAFETY: child is a live owned process handle, and exit points to initialized writable memory.
    assert_ne!(
        unsafe { GetExitCodeProcess(child.as_raw_handle(), &mut exit) },
        0
    );
    assert_ne!(exit, 259, "Descendant must not remain running");
    manager.close_all();
}
