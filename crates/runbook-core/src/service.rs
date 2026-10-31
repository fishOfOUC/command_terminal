use crate::{model::*, shell, ApiError, Result, Store};
use schemars::{schema_for, JsonSchema};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

// This protocol is shared by the CLI JSON interface and Tauri IPC.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Info,
    Schema,
    ProjectList {
        #[serde(default)]
        query: Option<String>,
        #[serde(default)]
        archived: Option<bool>,
    },
    ProjectGet {
        id: String,
    },
    ProjectCreate {
        input: ProjectCreate,
    },
    ProjectUpdate {
        id: String,
        patch: ProjectPatch,
        #[serde(rename = "ifRevision")]
        if_revision: Option<u64>,
    },
    ProjectDelete {
        id: String,
        #[serde(rename = "ifRevision")]
        if_revision: Option<u64>,
    },
    ProjectTouch {
        id: String,
    },
    CommandList {
        project: String,
        #[serde(default)]
        query: Option<String>,
    },
    CommandGet {
        project: String,
        id: String,
    },
    CommandCreate {
        project: String,
        input: CommandCreate,
    },
    CommandUpdate {
        project: String,
        id: String,
        patch: CommandPatch,
        #[serde(rename = "ifRevision")]
        if_revision: Option<u64>,
    },
    CommandDelete {
        project: String,
        id: String,
        #[serde(rename = "ifRevision")]
        if_revision: Option<u64>,
    },
    CommandRender {
        project: String,
        id: String,
        #[serde(default)]
        values: BTreeMap<String, Value>,
    },
}
pub fn dispatch(store: &Store, request: Request) -> Result<Value> {
    match request {
        Request::Info => Ok(
            json!({"version":env!("CARGO_PKG_VERSION"),"databasePath":store.path(),"schemaVersion":1,"shell":if cfg!(windows){"powershell"}else{"posix"}}),
        ),
        Request::Schema => Ok(json!({
            "protocolVersion":1,
            "requestSchema":schema_for!(Request),
            "projectSchema":schema_for!(Project),
            "commandSchema":schema_for!(Command),
            "conventions":{
                "output":"One JSON envelope on stdout: {schemaVersion:1,ok:true,data:...} or {schemaVersion:1,ok:false,error:{code,message,details}}",
                "input":"runbook api --input - accepts one request object on stdin. --input FILE reads UTF-8 JSON (UTF-8 BOM allowed).",
                "updates":"Partial field patches; omitted fields stay unchanged. params replaces the complete parameter list. IDs and timestamps are read-only.",
                "concurrency":"Use ifRevision from the last get result. REVISION_CONFLICT means fetch again and reapply your patch.",
                "execution":"CRUD/render never executes a command. command run explicitly executes a stored command in the project directory.",
                "exitCodes":{"0":"Success","1":"I/O, database, terminal or execution error","2":"Arguments, JSON or validation error","3":"Not found","4":"Duplicate ID/path or revision conflict","10":"Executed command returned a non-zero exit code","124":"Execution timed out"},
                "defaultDataDirectory":"RUNBOOK_DATA_DIR, otherwise OS local app data / Runbook. --data-dir overrides this for the CLI."
            }
        })),
        Request::ProjectList { query, archived } => {
            let needle = query.unwrap_or_default().to_lowercase();
            let rows: Vec<_> = store
                .list()?
                .into_iter()
                .filter(|p| {
                    archived.is_none_or(|v| p.archived == v)
                        && format!("{} {} {}", p.name, p.path, p.description)
                            .to_lowercase()
                            .contains(&needle)
                })
                .collect();
            Ok(json!({"items":rows,"total":rows.len()}))
        }
        Request::ProjectGet { id } => Ok(serde_json::to_value(store.get(&id)?)?),
        Request::ProjectCreate { input } => Ok(serde_json::to_value(store.create_project(input)?)?),
        Request::ProjectUpdate {
            id,
            patch,
            if_revision,
        } => Ok(serde_json::to_value(store.patch_project(
            &id,
            patch,
            if_revision,
        )?)?),
        Request::ProjectDelete { id, if_revision } => {
            store.delete_project(&id, if_revision)?;
            Ok(json!({"id":id,"deleted":true}))
        }
        Request::ProjectTouch { id } => {
            store.touch(&id)?;
            Ok(json!({"id":id}))
        }
        Request::CommandList { project, query } => {
            let needle = query.unwrap_or_default().to_lowercase();
            let rows: Vec<_> = store
                .get(&project)?
                .commands
                .into_iter()
                .filter(|c| {
                    format!("{} {} {}", c.name, c.base, c.description)
                        .to_lowercase()
                        .contains(&needle)
                })
                .collect();
            Ok(json!({"items":rows,"total":rows.len()}))
        }
        Request::CommandGet { project, id } => {
            Ok(serde_json::to_value(store.command(&project, &id)?)?)
        }
        Request::CommandCreate { project, input } => Ok(serde_json::to_value(
            store.create_command(&project, input)?,
        )?),
        Request::CommandUpdate {
            project,
            id,
            patch,
            if_revision,
        } => Ok(serde_json::to_value(store.patch_command(
            &project,
            &id,
            patch,
            if_revision,
        )?)?),
        Request::CommandDelete {
            project,
            id,
            if_revision,
        } => {
            store.delete_command(&project, &id, if_revision)?;
            Ok(json!({"project":project,"id":id,"deleted":true}))
        }
        Request::CommandRender {
            project,
            id,
            values,
        } => {
            let p = store.get(&project)?;
            let command = p
                .commands
                .iter()
                .find(|c| c.id == id)
                .ok_or_else(|| ApiError::not_found("command", &id))?;
            Ok(
                json!({"project":project,"id":id,"revision":command.revision,"cwd":p.path,"command":shell::render(command,&values)?,"shell":if cfg!(windows){"powershell"}else{"posix"}}),
            )
        }
    }
}
