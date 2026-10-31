use serde_json::{json, Value};
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

fn cli(dir: &Path, args: &[&str], input: Option<Value>) -> (i32, Value) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_runbook"))
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
