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
#[tauri::command]
fn update_status(
    updater: State<'_, std::sync::Arc<runbook_update::Controller>>,
) -> runbook_update::UpdateStatus {
    updater.status()
}
#[tauri::command]
async fn update_check(
    updater: State<'_, std::sync::Arc<runbook_update::Controller>>,
) -> Result<runbook_update::UpdateStatus, String> {
    let updater = updater.inner().clone();
    tauri::async_runtime::spawn_blocking(move || updater.check().map_err(|e| format!("{e:#}")))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn update_download(
    updater: State<'_, std::sync::Arc<runbook_update::Controller>>,
) -> Result<runbook_update::UpdateStatus, String> {
    let updater = updater.inner().clone();
    tauri::async_runtime::spawn_blocking(move || updater.download().map_err(|e| format!("{e:#}")))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn update_install(
    app: tauri::AppHandle,
    project: Option<String>,
    backend: State<'_, Backend>,
    updater: State<'_, std::sync::Arc<runbook_update::Controller>>,
) -> Result<(), String> {
    if let Some(id) = &project {
        runbook_core::model::validate_id(id).map_err(|e| e.to_string())?;
    }
    let data = backend
        .store
        .path()
        .parent()
        .ok_or("数据目录不可用")?
        .to_path_buf();
    let updater = updater.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        updater
            .install(&data, project)
            .map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| e.to_string())??;
    // Request a normal Tauri exit, allowing the existing terminal cleanup hook to run.
    app.exit(0);
    Ok(())
}
#[tauri::command]
fn update_ready() -> Result<(), String> {
    runbook_update::application_dir()
        .and_then(|dir| runbook_update::acknowledge(&dir))
        .map_err(|e| format!("{e:#}"))
}

fn main() {
    let app_dir = runbook_update::application_dir().expect("Could not locate application");
    let _runtime_guard = match runbook_update::RuntimeGuard::acquire(&app_dir) {
        Ok(guard) => guard,
        Err(error) => {
            if runbook_update::is_recovery_scheduled(&error) {
                return;
            }
            eprintln!("{error:#}");
            #[cfg(windows)]
            {
                let message: Vec<u16> =
                    format!("{error:#}").encode_utf16().chain(Some(0)).collect();
                let title: Vec<u16> = "Runbook".encode_utf16().chain(Some(0)).collect();
                // SAFETY: both UTF-16 strings are NUL terminated and remain alive for this synchronous call.
                unsafe {
                    windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
                        std::ptr::null_mut(),
                        message.as_ptr(),
                        title.as_ptr(),
                        0x30,
                    );
                }
            }
            return;
        }
    };
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
            app.manage(std::sync::Arc::new(runbook_update::Controller::new(
                app_dir,
            )));
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
            set_theme,
            update_status,
            update_check,
            update_download,
            update_install,
            update_ready
        ])
        .build(context)
        .expect("Could not initialize Runbook desktop")
        .run(|handle, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                handle.state::<Backend>().terminals.close_all();
            }
        });
}
