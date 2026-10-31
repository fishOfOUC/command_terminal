use crate::{model::*, ApiError, Result};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone)]
pub struct Store {
    path: PathBuf,
}
pub fn default_data_dir() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os("RUNBOOK_DATA_DIR") {
        return Ok(PathBuf::from(dir));
    }
    dirs::data_local_dir()
        .map(|p| p.join("Runbook"))
        .ok_or_else(|| {
            ApiError::new(
                "IO_ERROR",
                "Could not determine local application data directory",
            )
        })
}
pub fn directory(path: &str) -> Result<String> {
    let expanded = if path == "~" {
        dirs::home_dir().ok_or_else(|| ApiError::field("path", "Home directory unavailable"))?
    } else if let Some(rest) = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        dirs::home_dir()
            .ok_or_else(|| ApiError::field("path", "Home directory unavailable"))?
            .join(rest)
    } else {
        PathBuf::from(path)
    };
    if !expanded.is_absolute() {
        return Err(ApiError::field(
            "path",
            "Project directory must be absolute or start with ~/",
        ));
    }
    let p = dunce::canonicalize(&expanded)
        .map_err(|e| ApiError::field("path", format!("Project directory is unavailable: {e}")))?;
    if !p.is_dir() {
        return Err(ApiError::field(
            "path",
            "Project path must be an existing directory",
        ));
    }
    Ok(p.to_string_lossy().into_owned())
}
fn path_key(path: &str) -> String {
    if cfg!(windows) {
        path.replace('\\', "/").to_lowercase()
    } else {
        path.into()
    }
}
impl Store {
    pub fn open(dir: impl AsRef<Path>) -> Result<Self> {
        std::fs::create_dir_all(dir.as_ref())?;
        let s = Self {
            path: dir.as_ref().join("runbook.db"),
        };
        let c = s.connect()?;
        c.pragma_update(None, "journal_mode", "WAL")?;
        c.execute_batch("CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            INSERT OR IGNORE INTO meta VALUES ('schema_version','1');
            CREATE TABLE IF NOT EXISTS projects (id TEXT PRIMARY KEY, path_key TEXT NOT NULL UNIQUE, document TEXT NOT NULL);")?;
        let version: String = c.query_row(
            "SELECT value FROM meta WHERE key='schema_version'",
            [],
            |r| r.get(0),
        )?;
        if version != "1" {
            return Err(ApiError::new(
                "SCHEMA_VERSION",
                "Unsupported database schema version",
            ));
        }
        Ok(s)
    }
    fn connect(&self) -> Result<Connection> {
        let c = Connection::open(&self.path)?;
        c.busy_timeout(Duration::from_secs(5))?;
        Ok(c)
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn list(&self) -> Result<Vec<Project>> {
        let c = self.connect()?;
        let mut stmt = c.prepare("SELECT document FROM projects ORDER BY id")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
    }
    fn load(c: &Connection, id: &str) -> Result<Project> {
        let raw: Option<String> = c
            .query_row("SELECT document FROM projects WHERE id=?1", [id], |r| {
                r.get(0)
            })
            .optional()?;
        serde_json::from_str(&raw.ok_or_else(|| ApiError::not_found("project", id))?)
            .map_err(Into::into)
    }
    pub fn get(&self, id: &str) -> Result<Project> {
        Self::load(&self.connect()?, id)
    }
    fn save(c: &Connection, p: &Project, insert: bool) -> Result<()> {
        let existing: Option<String> = c
            .query_row(
                "SELECT id FROM projects WHERE path_key=?1 AND id<>?2",
                params![path_key(&p.path), p.id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(id) = existing {
            return Err(ApiError {
                code: "ALREADY_EXISTS".into(),
                message: "This project directory is already registered".into(),
                details: serde_json::json!({"projectId":id}),
            });
        }
        let document = serde_json::to_string(p)?;
        if insert {
            if c.query_row("SELECT 1 FROM projects WHERE id=?1", [&p.id], |r| {
                r.get::<_, u8>(0)
            })
            .optional()?
            .is_some()
            {
                return Err(ApiError::new("ALREADY_EXISTS", "Project ID already exists"));
            }
            c.execute(
                "INSERT INTO projects(id,path_key,document) VALUES(?1,?2,?3)",
                params![p.id, path_key(&p.path), document],
            )?;
        } else {
            c.execute(
                "UPDATE projects SET path_key=?1,document=?2 WHERE id=?3",
                params![path_key(&p.path), document, p.id],
            )?;
        }
        Ok(())
    }
    pub fn create_project(&self, input: ProjectCreate) -> Result<Project> {
        text(&input.name, "name", 60, true)?;
        text(&input.description, "description", 1000, false)?;
        let id = input.id.unwrap_or_else(|| new_id("project"));
        validate_id(&id)?;
        let t = now();
        let p = Project {
            id,
            name: input.name.trim().into(),
            path: directory(&input.path)?,
            description: input.description,
            starred: input.starred,
            archived: false,
            opened_at: None,
            created_at: t,
            updated_at: t,
            revision: 1,
            commands: vec![],
        };
        let mut c = self.connect()?;
        let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
        Self::save(&tx, &p, true)?;
        tx.commit()?;
        Ok(p)
    }
    pub fn mutate<T>(
        &self,
        id: &str,
        expected: Option<u64>,
        f: impl FnOnce(&mut Project) -> Result<T>,
    ) -> Result<T> {
        let mut c = self.connect()?;
        let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut p = Self::load(&tx, id)?;
        check_revision(expected, p.revision)?;
        p.revision += 1;
        p.updated_at = now();
        let result = f(&mut p)?;
        Self::save(&tx, &p, false)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn patch_project(
        &self,
        id: &str,
        patch: ProjectPatch,
        expected: Option<u64>,
    ) -> Result<Project> {
        self.mutate(id, expected, |p| {
            if let Some(name) = patch.name {
                text(&name, "name", 60, true)?;
                p.name = name.trim().into();
            }
            if let Some(path) = patch.path {
                p.path = directory(&path)?;
            }
            if let Some(d) = patch.description {
                text(&d, "description", 1000, false)?;
                p.description = d;
            }
            if let Some(v) = patch.starred {
                p.starred = v;
            }
            if let Some(v) = patch.archived {
                p.archived = v;
            }
            Ok(p.clone())
        })
    }
    pub fn touch(&self, id: &str) -> Result<()> {
        self.mutate(id, None, |p| {
            p.opened_at = Some(now());
            Ok(())
        })
    }
    pub fn delete_project(&self, id: &str, expected: Option<u64>) -> Result<()> {
        let mut c = self.connect()?;
        let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let p = Self::load(&tx, id)?;
        check_revision(expected, p.revision)?;
        tx.execute("DELETE FROM projects WHERE id=?1", [id])?;
        tx.commit()?;
        Ok(())
    }
    pub fn command(&self, project: &str, id: &str) -> Result<Command> {
        self.get(project)?
            .commands
            .into_iter()
            .find(|c| c.id == id)
            .ok_or_else(|| ApiError::not_found("command", id))
    }
    pub fn create_command(&self, project: &str, input: CommandCreate) -> Result<Command> {
        let t = now();
        let cmd = Command {
            id: input.id.unwrap_or_else(|| new_id("command")),
            name: input.name.trim().into(),
            base: input.base.trim().into(),
            description: input.description,
            params: input.params,
            revision: 1,
            created_at: t,
            updated_at: t,
        };
        validate_command(&cmd)?;
        self.mutate(project, None, |p| {
            if p.commands.iter().any(|c| c.id == cmd.id) {
                return Err(ApiError::new(
                    "ALREADY_EXISTS",
                    "Command ID already exists in this project",
                ));
            }
            p.commands.push(cmd.clone());
            Ok(cmd)
        })
    }
    pub fn patch_command(
        &self,
        project: &str,
        id: &str,
        patch: CommandPatch,
        expected: Option<u64>,
    ) -> Result<Command> {
        self.mutate(project, None, |p| {
            let c = p
                .commands
                .iter_mut()
                .find(|c| c.id == id)
                .ok_or_else(|| ApiError::not_found("command", id))?;
            check_revision(expected, c.revision)?;
            if let Some(name) = patch.name {
                c.name = name.trim().into();
            }
            if let Some(base) = patch.base {
                c.base = base.trim().into();
            }
            if let Some(description) = patch.description {
                c.description = description;
            }
            if let Some(params) = patch.params {
                c.params = params;
            }
            validate_command(c)?;
            c.revision += 1;
            c.updated_at = now();
            Ok(c.clone())
        })
    }
    pub fn delete_command(&self, project: &str, id: &str, expected: Option<u64>) -> Result<()> {
        self.mutate(project, None, |p| {
            let index = p
                .commands
                .iter()
                .position(|c| c.id == id)
                .ok_or_else(|| ApiError::not_found("command", id))?;
            check_revision(expected, p.commands[index].revision)?;
            p.commands.remove(index);
            Ok(())
        })
    }
}
pub fn check_revision(expected: Option<u64>, actual: u64) -> Result<()> {
    if let Some(expected) = expected {
        if expected != actual {
            return Err(ApiError::conflict(expected, actual));
        }
    }
    Ok(())
}
