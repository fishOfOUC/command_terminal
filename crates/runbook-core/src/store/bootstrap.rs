use super::{application_dir, path_key, Store};
use crate::{model::*, ApiError, Result};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use std::path::Path;

fn everyday_commands() -> Vec<Command> {
    let t = now();
    [
        (
            "add-project",
            "添加项目",
            "runbook project add",
            "填写要添加的项目目录后执行；相对路径以终端当前目录为准。",
            true,
            "",
        ),
        (
            "open-gui",
            "启动 GUI",
            "runbook gui",
            "打开指定目录对应的项目；留空时使用终端当前目录，没有匹配项目时自动添加。",
            false,
            "",
        ),
    ]
    .into_iter()
    .map(|(id, name, base, description, required, value)| Command {
        id: id.into(),
        name: name.into(),
        base: base.into(),
        description: description.into(),
        params: vec![Parameter {
            id: "path".into(),
            label: "项目目录".into(),
            flag: String::new(),
            kind: ParameterType::Text,
            value: value.into(),
            required,
            min: None,
            max: None,
        }],
        revision: 1,
        created_at: t,
        updated_at: t,
    })
    .collect()
}

impl Store {
    /// Runtime entry point. Low-level `open` remains available for empty-store tools and tests.
    pub fn open_application(dir: impl AsRef<Path>) -> Result<Self> {
        let store = Self::open(dir)?;
        store.initialize_builtin(&application_dir()?)?;
        Ok(store)
    }

    fn initialize_builtin(&self, app_dir: &Path) -> Result<()> {
        let app_dir = super::directory(&app_dir.to_string_lossy())?;
        let mut c = self.connect()?;
        let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let previous: Option<String> = tx
            .query_row(
                "SELECT value FROM meta WHERE key='builtin_app_dir'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        match previous {
            None => {
                let projects = Self::list_from(&tx)?;
                let existing = projects
                    .iter()
                    .find(|p| path_key(&p.path) == path_key(&app_dir));
                let mut project = match existing {
                    Some(project) => project.clone(),
                    None => Self::new_project(ProjectCreate {
                        id: Some(if projects.iter().any(|p| p.id == "runbook-home") {
                            new_id("runbook-home")
                        } else {
                            "runbook-home".into()
                        }),
                        name: "Runbook".into(),
                        path: app_dir.clone(),
                        description: "Runbook 日常操作：添加项目、启动 GUI。".into(),
                        starred: true,
                    })?,
                };
                for command in everyday_commands() {
                    validate_command(&command)?;
                    if !project.commands.iter().any(|c| c.id == command.id) {
                        project.commands.push(command);
                    }
                }
                if existing.is_some() {
                    project.revision += 1;
                    project.updated_at = now();
                }
                Self::save(&tx, &project, existing.is_none())?;
                tx.execute(
                    "INSERT INTO meta(key,value) VALUES('builtin_project_id',?1)",
                    [&project.id],
                )?;
            }
            Some(previous) if path_key(&previous) != path_key(&app_dir) => {
                let id: String = tx.query_row(
                    "SELECT value FROM meta WHERE key='builtin_project_id'",
                    [],
                    |r| r.get(0),
                )?;
                match Self::load(&tx, &id) {
                    Ok(mut project) if path_key(&project.path) == path_key(&previous) => {
                        project.path = app_dir.clone();
                        project.revision += 1;
                        project.updated_at = now();
                        Self::save(&tx, &project, false)?;
                    }
                    Ok(_) => {} // Preserve a directory deliberately edited by the user.
                    Err(ApiError { code, .. }) if code == "NOT_FOUND" => {}
                    Err(error) => return Err(error),
                }
            }
            Some(_) => {} // Do not reset edits or recreate projects/commands deleted by the user.
        }
        tx.execute(
            "INSERT INTO meta(key,value) VALUES('builtin_app_dir',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![app_dir],
        )?;
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initializes_once_and_preserves_edits_deletions_and_other_projects() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().join("data")).unwrap();
        store.initialize_builtin(temp.path()).unwrap();
        let home = store.get("runbook-home").unwrap();
        assert_eq!(
            home.path,
            super::super::directory(&temp.path().to_string_lossy()).unwrap()
        );
        assert_eq!(home.commands.len(), 2);
        assert_eq!(home.commands[0].base, "runbook project add");
        assert!(home.commands[0].params[0].required);
        assert_eq!(home.commands[1].base, "runbook gui");
        store
            .patch_command(
                &home.id,
                "open-gui",
                CommandPatch {
                    description: Some("我的说明".into()),
                    ..Default::default()
                },
                None,
            )
            .unwrap();
        store.delete_command(&home.id, "add-project", None).unwrap();
        store.initialize_builtin(temp.path()).unwrap();
        let home = store.get(&home.id).unwrap();
        assert_eq!(home.commands.len(), 1);
        assert_eq!(home.commands[0].description, "我的说明");
        store.delete_project(&home.id, None).unwrap();
        store.initialize_builtin(temp.path()).unwrap();
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn relocation_rebinds_the_default_project_and_preserves_commands() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first");
        let moved = temp.path().join("移到这里 with spaces");
        std::fs::create_dir(&first).unwrap();
        std::fs::create_dir(&moved).unwrap();
        let store = Store::open(temp.path().join("data")).unwrap();
        store.initialize_builtin(&first).unwrap();
        store
            .create_command(
                "runbook-home",
                CommandCreate {
                    id: Some("mine".into()),
                    name: "我的命令".into(),
                    base: "echo keep".into(),
                    description: String::new(),
                    params: vec![],
                },
            )
            .unwrap();
        store.initialize_builtin(&moved).unwrap();
        let home = store.get("runbook-home").unwrap();
        assert_eq!(
            home.path,
            super::super::directory(&moved.to_string_lossy()).unwrap()
        );
        assert_eq!(home.commands.len(), 3);
        assert_eq!(store.list().unwrap().len(), 1);
    }

    #[test]
    fn concurrent_first_launches_seed_one_project() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().join("data")).unwrap();
        let handles: Vec<_> = (0..6)
            .map(|_| {
                let store = store.clone();
                let root = temp.path().to_owned();
                std::thread::spawn(move || store.initialize_builtin(&root).unwrap())
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        assert_eq!(store.list().unwrap().len(), 1);
        assert_eq!(store.get("runbook-home").unwrap().commands.len(), 2);
    }
}
