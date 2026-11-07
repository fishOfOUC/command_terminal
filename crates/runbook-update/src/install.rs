use crate::package::{extract, no_links, replace, safe_name, verify_archive};
use anyhow::{bail, ensure, Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub(crate) const STATE: &str = ".runbook-update";

fn lock_file(path: &Path) -> Result<File> {
    Ok(OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)?)
}

pub(crate) fn state_dir(app: &Path) -> Result<PathBuf> {
    no_links(app, STATE)?;
    let dir = app.join(STATE);
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub(crate) fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut temp = tempfile::NamedTempFile::new_in(path.parent().context("无效状态路径")?)?;
    serde_json::to_writer(&mut temp, value)?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Plan {
    pub app: PathBuf,
    pub stage: PathBuf,
    pub data: PathBuf,
    pub project: Option<String>,
    pub token: String,
}

#[derive(Serialize, Deserialize)]
struct Journal {
    plan: Plan,
    phase: String,
    files: Vec<(String, bool)>,
}

fn journal_path(app: &Path) -> PathBuf {
    app.join(STATE).join("transaction.json")
}

fn validate_plan(plan: &Plan) -> Result<()> {
    ensure!(
        plan.app.is_absolute() && dunce::canonicalize(&plan.app)? == plan.app,
        "程序路径无效"
    );
    ensure!(
        plan.stage.parent() == Some(plan.app.join(STATE).as_path()),
        "更新暂存路径无效"
    );
    ensure!(
        plan.stage
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.starts_with("download-")),
        "更新目录名称无效"
    );
    no_links(&plan.app, STATE)?;
    ensure!(
        plan.token.len() == 36 && uuid::Uuid::parse_str(&plan.token).is_ok(),
        "更新令牌无效"
    );
    ensure!(plan.data.is_absolute(), "数据目录必须是绝对路径");
    Ok(())
}

fn restore(journal: &Journal) -> Result<()> {
    validate_plan(&journal.plan)?;
    for (name, existed) in journal.files.iter().rev() {
        ensure!(safe_name(name), "恢复日志包含无效路径");
        no_links(&journal.plan.app, name)?;
        let destination = journal.plan.app.join(name);
        if *existed {
            replace(&journal.plan.stage.join("backup").join(name), &destination)?;
        } else if destination.is_file() {
            fs::remove_file(destination)?;
        }
    }
    Ok(())
}

fn record(app: &Path, message: &str) -> Result<()> {
    write_json(&app.join(STATE).join("last-result.json"), &message)
}

fn cleanup(journal: &Journal) {
    // A running helper on Windows can keep its own directory locked. Retry on next startup.
    let removed = match fs::remove_dir_all(&journal.plan.stage) {
        Ok(()) => true,
        Err(error) => error.kind() == std::io::ErrorKind::NotFound,
    };
    if removed {
        let _ = fs::remove_file(journal_path(&journal.plan.app));
    }
}

pub struct RuntimeGuard {
    _file: File,
}
#[derive(Debug)]
struct RecoveryScheduled;
impl std::fmt::Display for RecoveryScheduled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("正在恢复上次中断的更新，恢复后将重新打开 Runbook")
    }
}
impl std::error::Error for RecoveryScheduled {}
pub fn is_recovery_scheduled(error: &anyhow::Error) -> bool {
    error.downcast_ref::<RecoveryScheduled>().is_some()
}
impl RuntimeGuard {
    pub fn acquire(app: &Path) -> Result<Self> {
        let state = state_dir(app)?;
        let file = lock_file(&app.join(".runbook-runtime.lock"))?;
        let startup_operation = lock_file(&state.join("operation.lock"))?;
        let mut candidate = false;
        if journal_path(app).exists() {
            let journal: Journal = serde_json::from_slice(&fs::read(journal_path(app))?)?;
            validate_plan(&journal.plan)?;
            ensure!(journal.plan.app == app, "更新恢复目录不匹配");
            let health = std::env::var("RUNBOOK_UPDATE_TOKEN").ok();
            candidate =
                journal.phase == "testing" && health.as_deref() == Some(&journal.plan.token);
            if !candidate {
                let operation = lock_file(&state.join("operation.lock"))?;
                FileExt::try_lock_exclusive(&operation)
                    .context("正在安装更新，请稍后启动 Runbook")?;
                if journal.phase != "committed" && journal.phase != "rolled-back" {
                    // The running executable is mapped on Windows. A separate helper must restore it
                    // after this process exits; never attempt to overwrite ourselves in-process.
                    FileExt::try_lock_shared(&file).context("恢复更新前仍有文件替换正在进行")?;
                    let mut command = Command::new(journal.plan.stage.join("helper.exe"));
                    command.arg("--recover").arg(journal_path(app));
                    spawn_detached(&mut command)
                        .context("无法启动恢复程序；请保留更新备份并手动修复")?;
                    // Retain the shared lock until process exit, so the helper waits for our image
                    // and every other runtime to close. Windows releases the handle on exit.
                    std::mem::forget(file);
                    return Err(RecoveryScheduled.into());
                }
                cleanup(&journal);
            }
        }
        if !candidate {
            FileExt::try_lock_shared(&startup_operation)
                .context("正在准备更新，请稍后启动 Runbook")?;
        }
        FileExt::try_lock_shared(&file).context("Runbook 正在替换更新文件，请稍后重试")?;
        Ok(Self { _file: file })
    }
}

