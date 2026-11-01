use serde_json::{json, Value};
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

fn cli(dir: &Path, args: &[&str], input: Option<Value>) -> (i32, Value) {
    cli_at(dir, args, input, None)
}
fn cli_at(dir: &Path, args: &[&str], input: Option<Value>, cwd: Option<&Path>) -> (i32, Value) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_runbook"));
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let mut child = command
        .arg("--data-dir")
        .arg(dir)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(value) = input {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(value.to_string().as_bytes())
            .unwrap();
    } else {
        child.stdin.take();
    }
    let out = child.wait_with_output().unwrap();
    assert!(
        out.stderr.is_empty(),
        "stderr must be clean: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    (
        out.status.code().unwrap(),
        serde_json::from_slice(&out.stdout)
            .unwrap_or_else(|_| panic!("Invalid output: {}", String::from_utf8_lossy(&out.stdout))),
    )
}
#[test]
fn add_project_uses_real_directory_name_and_preserves_existing_projects() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("db");
    let work = temp.path().join("真实项目 with spaces");
    std::fs::create_dir(&work).unwrap();
    let (code, created) = cli_at(&db, &["project", "add"], None, Some(&work));
    assert_eq!(code, 0, "{created}");
    assert_eq!(created["data"]["name"], "真实项目 with spaces");
    assert_eq!(created["data"]["description"], "");
    assert_eq!(created["data"]["commands"], json!([]));
    let id = created["data"]["id"].as_str().unwrap();
    let (code, duplicate) = cli_at(
        &db,
        &["project", "add", ".", "--name", "不要覆盖"],
        None,
        Some(&work),
    );
    assert_eq!(code, 4);
    assert_eq!(duplicate["error"]["details"]["projectId"], id);
    let (code, _) = cli(
        &db,
        &["project", "update", "--id", id, "--archived", "true"],
        None,
    );
    assert_eq!(code, 0);
    let (code, _) = cli_at(
        &db,
        &["project", "add", "真实项目 with spaces"],
        None,
        Some(temp.path()),
    );
    assert_eq!(code, 4);
    let (_, existing) = cli(&db, &["project", "get", "--id", id], None);
    assert_eq!(existing["data"]["name"], "真实项目 with spaces");
    assert_eq!(existing["data"]["archived"], true);
    let other = temp.path().join("other");
    std::fs::create_dir(&other).unwrap();
    let (code, custom) = cli_at(
        &db,
        &[
            "project",
            "add",
            "other",
            "--name",
            "自定义名称",
            "--description",
            "说明",
        ],
        None,
        Some(temp.path()),
    );
    assert_eq!(code, 0);
    assert_eq!(custom["data"]["name"], "自定义名称");
    assert_eq!(custom["data"]["description"], "说明");
    let (code, missing) = cli_at(&db, &["project", "add", "missing"], None, Some(temp.path()));
    assert_eq!(code, 2);
    assert_eq!(missing["error"]["details"]["field"], "path");
    let (_, list) = cli(&db, &["project", "list"], None);
    assert_eq!(list["data"]["total"], 3); // includes the built-in Runbook project
}
#[test]
fn gui_errors_are_json_and_do_not_create_projects_or_missing_directories() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("db");
    let (code, error) = cli_at(&db, &["gui", "missing"], None, Some(temp.path()));
    assert_eq!(code, 2, "{error}");
    assert_eq!(error["error"]["details"]["field"], "path");
    assert!(!temp.path().join("missing").exists());
    assert!(!db.exists());

    // An independently copied CLI reports missing desktop packaging before any database writes.
    let standalone = temp
        .path()
        .join(format!("runbook{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(env!("CARGO_BIN_EXE_runbook"), &standalone).unwrap();
    let out = Command::new(standalone)
        .current_dir(temp.path())
        .arg("--data-dir")
        .arg(&db)
        .arg("gui")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stderr.is_empty());
    let error: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(error["error"]["code"], "GUI_NOT_FOUND");
    assert!(!db.exists());
}
#[test]
fn cli_machine_contract_crud_stdin_schema_and_errors() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("db");
    let (code, schema) = cli(&db, &["schema"], None);
    assert_eq!(code, 0);
    assert!(schema["data"]["requestSchema"].is_object());
    let (code, p) = cli(
        &db,
        &["project", "create", "--input", "-"],
        Some(json!({"id":"ai-project","name":"AI 项目","path":temp.path()})),
    );
    assert_eq!(code, 0);
    assert_eq!(p["data"]["id"], "ai-project");
    let (code, c) = cli(
        &db,
        &[
            "command",
            "create",
            "--project",
            "ai-project",
            "--input",
            "-",
        ],
        Some(json!({"id":"hello","name":"中文命令","base":"echo hello"})),
    );
    assert_eq!(code, 0);
    assert_eq!(c["data"]["revision"], 1);
    let (code, _) = cli(
        &db,
        &[
            "command",
            "update",
            "--project",
            "ai-project",
            "--id",
            "hello",
            "--if-revision",
            "1",
            "--base",
            "echo changed",
        ],
        None,
    );
    assert_eq!(code, 0);
    let (code, conflict) = cli(
        &db,
        &[
            "command",
            "delete",
            "--project",
            "ai-project",
            "--id",
            "hello",
            "--if-revision",
            "1",
        ],
        None,
    );
    assert_eq!(code, 4);
    assert_eq!(conflict["error"]["code"], "REVISION_CONFLICT");
    let (code, list) = cli(
        &db,
        &["api", "--input", "-"],
        Some(json!({"op":"command_list","project":"ai-project","query":"changed"})),
    );
    assert_eq!(code, 0);
    assert_eq!(list["data"]["total"], 1);
    let (code, get) = cli(
        &db,
        &["command", "get", "--project", "ai-project", "--id", "hello"],
        None,
    );
    assert_eq!(code, 0);
    assert_eq!(get["data"]["name"], "中文命令");
    let (code, _) = cli(
        &db,
        &[
            "command",
            "delete",
            "--project",
            "ai-project",
            "--id",
            "hello",
            "--if-revision",
            "2",
        ],
        None,
    );
    assert_eq!(code, 0);
    let (code, error) = cli(
        &db,
        &["command", "get", "--project", "ai-project", "--id", "hello"],
        None,
    );
    assert_eq!(code, 3);
    assert_eq!(error["error"]["code"], "NOT_FOUND");
    let (code, error) = cli(&db, &["nonsense"], None);
    assert_eq!(code, 2);
    assert_eq!(error["error"]["code"], "INVALID_ARGUMENT");
    let (code, error) = cli(
        &db,
        &["api", "--input", "-"],
        Some(json!({"op":"project_get","id":"ai-project","typo":true})),
    );
    assert_eq!(code, 2);
    assert_eq!(error["error"]["code"], "INVALID_JSON");
}
#[test]
fn schema_discovery_is_database_independent_and_scoped() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("must-not-be-created");
    let (code, full) = cli(&db, &["schema"], None);
    assert_eq!(code, 0, "{full}");
    assert!(full["data"]["projectSchema"].is_object());
    assert!(full["data"]["commandSchema"].is_object());
    assert!(!db.exists(), "schema must not create a data directory");

    // Discovery must work even when this data directory cannot be opened.
    let blocked = temp.path().join("regular-file");
    std::fs::write(&blocked, "leave this file unchanged").unwrap();
    let (code, list) = cli(&blocked, &["schema", "--list"], None);
    assert_eq!(code, 0, "{list}");
    let operations = list["data"]["operations"].as_array().unwrap();
    assert!(operations.contains(&json!("command_create")));
    assert!(!operations.contains(&json!("command_run")));
    assert!(list.to_string().len() < full.to_string().len() / 10);
    for operation in operations {
        let operation = operation.as_str().unwrap();
        let (code, scoped) = cli(&blocked, &["schema", "--op", operation], None);
        assert_eq!(code, 0, "{scoped}");
        assert_eq!(
            scoped["data"]["requestSchema"]["properties"]["op"]["const"],
            operation
        );
        assert!(scoped["data"].get("projectSchema").is_none());
        assert!(scoped["data"].get("commandSchema").is_none());
        let (code, api) = cli(
            &blocked,
            &["api", "--input", "-"],
            Some(json!({
                "op":"schema", "operation":operation
            })),
        );
        assert_eq!(code, 0, "{api}");
        assert_eq!(api, scoped);
    }
    let (code, api) = cli(
        &blocked,
        &["api", "--input", "-"],
        Some(json!({"op":"schema"})),
    );
    assert_eq!(code, 0, "{api}");
    assert_eq!(api, full);
    let (code, api_list) = cli(
        &blocked,
        &["api", "--input", "-"],
        Some(json!({"op":"schema","list":true})),
    );
    assert_eq!(code, 0, "{api_list}");
    assert_eq!(api_list, list);
    assert_eq!(
        std::fs::read_to_string(blocked).unwrap(),
        "leave this file unchanged"
    );
}

