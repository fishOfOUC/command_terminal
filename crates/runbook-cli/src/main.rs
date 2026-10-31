use base64::{engine::general_purpose::STANDARD, Engine};
use clap::{Args, Parser, Subcommand};
use runbook_core::{
    dispatch,
    store::default_data_dir,
    terminal::{StartRequest, TerminalManager},
    ApiError, Request, Result, Store,
};
use serde_json::{json, Map, Value};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::PathBuf,
    process::ExitCode,
    time::{Duration, Instant},
};

#[derive(Parser)]
#[command(
    name = "runbook",
    version,
    about = "Manage Runbook projects and commands. JSON by default; no GUI or interactive prompts required.",
    after_help = "AI quick start: runbook schema\nJSON stdin:    runbook api --input -\nExample:       runbook command list --project PROJECT_ID\nUpdate safely: runbook command update --project PROJECT_ID --id COMMAND_ID --if-revision 1 --base 'npm run build'\nAll regular results/errors are one JSON envelope on stdout. See docs/CLI.md."
)]
struct Cli {
    #[arg(
        long,
        global = true,
        help = "Override RUNBOOK_DATA_DIR and the system-local Runbook directory"
    )]
    data_dir: Option<PathBuf>,
    #[arg(long, global = true, help = "Indent JSON output")]
    pretty: bool,
    #[command(subcommand)]
    action: Action,
}
#[derive(Subcommand)]
enum Action {
    /// Describe the active data directory and runtime.
    Info,
    /// JSON schemas, request operations, exit codes and AI usage conventions.
    Schema,
    /// Execute a structured request. Does not accept terminal execution operations.
    Api {
        #[arg(long, default_value = "-")]
        input: String,
    },
    #[command(subcommand)]
    Project(ProjectAction),
    #[command(subcommand)]
    Command(CommandAction),
}
#[derive(Subcommand)]
enum ProjectAction {
    /// Register a directory (current directory by default). Name defaults to its folder name.
    Add {
        /// Absolute, relative or home-relative directory to register.
        path: Option<PathBuf>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        description: Option<String>,
    },
    /// List/search projects. Omit --archived to include both active and archived.
    List {
        #[arg(long)]
        query: Option<String>,
        #[arg(long)]
        archived: Option<bool>,
    },
    Get {
        #[arg(long)]
        id: String,
    },
    Create {
        #[arg(long)]
        id: Option<String>,
        #[command(flatten)]
        fields: ProjectFields,
    },
    Update {
        #[arg(long)]
        id: String,
        #[arg(long)]
        if_revision: Option<u64>,
        #[arg(long)]
        archived: Option<bool>,
        #[command(flatten)]
        fields: ProjectFields,
    },
    /// Delete only Runbook data. The project directory is never removed.
    Delete {
        #[arg(long)]
        id: String,
        #[arg(long)]
        if_revision: Option<u64>,
    },
}
#[derive(Args)]
struct ProjectFields {
    /// UTF-8 JSON object file, or '-' for stdin. Cannot be mixed with field flags.
    #[arg(long)]
    input: Option<String>,
    #[arg(long)]
    name: Option<String>,
    #[arg(long)]
    path: Option<String>,
    #[arg(long)]
    description: Option<String>,
    #[arg(long)]
    starred: Option<bool>,
}
#[derive(Subcommand)]
enum CommandAction {
    List {
        #[arg(long)]
        project: String,
        #[arg(long)]
        query: Option<String>,
    },
    Get {
        #[arg(long)]
        project: String,
        #[arg(long)]
        id: String,
    },
    Create {
        #[arg(long)]
        project: String,
        #[arg(long)]
        id: Option<String>,
        #[command(flatten)]
        fields: CommandFields,
    },
    Update {
        #[arg(long)]
        project: String,
        #[arg(long)]
        id: String,
        #[arg(long)]
        if_revision: Option<u64>,
        #[command(flatten)]
        fields: CommandFields,
    },
    Delete {
        #[arg(long)]
        project: String,
        #[arg(long)]
        id: String,
        #[arg(long)]
        if_revision: Option<u64>,
    },
    /// Validate parameters and return the shell command without executing it.
    Render {
        #[arg(long)]
        project: String,
        #[arg(long)]
        id: String,
        #[arg(
            long,
            help = "JSON object of parameter ID to value; file or '-' for stdin"
        )]
        values: Option<String>,
    },
    /// Explicitly run a saved command using a real PTY and return output/exit status as JSON.
    Run {
        #[arg(long)]
        project: String,
        #[arg(long)]
        id: String,
        #[arg(long)]
        values: Option<String>,
        #[arg(long)]
        if_revision: Option<u64>,
        #[arg(long, default_value_t = 30000)]
        timeout_ms: u64,
    },
}
#[derive(Args)]
struct CommandFields {
    #[arg(
        long,
        help = "Command create object or update patch: UTF-8 JSON file, or '-' for stdin"
    )]
    input: Option<String>,
    #[arg(long)]
    name: Option<String>,
    #[arg(long, allow_hyphen_values = true)]
    base: Option<String>,
    #[arg(long)]
    description: Option<String>,
    #[arg(
        long,
        help = "Complete JSON parameter array: UTF-8 file, or '-' for stdin"
    )]
    params_file: Option<String>,
}
fn read_json(source: &str) -> Result<Value> {
    let mut raw = String::new();
    if source == "-" {
        std::io::stdin()
            .take(4 * 1024 * 1024 + 1)
            .read_to_string(&mut raw)?;
    } else {
        std::fs::File::open(source)?
            .take(4 * 1024 * 1024 + 1)
            .read_to_string(&mut raw)?;
    }
    if raw.len() > 4 * 1024 * 1024 {
        return Err(ApiError::new(
            "INVALID_ARGUMENT",
            "JSON input exceeds 4 MiB",
        ));
    }
    serde_json::from_str(raw.trim_start_matches('\u{feff}')).map_err(Into::into)
}
fn put(map: &mut Map<String, Value>, key: &str, value: Option<impl Into<Value>>) {
    if let Some(v) = value {
        map.insert(key.into(), v.into());
    }
}
fn input_or_flags(input: Option<String>, map: Map<String, Value>) -> Result<Value> {
    if let Some(file) = input {
        if !map.is_empty() {
            return Err(ApiError::new(
                "INVALID_ARGUMENT",
                "--input cannot be mixed with field flags; put all fields in the JSON object",
            ));
        }
        let value = read_json(&file)?;
        if !value.is_object() {
            return Err(ApiError::new("INVALID_JSON", "Expected one JSON object"));
        }
        Ok(value)
    } else {
        Ok(Value::Object(map))
    }
}
fn project_fields(
    fields: ProjectFields,
    id: Option<String>,
    archived: Option<bool>,
) -> Result<Value> {
    let mut map = Map::new();
    put(&mut map, "id", id);
    put(&mut map, "name", fields.name);
    put(&mut map, "path", fields.path);
    put(&mut map, "description", fields.description);
    put(&mut map, "starred", fields.starred);
    put(&mut map, "archived", archived);
    input_or_flags(fields.input, map)
}
fn command_fields(fields: CommandFields, id: Option<String>) -> Result<Value> {
    let mut map = Map::new();
    put(&mut map, "id", id);
    put(&mut map, "name", fields.name);
    put(&mut map, "base", fields.base);
    put(&mut map, "description", fields.description);
    if let Some(file) = fields.params_file {
        map.insert("params".into(), read_json(&file)?);
    }
    input_or_flags(fields.input, map)
}
fn values(file: Option<String>) -> Result<BTreeMap<String, Value>> {
    file.map(|f| read_json(&f).and_then(|v| serde_json::from_value(v).map_err(Into::into)))
        .transpose()
        .map(Option::unwrap_or_default)
}
fn request(action: Action) -> Result<Request> {
    let value = match action {
        Action::Info => json!({"op":"info"}),
        Action::Schema => json!({"op":"schema"}),
        Action::Api { input } => read_json(&input)?,
        Action::Project(action) => match action {
            ProjectAction::Add {
                path,
                name,
                description,
            } => {
                let path = match path {
                    Some(path) => path,
                    None => std::env::current_dir()?,
                };
                let raw = path.to_string_lossy();
                let absolute = if path.is_absolute()
                    || raw == "~"
                    || raw.starts_with("~/")
                    || raw.starts_with("~\\")
                {
                    path
                } else {
                    std::env::current_dir()?.join(path)
                };
                let canonical = runbook_core::store::directory(&absolute.to_string_lossy())?;
                let folder_name = std::path::Path::new(&canonical)
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| {
                        let root = canonical.trim_end_matches(['\\', '/']);
                        if root.is_empty() {
                            canonical.clone()
                        } else {
                            root.to_owned()
                        }
                    });
                json!({"op":"project_create","input":{"name":name.unwrap_or(folder_name),"path":canonical,"description":description.unwrap_or_default()}})
            }
            ProjectAction::List { query, archived } => {
                json!({"op":"project_list","query":query,"archived":archived})
            }
            ProjectAction::Get { id } => json!({"op":"project_get","id":id}),
            ProjectAction::Create { id, fields } => {
                json!({"op":"project_create","input":project_fields(fields,id,None)?})
            }
            ProjectAction::Update {
                id,
                if_revision,
                archived,
                fields,
            } => {
                json!({"op":"project_update","id":id,"ifRevision":if_revision,"patch":project_fields(fields,None,archived)?})
            }
            ProjectAction::Delete { id, if_revision } => {
                json!({"op":"project_delete","id":id,"ifRevision":if_revision})
            }
        },
        Action::Command(action) => match action {
            CommandAction::List { project, query } => {
                json!({"op":"command_list","project":project,"query":query})
            }
            CommandAction::Get { project, id } => {
                json!({"op":"command_get","project":project,"id":id})
            }
            CommandAction::Create {
                project,
                id,
                fields,
            } => {
                json!({"op":"command_create","project":project,"input":command_fields(fields,id)?})
            }
            CommandAction::Update {
                project,
                id,
                if_revision,
                fields,
            } => {
                json!({"op":"command_update","project":project,"id":id,"ifRevision":if_revision,"patch":command_fields(fields,None)?})
            }
            CommandAction::Delete {
                project,
                id,
                if_revision,
            } => json!({"op":"command_delete","project":project,"id":id,"ifRevision":if_revision}),
            CommandAction::Render {
                project,
                id,
                values: file,
            } => json!({"op":"command_render","project":project,"id":id,"values":values(file)?}),
            CommandAction::Run { .. } => {
                return Err(ApiError::new("INVALID_ARGUMENT", "Unexpected run request"))
            }
        },
    };
    serde_json::from_value(value).map_err(Into::into)
}
fn run(
    store: &Store,
    project: String,
    id: String,
    file: Option<String>,
    if_revision: Option<u64>,
    timeout_ms: u64,
) -> Result<(Value, u8)> {
    if !(1..=86400000).contains(&timeout_ms) {
        return Err(ApiError::field(
            "timeoutMs",
            "timeout-ms must be between 1 and 86400000",
        ));
    }
    let manager = TerminalManager::default();
    let info = manager.start(
        store,
        StartRequest {
            project: project.clone(),
            command: Some(id.clone()),
            values: values(file)?,
            if_revision,
            cols: 120,
            rows: 30,
        },
    )?;
    let started = Instant::now();
    let mut sequence = 0;
    let mut output = Vec::new();
    let mut truncated = false;
    let mut timed_out = false;
    let result = loop {
        let snapshot = manager.snapshot(&info.id, sequence)?;
        sequence = snapshot.next_sequence;
        truncated |= snapshot.truncated;
        for data in snapshot.chunks {
            let bytes = STANDARD
                .decode(data)
                .map_err(|e| ApiError::new("TERMINAL_ERROR", e.to_string()))?;
            if bytes.windows(4).any(|w| w == b"\x1b[6n") {
                let _ = manager.write(&info.id, "\x1b[1;1R");
            }
            output.extend(bytes);
        }
        if output.len() > 2 * 1024 * 1024 {
            output.drain(..output.len() - 2 * 1024 * 1024);
            truncated = true;
        }
        if snapshot.session.finished {
            break snapshot.session;
        }
        if started.elapsed() > Duration::from_millis(timeout_ms) && !timed_out {
            timed_out = true;
            manager.stop(&info.id, true)?;
        }
        if timed_out && started.elapsed() > Duration::from_millis(timeout_ms + 5000) {
            let _ = manager.close(&info.id);
            return Err(ApiError::new(
                "TERMINAL_ERROR",
                "Timed-out process did not close its terminal",
            ));
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    let _ = manager.close(&info.id);
    let code = if timed_out {
        124
    } else if result.exit_code == Some(0) {
        0
    } else {
        10
    };
    Ok((
        json!({"project":project,"id":id,"exitCode":result.exit_code,"timedOut":timed_out,"output":String::from_utf8_lossy(&output),"outputTruncated":truncated,"durationMs":started.elapsed().as_millis(),"error":result.error}),
        code,
    ))
}
fn execute(cli: Cli) -> Result<(Value, u8)> {
    let dir = cli.data_dir.map(Ok).unwrap_or_else(default_data_dir)?;
    let store = Store::open(dir)?;
    if let Action::Command(CommandAction::Run {
        project,
        id,
        values,
        if_revision,
        timeout_ms,
    }) = cli.action
    {
        return run(&store, project, id, values, if_revision, timeout_ms);
    }
    Ok((dispatch(&store, request(cli.action)?)?, 0))
}
fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) {
                print!("{error}");
                return ExitCode::SUCCESS;
            }
            let error = ApiError::new("INVALID_ARGUMENT", error.to_string());
            println!("{}", json!({"schemaVersion":1,"ok":false,"error":error}));
            return ExitCode::from(2);
        }
    };
    let pretty = cli.pretty;
    let (envelope, code) = match execute(cli) {
        Ok((data, code)) => (json!({"schemaVersion":1,"ok":code==0,"data":data}), code),
        Err(error) => {
            let code = error.exit_code();
            (json!({"schemaVersion":1,"ok":false,"error":error}), code)
        }
    };
    let serialized = if pretty {
        serde_json::to_string_pretty(&envelope)
    } else {
        serde_json::to_string(&envelope)
    };
    let mut out = std::io::stdout().lock();
    if writeln!(
        out,
        "{}",
        serialized.expect("JSON value serialization cannot fail")
    )
    .is_err()
    {
        return ExitCode::from(1);
    }
    ExitCode::from(code)
}