pub fn acknowledge(app: &Path) -> Result<()> {
    let Ok(token) = std::env::var("RUNBOOK_UPDATE_TOKEN") else {
        return Ok(());
    };
    if !journal_path(app).exists() {
        return Ok(());
    }
    let journal: Journal = serde_json::from_slice(&fs::read(journal_path(app))?)?;
    validate_plan(&journal.plan)?;
    ensure!(
        journal.plan.app == app && journal.plan.token == token,
        "更新确认令牌不匹配"
    );
    if journal.phase == "committed" {
        return Ok(());
    }
    ensure!(journal.phase == "testing", "更新状态不允许启动确认");
    let manifest = crate::verify_manifest(
        &fs::read(journal.plan.stage.join("manifest.json"))?,
        crate::PUBLIC_KEY,
    )?;
    ensure!(
        manifest.version == crate::CURRENT_VERSION,
        "新版程序版本与更新清单不一致"
    );
    write_json(&journal.plan.stage.join("ready.json"), &token)?;
    Ok(())
}

pub(crate) fn spawn_detached(command: &mut Command) -> Result<Child> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // A GUI launch must not become a terminal-owned child job or inherit its console.
        command.creation_flags(0x00000008 | 0x00000200 | 0x01000000); // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB
    }
    command.spawn().map_err(Into::into)
}

fn launch(plan: &Plan, health: bool) -> Result<Child> {
    let mut command = Command::new(plan.app.join("Runbook-Desktop.exe"));
    command
        .current_dir(&plan.app)
        .env("RUNBOOK_DATA_DIR", &plan.data);
    command.env_remove("RUNBOOK_UPDATE_TOKEN");
    if let Some(project) = &plan.project {
        command.arg("--project").arg(project);
    }
    if health {
        command.env("RUNBOOK_UPDATE_TOKEN", &plan.token);
    }
    spawn_detached(&mut command)
}

fn exclusive(file: &File, seconds: u64) -> Result<()> {
    let start = Instant::now();
    loop {
        if FileExt::try_lock_exclusive(file).is_ok() {
            return Ok(());
        }
        ensure!(
            start.elapsed() < Duration::from_secs(seconds),
            "其他 Runbook 窗口或 CLI 仍在使用程序，请关闭后重试"
        );
        thread::sleep(Duration::from_millis(100));
    }
}