#[test]
fn schema_invalid_filters_are_json_errors_without_database_access() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("must-not-be-created");
    for args in [
        vec!["schema", "--op", "unknown"],
        vec!["schema", "--op", "info", "--list"],
    ] {
        let (code, response) = cli(&db, &args, None);
        assert_eq!(code, 2, "{response}");
        assert_eq!(response["error"]["code"], "INVALID_ARGUMENT");
    }
    let (code, response) = cli(
        &db,
        &["api", "--input", "-"],
        Some(json!({
            "op":"schema", "operation":"info", "list":true
        })),
    );
    assert_eq!(code, 2, "{response}");
    assert_eq!(response["error"]["code"], "INVALID_ARGUMENT");
    assert!(!db.exists());
}

#[test]
fn explicit_execution_reports_real_exit_code_and_timeout() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("db");
    cli(
        &db,
        &["project", "create", "--input", "-"],
        Some(json!({"id":"p","name":"run","path":temp.path()})),
    );
    let base = if cfg!(windows) {
        "Write-Output 'CLI真实输出'; exit 7"
    } else {
        "printf 'CLI真实输出\\n'; exit 7"
    };
    cli(
        &db,
        &["command", "create", "--project", "p", "--input", "-"],
        Some(json!({"id":"fail","name":"fail","base":base})),
    );
    let (code, out) = cli(
        &db,
        &[
            "command",
            "run",
            "--project",
            "p",
            "--id",
            "fail",
            "--timeout-ms",
            "10000",
        ],
        None,
    );
    assert_eq!(code, 10, "{out}");
    assert_eq!(out["data"]["exitCode"], 7);
    assert!(out["data"]["output"]
        .as_str()
        .unwrap()
        .contains("CLI真实输出"));
    cli(
        &db,
        &["command", "create", "--project", "p", "--input", "-"],
        Some(
            json!({"id":"slow","name":"slow","base":if cfg!(windows){"Start-Sleep 60"}else{"sleep 60"}}),
        ),
    );
    let (code, out) = cli(
        &db,
        &[
            "command",
            "run",
            "--project",
            "p",
            "--id",
            "slow",
            "--timeout-ms",
            "200",
        ],
        None,
    );
    assert_eq!(code, 124, "{out}");
    assert_eq!(out["data"]["timedOut"], true);
}
