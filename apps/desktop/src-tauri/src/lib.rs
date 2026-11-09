use db_types::{DbError, DbResult};
use desktop_runtime::{DesktopHost, IPC_VERSION, OsSecretStore};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
use tauri::{Manager, State, ipc::Channel};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;
use tokio::sync::Semaphore;
use uuid::Uuid;

struct Watch {
    id: u64,
    sequence: u64,
    credit: Arc<Semaphore>,
    outstanding: bool,
}
struct AppState {
    host: Arc<DesktopHost>,
    watches: Mutex<HashMap<Uuid, Watch>>,
}
static NEXT_WATCH_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static NEXT_WATCH_ID: AtomicU64 = AtomicU64::new(1);

impl Watch {
    fn acknowledge(&mut self, sequence: u64) -> bool {
        if sequence == self.sequence && self.outstanding {
            self.outstanding = false;
            self.credit.add_permits(1);
            true
        } else {
            false
        }
    }
}

fn remove_watch_entry(
    watches: &mut HashMap<Uuid, Watch>,
    execution_id: Uuid,
    watch_id: u64,
) -> bool {
    if watches
        .get(&execution_id)
        .is_some_and(|watch| watch.id == watch_id)
    {
        return watches.remove(&execution_id).is_some();
    }
    false
}

impl AppState {
    fn remove_watch(&self, execution_id: Uuid, watch_id: u64) {
        if let Ok(mut watches) = self.watches.lock() {
            remove_watch_entry(&mut watches, execution_id, watch_id);
        }
    }
}

fn next_watch_sequence() -> u64 {
    NEXT_WATCH_SEQUENCE.fetch_add(1, Ordering::Relaxed)
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
        if let Some(watch) = watches.get_mut(&id) {
            watch.acknowledge(sequence);
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
    let watch_id = NEXT_WATCH_ID.fetch_add(1, Ordering::Relaxed);
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
                id: watch_id,
                sequence: 0,
                outstanding: false,
                credit: credit.clone(),
            },
        );
    }
    let host = state.host.clone();
    tauri::async_runtime::spawn(async move {
        let mut previous = String::new();
        let mut available_credit = credit.clone().acquire_owned().await.ok();
        while let Ok(info) = host.snapshot(execution_id) {
            let signature = format!(
                "{:?}/{:?}/{:?}",
                info.status,
                info.results.iter().map(|r| r.rows).collect::<Vec<_>>(),
                info.persistence_error.as_ref().map(|e| &e.code)
            );
            if previous != signature {
                let Some(permit) = available_credit.take() else {
                    app.state::<AppState>().remove_watch(execution_id, watch_id);
                    let _ = host.cancel(execution_id).await;
                    break;
                };
                permit.forget();
                let sequence = next_watch_sequence();
                if let Ok(mut watches) = app.state::<AppState>().watches.lock()
                    && let Some(watch) = watches.get_mut(&execution_id)
                {
                    watch.sequence = sequence;
                    watch.outstanding = true;
                }
                // Small notification only. Rows/columns remain behind bounded page/snapshot requests.
                if on_event.send(serde_json::json!({"execution_id":execution_id,"sequence":sequence,"status":info.status,"rows":info.results.iter().map(|r|r.rows).sum::<u64>()})).is_err(){app.state::<AppState>().remove_watch(execution_id, watch_id);let _=host.cancel(execution_id).await;break;}
                previous = signature;
                if info.status.terminal() {
                    break;
                }
                available_credit = match tokio::time::timeout(
                    std::time::Duration::from_secs(30),
                    credit.clone().acquire_owned(),
                )
                .await
                {
                    Ok(Ok(permit)) => Some(permit),
                    _ => {
                        app.state::<AppState>().remove_watch(execution_id, watch_id);
                        let _ = host.cancel(execution_id).await;
                        break;
                    }
                };
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        app.state::<AppState>().remove_watch(execution_id, watch_id);
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
            let secret_namespace = std::env::var_os("DATABASE_STUDIO_TEST_DATA")
                .and_then(|path| {
                    std::path::PathBuf::from(path)
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                })
                .unwrap_or_else(|| std::process::id().to_string());
            #[cfg(feature = "desktop-e2e")]
            let secrets =
                OsSecretStore::with_service(format!("dev.database-studio.e2e.{secret_namespace}"));
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
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_watch_cannot_replenish_or_remove_new_watch() {
        let execution_id = Uuid::new_v4();
        let old_credit = Arc::new(Semaphore::new(1));
        let old_sequence = next_watch_sequence();
        let mut watches = HashMap::new();
        watches.insert(
            execution_id,
            Watch {
                id: 1,
                sequence: old_sequence,
                credit: old_credit.clone(),
                outstanding: true,
            },
        );
        let old_permit = old_credit.try_acquire_owned().unwrap();
        old_permit.forget();
        assert!(remove_watch_entry(&mut watches, execution_id, 1));

        let new_credit = Arc::new(Semaphore::new(1));
        let new_sequence = next_watch_sequence();
        watches.insert(
            execution_id,
            Watch {
                id: 2,
                sequence: new_sequence,
                credit: new_credit.clone(),
                outstanding: true,
            },
        );
        assert!(!remove_watch_entry(&mut watches, execution_id, 1));
        assert_eq!(watches.get(&execution_id).map(|watch| watch.id), Some(2));
        let new_permit = new_credit.clone().try_acquire_owned().unwrap();
        new_permit.forget();

        assert!(
            !watches
                .get_mut(&execution_id)
                .unwrap()
                .acknowledge(old_sequence)
        );
        assert!(new_credit.try_acquire().is_err());
    }

    #[test]
    fn duplicate_ack_cannot_replenish_credit() {
        let credit = Arc::new(Semaphore::new(0));
        let mut watch = Watch {
            id: 3,
            sequence: 9,
            credit: credit.clone(),
            outstanding: true,
        };

        assert!(watch.acknowledge(9));
        let _permit = credit.try_acquire().unwrap();
        assert!(!watch.acknowledge(9));
        assert!(credit.try_acquire().is_err());
    }
}