fn install(plan: Plan) -> Result<()> {
    validate_plan(&plan)?;
    no_links(
        &plan.app,
        &format!(
            "{STATE}/{}",
            plan.stage.file_name().unwrap().to_string_lossy()
        ),
    )?;
    let state = state_dir(&plan.app)?;
    let operation = lock_file(&state.join("operation.lock"))?;
    FileExt::try_lock_exclusive(&operation).context("另一个更新器正在运行")?;
    ensure!(
        !journal_path(&plan.app).exists(),
        "上次更新尚未清理，请重启应用后重试"
    );
    let manifest = crate::verify_manifest(
        &fs::read(plan.stage.join("manifest.json"))?,
        crate::PUBLIC_KEY,
    )?;
    ensure!(
        crate::manifest::newer(&manifest.version)?,
        "更新版本必须高于当前版本"
    );
    verify_archive(&plan.stage.join("package.zip"), &manifest)?;
    let names = extract(&plan.stage.join("package.zip"), &plan.stage.join("new"))?;
    for name in &names {
        no_links(&plan.app, name)?;
        let target = plan.app.join(name);
        ensure!(
            !target.exists() || target.is_file(),
            "更新目标不是文件: {name}"
        );
        if target.exists() {
            ensure!(
                !fs::metadata(&target)?.permissions().readonly(),
                "更新目标为只读文件: {name}"
            );
        }
    }
    write_json(&plan.stage.join("accepted.json"), &plan.token)?;
    let file = lock_file(&plan.app.join(".runbook-runtime.lock"))?;
    exclusive(&file, 60)?;
    let mut journal = Journal {
        plan: plan.clone(),
        phase: "replacing".into(),
        files: Vec::new(),
    };
    // Make complete backups before publishing the transaction journal or changing any live file.
    for name in &names {
        no_links(&plan.app, name)?;
        let original = plan.app.join(name);
        ensure!(
            !original.exists() || original.is_file(),
            "更新目标不是文件: {name}"
        );
        if original.is_file() {
            ensure!(
                !fs::metadata(&original)?.permissions().readonly(),
                "更新目标为只读文件: {name}"
            );
            replace(&original, &plan.stage.join("backup").join(name))?;
        }
        journal.files.push((name.clone(), original.exists()));
    }
    write_json(&journal_path(&plan.app), &journal)?;
    let mut launched = None;
    let result = (|| -> Result<()> {
        for name in &names {
            replace(&plan.stage.join("new").join(name), &plan.app.join(name))?;
        }
        journal.phase = "testing".into();
        write_json(&journal_path(&plan.app), &journal)?;
        FileExt::unlock(&file)?;
        launched = Some(launch(&plan, true)?);
        let child = launched.as_mut().unwrap();
        let start = Instant::now();
        loop {
            if let Ok(bytes) = fs::read(plan.stage.join("ready.json")) {
                if serde_json::from_slice::<String>(&bytes).ok().as_deref() == Some(&plan.token) {
                    journal.phase = "committed".into();
                    write_json(&journal_path(&plan.app), &journal)?;
                    let _ = record(&plan.app, &format!("已成功更新到 {}", manifest.version));
                    return Ok(());
                }
            }
            if child.try_wait()?.is_some() {
                bail!("新版启动失败，已准备恢复旧版");
            }
            if start.elapsed() > Duration::from_secs(45) {
                let _ = child.kill();
                let _ = child.wait();
                bail!("新版未完成启动确认，已准备恢复旧版");
            }
            thread::sleep(Duration::from_millis(100));
        }
    })();
    if let Err(error) = result {
        if let Some(mut child) = launched {
            let _ = child.kill();
            let _ = child.wait();
        }
        exclusive(&file, 15)?;
        restore(&journal).context("恢复旧版失败；保留了更新备份，请关闭占用后重启应用恢复")?;
        journal.phase = "rolled-back".into();
        write_json(&journal_path(&plan.app), &journal)?;
        record(&plan.app, &format!("更新失败，已恢复旧版：{error:#}"))?;
        FileExt::unlock(&file)?;
        FileExt::unlock(&operation)?;
        let _ = launch(&plan, false);
        return Err(error.context("更新失败，已恢复旧版"));
    }
    cleanup(&journal);
    Ok(())
}

pub fn run_plan(path: &Path) -> Result<()> {
    let plan: Plan = serde_json::from_slice(&fs::read(path)?)?;
    validate_plan(&plan)?;
    let app = plan.app.clone();
    let result = install(plan);
    if let Err(error) = &result {
        let _ = record(&app, &format!("更新失败：{error:#}"));
    }
    result
}

