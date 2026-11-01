#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use clap::Parser;
use runbook_core::{
    dispatch,
    store::default_data_dir,
    terminal::{ExecuteRequest, SessionInfo, Snapshot, StartRequest, TerminalManager},
    ApiError, Request, Store,
};
use serde_json::Value;
use tauri::{Manager, State};

#[derive(Parser)]
struct Startup {
    /// Project selected by `runbook gui` in the shared data directory.
    #[arg(long)]
    project: Option<String>,
}

struct Backend {
    store: Store,
    terminals: TerminalManager,
}
#[tauri::command]
async fn api(request: Value, backend: State<'_, Backend>) -> Result<Value, ApiError> {
    let request = serde_json::from_value::<Request>(request)?;
    let store = backend.store.clone();
    tauri::async_runtime::spawn_blocking(move || dispatch(&store, request))
        .await
        .map_err(|e| ApiError::new("INTERNAL_ERROR", e.to_string()))?
}
#[tauri::command]
async fn terminal_start(
    request: StartRequest,
    backend: State<'_, Backend>,
) -> Result<SessionInfo, ApiError> {
    let terminals = backend.terminals.clone();
    let store = backend.store.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if request.command.is_none() {
            terminals.open_interactive(&store, &request.project, request.cols, request.rows)
        } else {
            terminals.start(&store, request)
        }
    })
    .await
    .map_err(|e| ApiError::new("INTERNAL_ERROR", e.to_string()))?
}
#[tauri::command]
async fn terminal_execute(
    request: ExecuteRequest,
    backend: State<'_, Backend>,
) -> Result<SessionInfo, ApiError> {
    let terminals = backend.terminals.clone();
    let store = backend.store.clone();
    tauri::async_runtime::spawn_blocking(move || terminals.execute(&store, request))
        .await
        .map_err(|e| ApiError::new("INTERNAL_ERROR", e.to_string()))?
}
#[tauri::command]
fn terminal_list(
    project: Option<String>,
    backend: State<'_, Backend>,
) -> Result<Vec<SessionInfo>, ApiError> {
    backend.terminals.list(project.as_deref())
}
#[tauri::command]
fn terminal_read(
    id: String,
    after: u64,
    backend: State<'_, Backend>,
) -> Result<Snapshot, ApiError> {
    backend.terminals.snapshot(&id, after)
}
#[tauri::command]
fn terminal_write(id: String, data: String, backend: State<'_, Backend>) -> Result<(), ApiError> {
    backend.terminals.write(&id, &data)
}
#[tauri::command]
fn terminal_resize(
    id: String,
    cols: u16,
    rows: u16,
    backend: State<'_, Backend>,
) -> Result<(), ApiError> {
    backend.terminals.resize(&id, cols, rows)
}
#[tauri::command]
fn terminal_stop(id: String, force: bool, backend: State<'_, Backend>) -> Result<(), ApiError> {
    backend.terminals.stop(&id, force)
}
#[tauri::command]
fn terminal_close(id: String, backend: State<'_, Backend>) -> Result<(), ApiError> {
    backend.terminals.close(&id)
}
#[tauri::command]
fn set_theme(theme: String, window: tauri::WebviewWindow) -> Result<(), String> {
    let theme = match theme.as_str() {
        "light" => tauri::Theme::Light,
        "dark" => tauri::Theme::Dark,
        _ => return Err("Unknown theme".into()),
    };
    window
        .set_theme(Some(theme))
        .map_err(|error| error.to_string())
}
fn main() {
    let startup = Startup::parse();
    let store = Store::open_application(
        default_data_dir().expect("Could not locate Runbook data directory"),
    )
    .expect("Could not open application-local Runbook data directory");
    let mut context = tauri::generate_context!();
    // All Tauri state, including WebView2 localStorage/cache, follows the portable database.
    context.config_mut().app.app_directories_override =
        Some(tauri::utils::config::AppDirectoriesOverride::Root(
            store
                .path()
                .parent()
                .expect("database parent")
                .to_path_buf(),
        ));
    if let Some(id) = startup.project {
        runbook_core::model::validate_id(&id).expect("Invalid startup project ID");
        if let Some(window) = context
            .config_mut()
            .app
            .windows
            .iter_mut()
            .find(|w| w.label == "main")
        {
            window.url = tauri::WebviewUrl::App(format!("index.html?project={id}").into());
        }
    }
    tauri::Builder::default()
        .setup(move |app| {
            app.manage(Backend {
                store,
                terminals: TerminalManager::default(),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            api,
            terminal_start,
            terminal_execute,
            terminal_list,
            terminal_read,
            terminal_write,
            terminal_resize,
            terminal_stop,
            terminal_close,
            set_theme
        ])
        .build(context)
        .expect("Could not initialize Runbook desktop")
        .run(|handle, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                handle.state::<Backend>().terminals.close_all();
            }
        });
}
