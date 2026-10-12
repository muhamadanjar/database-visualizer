use db_types::{DbError, DbResult};
use desktop_runtime::{DesktopHost, IPC_VERSION, OsSecretStore};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tauri::{Manager, State, ipc::Channel};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;
use tokio::sync::Semaphore;
use uuid::Uuid;

struct Watch {
    sequence: u64,
    credit: Arc<Semaphore>,
    outstanding: bool,
}
struct AppState {
    host: Arc<DesktopHost>,
    watches: Mutex<HashMap<Uuid, Watch>>,
}
fn error() -> DbError {
    DbError::new("INVALID_REQUEST", "desktop", "Desktop request is invalid")
}

#[tauri::command]
async fn desktop_request(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    request: serde_json::Value,
) -> DbResult<serde_json::Value> {
    let operation = request
        .get("operation")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if operation == "copy_text" {
        let text = request
            .get("args")
            .and_then(|v| v.get("text"))
            .and_then(|v| v.as_str())
            .ok_or_else(error)?;
        if text.len() > 32 * 1024 * 1024 {
            return Err(error());
        }
        app.clipboard().write_text(text).map_err(|_| {
            DbError::new(
                "CLIPBOARD_UNAVAILABLE",
                "desktop",
                "Clipboard unavailable; retry copying",
            )
        })?;
        return Ok(serde_json::Value::Null);
    }
    if operation == "recent_files" {
        return Ok(serde_json::json!(
            state
                .host
                .persistence
                .recent_files()?
                .iter()
                .map(|f| serde_json::json!({"id":f.id,"name":f.name}))
                .collect::<Vec<_>>()
        ));
    }
    if matches!(operation, "open_file" | "save_file" | "open_recent") {
        let content = request
            .get("args")
            .and_then(|v| v.get("sql"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        if content.len() > 1024 * 1024 {
            return Err(error());
        }
        let path = if operation == "open_recent" {
            let id = request
                .get("args")
                .and_then(|v| v.get("id"))
                .and_then(|v| v.as_str())
                .and_then(|v| Uuid::parse_str(v).ok())
                .ok_or_else(error)?;
            state
                .host
                .persistence
                .recent_files()?
                .into_iter()
                .find(|f| f.id == id)
                .ok_or_else(error)?
                .path
        } else {
            let save = operation == "save_file";
            let selected = tauri::async_runtime::spawn_blocking(move || {
                let dialog = app.dialog().file().add_filter("SQL", &["sql"]);
                if save {
                    dialog.set_file_name("query.sql").blocking_save_file()
                } else {
                    dialog.blocking_pick_file()
                }
            })
            .await
            .map_err(|_| error())?;
            let Some(selected) = selected else {
                return Ok(serde_json::Value::Null);
            };
            selected.into_path().map_err(|_| error())?
        };
        let file_error = || {
            DbError::new(
                "SQL_FILE_ERROR",
                "desktop",
                "SQL file could not be opened or saved (UTF-8, maximum 1 MiB)",
            )
        };
        let name = path
            .file_name()
            .and_then(|v| v.to_str())
            .ok_or_else(error)?
            .to_owned();
        let save = operation == "save_file";
        let sql = tauri::async_runtime::spawn_blocking({
            let path = path.clone();
            move || -> DbResult<String> {
                if save {
                    std::fs::write(path, &content).map_err(|_| file_error())?;
                    Ok(content)
                } else {
                    use std::io::Read;
                    let mut bytes = Vec::new();
                    std::fs::File::open(path)
                        .map_err(|_| file_error())?
                        .take(1024 * 1024 + 1)
                        .read_to_end(&mut bytes)
                        .map_err(|_| file_error())?;
                    if bytes.len() > 1024 * 1024 {
                        return Err(file_error());
                    }
                    String::from_utf8(bytes).map_err(|_| file_error())
                }
            }
        })
        .await
        .map_err(|_| error())??;
        state.host.persistence.remember_file(path)?;
        return Ok(serde_json::json!({"name":name,"sql":sql}));
    }
    if request.get("operation").and_then(|v| v.as_str()) == Some("watch_ack") {
        let args = request.get("args").ok_or_else(error)?;
        let id = args
            .get("execution_id")
            .and_then(|v| v.as_str())
            .and_then(|v| Uuid::parse_str(v).ok())
            .ok_or_else(error)?;
        let sequence = args
            .get("sequence")
            .and_then(|v| v.as_u64())
            .ok_or_else(error)?;
        let mut watches = state.watches.lock().map_err(|_| error())?;
        if let Some(watch) = watches.get_mut(&id)
            && sequence == watch.sequence
            && watch.outstanding
        {
            watch.outstanding = false;
            watch.credit.add_permits(1);
        }
        return Ok(serde_json::Value::Null);
    }
    if request.get("operation").and_then(|v| v.as_str()) == Some("shutdown") {
        state.host.shutdown().await?;
        return Ok(serde_json::Value::Null);
    }
    state.host.request(request).await
}

#[tauri::command]
async fn watch_execution(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    execution_id: Uuid,
    version: u32,
    on_event: Channel<serde_json::Value>,
) -> DbResult<()> {
    if version != IPC_VERSION {
        return Err(DbError::new(
            "IPC_VERSION_MISMATCH",
            "desktop",
            "Update frontend and host together",
        ));
    }
    state.host.snapshot(execution_id)?;
    let credit = Arc::new(Semaphore::new(1));
    {
        let mut watches = state.watches.lock().map_err(|_| error())?;
        if watches.contains_key(&execution_id) {
            return Err(DbError::new(
                "WATCH_EXISTS",
                "desktop",
                "Execution already has a subscriber",
            ));
        }
        watches.insert(
            execution_id,
            Watch {
                sequence: 0,
                outstanding: false,
                credit: credit.clone(),
            },
        );
    }
    let host = state.host.clone();
    tauri::async_runtime::spawn(async move {
        let mut sequence = 0u64;
        let mut previous = String::new();
        while let Ok(info) = host.snapshot(execution_id) {
            let signature = format!(
                "{:?}/{:?}/{:?}",
                info.status,
                info.results.iter().map(|r| r.rows).collect::<Vec<_>>(),
                info.persistence_error.as_ref().map(|e| &e.code)
            );
            if previous != signature {
                let permit = match tokio::time::timeout(
                    std::time::Duration::from_secs(30),
                    credit.clone().acquire_owned(),
                )
                .await
                {
                    Ok(Ok(p)) => p,
                    _ => {
                        let _ = host.cancel(execution_id).await;
                        break;
                    }
                };
                permit.forget();
                sequence += 1;
                if let Ok(mut watches) = app.state::<AppState>().watches.lock()
                    && let Some(watch) = watches.get_mut(&execution_id)
                {
                    watch.sequence = sequence;
                    watch.outstanding = true;
                }
                // Small notification only. Rows/columns remain behind bounded page/snapshot requests.
                if on_event.send(serde_json::json!({"execution_id":execution_id,"sequence":sequence,"status":info.status,"rows":info.results.iter().map(|r|r.rows).sum::<u64>()})).is_err(){let _=host.cancel(execution_id).await;break;}
                previous = signature;
                if info.status.terminal() {
                    break;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        if let Ok(mut watches) = app.state::<AppState>().watches.lock() {
            watches.remove(&execution_id);
        }
    });
    Ok(())
}

pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init());
    #[cfg(feature = "desktop-e2e")]
    let builder = builder
        .plugin(tauri_plugin_wdio::init())
        .plugin(tauri_plugin_wdio_webdriver::init());
    builder
        .setup(|app| {
            let directory = app.path().app_local_data_dir()?;
            #[cfg(feature = "desktop-e2e")]
            let directory = std::env::var_os("DATABASE_STUDIO_TEST_DATA")
                .map(std::path::PathBuf::from)
                .unwrap_or(directory);
            #[cfg(not(feature = "desktop-e2e"))]
            let secrets = OsSecretStore::default();
            #[cfg(feature = "desktop-e2e")]
            let secrets = OsSecretStore::with_service(format!(
                "dev.database-studio.e2e.{}",
                std::process::id()
            ));
            let host = DesktopHost::open(directory, Arc::new(secrets))?;
            app.manage(AppState {
                host,
                watches: Mutex::new(HashMap::new()),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![desktop_request, watch_execution])
        .build(tauri::generate_context!())
        .expect("Desktop application could not start")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                let host = app.state::<AppState>().host.clone();
                let _ = tauri::async_runtime::block_on(host.shutdown());
            }
        });
}