pub fn run_recovery(path: &Path) -> Result<()> {
    let mut journal: Journal = serde_json::from_slice(&fs::read(path)?)?;
    validate_plan(&journal.plan)?;
    ensure!(
        path == journal_path(&journal.plan.app),
        "恢复日志路径不匹配"
    );
    let state = state_dir(&journal.plan.app)?;
    let operation = lock_file(&state.join("operation.lock"))?;
    // The process requesting recovery is exiting and briefly still owns this lock.
    exclusive(&operation, 15)?;
    journal = serde_json::from_slice(&fs::read(path)?)?;
    validate_plan(&journal.plan)?;
    if journal.phase == "committed" || journal.phase == "rolled-back" {
        return Ok(());
    }
    let file = lock_file(&journal.plan.app.join(".runbook-runtime.lock"))?;
    let result = (|| -> Result<()> {
        exclusive(&file, 60)?;
        if journal.phase != "committed" && journal.phase != "rolled-back" {
            restore(&journal)?;
            journal.phase = "rolled-back".into();
            write_json(path, &journal)?;
            record(&journal.plan.app, "上次更新中断，已恢复旧版程序")?;
        }
        FileExt::unlock(&file)?;
        FileExt::unlock(&operation)?;
        launch(&journal.plan, false)?;
        Ok(())
    })();
    if let Err(error) = &result {
        let _ = record(
            &journal.plan.app,
            &format!("中断恢复失败，保留了备份：{error:#}"),
        );
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interrupted_transaction_restores_files_and_preserves_data() {
        let dir = tempfile::tempdir().unwrap();
        let app = dunce::canonicalize(dir.path()).unwrap();
        let stage = state_dir(&app).unwrap().join("download-recovery-test");
        fs::create_dir_all(stage.join("backup")).unwrap();
        fs::create_dir_all(app.join("data")).unwrap();
        fs::write(app.join("data/runbook.db"), b"unchanged database").unwrap();
        fs::write(app.join("runbook.exe"), b"partially updated").unwrap();
        fs::write(stage.join("backup/runbook.exe"), b"original CLI").unwrap();
        fs::write(app.join("README.md"), b"new file").unwrap();
        let journal = Journal {
            plan: Plan {
                app: app.clone(),
                stage,
                data: app.join("data"),
                project: None,
                token: uuid::Uuid::new_v4().to_string(),
            },
            phase: "replacing".into(),
            files: vec![("runbook.exe".into(), true), ("README.md".into(), false)],
        };
        write_json(&journal_path(&app), &journal).unwrap();
        restore(&journal).unwrap();
        let journal = Journal {
            phase: "rolled-back".into(),
            ..journal
        };
        write_json(&journal_path(&app), &journal).unwrap();
        assert_eq!(fs::read(app.join("runbook.exe")).unwrap(), b"original CLI");
        assert!(!app.join("README.md").exists());
        assert_eq!(
            fs::read(app.join("data/runbook.db")).unwrap(),
            b"unchanged database"
        );
        let _guard = RuntimeGuard::acquire(&app).unwrap();
        assert!(!journal_path(&app).exists());
    }
    #[test]
    fn preparation_blocks_new_app_instances() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_dir(dir.path()).unwrap();
        let operation = lock_file(&state.join("operation.lock")).unwrap();
        FileExt::try_lock_exclusive(&operation).unwrap();
        assert!(RuntimeGuard::acquire(dir.path()).is_err());
    }
    #[test]
    fn finished_transaction_does_not_block_cli_while_new_desktop_is_running() {
        let dir = tempfile::tempdir().unwrap();
        let app = dunce::canonicalize(dir.path()).unwrap();
        let stage = state_dir(&app).unwrap().join("download-completed");
        fs::create_dir(&stage).unwrap();
        // Simulate a crash after removing the cache but before deleting the final journal.
        fs::remove_dir(&stage).unwrap();
        let desktop = RuntimeGuard::acquire(&app).unwrap();
        let journal = Journal {
            plan: Plan {
                app: app.clone(),
                stage,
                data: app.join("data"),
                project: None,
                token: uuid::Uuid::new_v4().to_string(),
            },
            phase: "committed".into(),
            files: vec![],
        };
        write_json(&journal_path(&app), &journal).unwrap();
        let cli = RuntimeGuard::acquire(&app).unwrap();
        assert!(!journal_path(&app).exists());
        drop((desktop, cli));
    }
}
