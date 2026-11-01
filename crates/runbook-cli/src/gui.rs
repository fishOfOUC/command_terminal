use runbook_core::{store::directory, ApiError, Result, Store};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

fn target_directory(path: Option<PathBuf>) -> Result<String> {
    let path = path.unwrap_or(std::env::current_dir()?);
    let raw = path.to_string_lossy();
    let path =
        if path.is_absolute() || raw == "~" || raw.starts_with("~/") || raw.starts_with("~\\") {
            path
        } else {
            std::env::current_dir()?.join(path)
        };
    directory(&path.to_string_lossy())
}

fn desktop_executable(cli: &Path) -> Result<PathBuf> {
    let parent = cli
        .parent()
        .ok_or_else(|| ApiError::new("IO_ERROR", "CLI has no parent directory"))?;
    for name in [
        format!("Runbook-Desktop{}", std::env::consts::EXE_SUFFIX),
        format!("runbook-desktop{}", std::env::consts::EXE_SUFFIX),
    ] {
        let candidate = parent.join(name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(ApiError::new(
        "GUI_NOT_FOUND",
        format!("Runbook desktop executable not found beside the CLI in {}. Build or copy the complete dist directory.", parent.display()),
    ))
}

fn launch_command(desktop: &Path, data_dir: &Path, project: &str, cwd: &str) -> Command {
    let mut command = Command::new(desktop);
    command
        .arg("--project")
        .arg(project)
        .env("RUNBOOK_DATA_DIR", data_dir)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW: do not attach to the caller's console.
    }
    command
}

pub fn open(data_dir: PathBuf, path: Option<PathBuf>) -> Result<Value> {
    let target = target_directory(path)?;
    // Check packaging before creating any project or database.
    let desktop = desktop_executable(&std::env::current_exe()?)?;
    let data_dir = if data_dir.is_absolute() {
        data_dir
    } else {
        std::env::current_dir()?.join(data_dir)
    };
    let store = Store::open_application(data_dir)?;
    let data_dir = PathBuf::from(directory(
        &store
            .path()
            .parent()
            .expect("database parent")
            .to_string_lossy(),
    )?);
    let (project, created) = store.open_project_directory(&target)?;
    #[cfg(windows)]
    let handles = crate::gui_handles::StandardHandles::protect()?;
    let mut command = launch_command(&desktop, &data_dir, &project.id, &target);
    #[cfg(windows)]
    let child = {
        use std::os::windows::process::CommandExt;
        // Leave a Runbook terminal's job so closing that terminal does not close the new GUI.
        command.creation_flags(0x08000000 | 0x01000000); // CREATE_NO_WINDOW | CREATE_BREAKAWAY_FROM_JOB
        match command.spawn() {
            // External launchers may use a restrictive job that does not permit breakaway.
            Err(error) if error.raw_os_error() == Some(5) => {
                command.creation_flags(0x08000000).spawn()?
            }
            result => result?,
        }
    };
    #[cfg(not(windows))]
    let child = command.spawn()?;
    #[cfg(windows)]
    drop(handles);
    Ok(json!({
        "project": project, "created": created, "directory": target,
        "pid": child.id(), "desktop": desktop
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_resolution_and_launch_preserve_project_database_and_cwd() {
        let temp = tempfile::tempdir().unwrap();
        let cli = temp.path().join("runbook.exe");
        assert_eq!(desktop_executable(&cli).unwrap_err().code, "GUI_NOT_FOUND");
        let desktop = temp
            .path()
            .join(format!("runbook-desktop{}", std::env::consts::EXE_SUFFIX));
        std::fs::write(&desktop, []).unwrap();
        assert_eq!(
            std::fs::canonicalize(desktop_executable(&cli).unwrap()).unwrap(),
            std::fs::canonicalize(&desktop).unwrap()
        );
        let db = temp.path().join("独立数据 with spaces");
        let command = launch_command(&desktop, &db, "project-123", "目录 with spaces");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            ["--project", "project-123"]
        );
        assert_eq!(
            command.get_current_dir(),
            Some(Path::new("目录 with spaces"))
        );
        assert!(command
            .get_envs()
            .any(|(key, value)| key == "RUNBOOK_DATA_DIR" && value == Some(db.as_os_str())));
    }
}
