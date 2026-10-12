use crate::{error, persistence::now_ms, *};
use db_core::{DatabaseCore, ProtectedTarget, RuntimeLimits, prepare_script};
use db_types::*;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex, atomic::AtomicU64},
    time::Duration,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use zeroize::Zeroizing;

pub const IPC_VERSION: u32 = 1;
pub use query_runtime::{ExecutionInfo, ResultInfo, SessionInfo};
struct EditorSession {
    profile: Profile,
    auto_commit: bool,
    running: Option<Uuid>,
}
struct Execution {
    info: ExecutionInfo,
    spools: HashMap<Uuid, Arc<Mutex<Spool>>>,
    views: HashMap<Uuid, Arc<Mutex<Spool>>>,
    cancel: CancellationToken,
    active_query: Option<Uuid>,
}
pub struct DesktopHost {
    edit_sessions: Mutex<HashMap<Uuid, (Uuid, CancellationToken)>>,
    edits: query_runtime::edit::EditRuntime,
    pub persistence: Persistence,
    core: DatabaseCore,
    sessions: Mutex<HashMap<Uuid, EditorSession>>,
    executions: Mutex<HashMap<Uuid, Execution>>,
    memory_passwords: Mutex<HashMap<Uuid, Zeroizing<String>>>,
    spool_directory: PathBuf,
    spool_total: Arc<AtomicU64>,
    _lock: std::fs::File,
}
fn lock<T>(mutex: &Mutex<T>) -> DbResult<std::sync::MutexGuard<'_, T>> {
    mutex
        .lock()
        .map_err(|_| error("HOST_UNAVAILABLE", "Desktop service unavailable"))
}
fn json_error(_: serde_json::Error) -> DbError {
    error("INVALID_REQUEST", "Request is invalid")
}
pub use query_runtime::safe_terminal;
impl DesktopHost {
    pub fn open(directory: PathBuf, secrets: Arc<dyn SecretStore>) -> DbResult<Arc<Self>> {
        crate::persistence::private_directory(&directory)?;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(directory.join("workspace.lock"))
            .map_err(|_| error("LOCAL_STORAGE_ERROR", "Application directory unavailable"))?;
        file.try_lock().map_err(|_| {
            error(
                "WORKSPACE_IN_USE",
                "Another Database Studio instance owns this workspace",
            )
        })?;
        let spool_directory = directory.join("results");
        cleanup_spools(&spool_directory)?;
        let persistence = Persistence::open(&directory, secrets)?;
        let core = DatabaseCore::new(
            vec![
                Arc::new(db_postgres::PostgresAdapter),
                Arc::new(db_mysql::MySqlAdapter),
            ],
            RuntimeLimits::default(),
            vec![
                ProtectedTarget {
                    host: "127.0.0.1".into(),
                    port: 55433,
                    database: "studio_app".into(),
                },
                ProtectedTarget {
                    host: "localhost".into(),
                    port: 55433,
                    database: "studio_app".into(),
                },
            ],
        )?;
        let host = Arc::new(Self {
            edit_sessions: Mutex::new(HashMap::new()),
            edits: query_runtime::edit::EditRuntime::default(),
            persistence,
            core,
            sessions: Mutex::new(HashMap::new()),
            executions: Mutex::new(HashMap::new()),
            memory_passwords: Mutex::new(HashMap::new()),
            spool_directory,
            spool_total: Arc::new(AtomicU64::new(0)),
            _lock: file,
        });
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let weak = Arc::downgrade(&host);
            runtime.spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(60));
                loop {
                    interval.tick().await;
                    let Some(host) = weak.upgrade() else {
                        break;
                    };
                    host.edits.prune();
                }
            });
        }
        Ok(host)
    }
    pub fn save_profile(
        &self,
        profile: Profile,
        password: Option<String>,
        remember: bool,
    ) -> DbResult<Profile> {
        let id = profile.id;
        let password = password.map(Zeroizing::new);
        let profile = self.persistence.save_profile(
            profile,
            password.as_ref().map(|p| p.as_str()),
            remember,
        )?;
        if let Some(password) = password {
            if !remember {
                lock(&self.memory_passwords)?.insert(id, password);
            } else {
                lock(&self.memory_passwords)?.remove(&id);
            }
        }
        Ok(profile)
    }
    fn config(&self, profile: &Profile, password: Option<String>) -> DbResult<ConnectionConfig> {
        let password = match password {
            Some(password) => password,
            None => match lock(&self.memory_passwords)?.get(&profile.id) {
                Some(password) => password.to_string(),
                None => self.persistence.password(profile.id)?,
            },
        };
        profile.config(password)
    }
    pub async fn test(&self, profile: Profile, password: Option<String>) -> DbResult<()> {
        self.core
            .test_connection(self.config(&profile, password)?)
            .await
    }
    pub async fn connect(&self, id: Uuid, password: Option<String>) -> DbResult<SessionInfo> {
        let profile = self.persistence.profile(id)?;
        let config = self.config(&profile, password)?;
        let session_id = self.core.connect(config).await?;
        lock(&self.sessions)?.insert(
            session_id,
            EditorSession {
                profile: profile.clone(),
                auto_commit: true,
                running: None,
            },
        );
        self.session(session_id).await
    }
    pub async fn session(&self, id: Uuid) -> DbResult<SessionInfo> {
        let (connection_id, revision, auto_commit, busy) = {
            let sessions = lock(&self.sessions)?;
            let s = sessions
                .get(&id)
                .ok_or_else(|| error("SESSION_NOT_FOUND", "Session is closed"))?;
            (
                s.profile.id,
                s.profile.revision,
                s.auto_commit,
                s.running.is_some(),
            )
        };
        Ok(SessionInfo {
            id,
            connection_id,
            revision,
            auto_commit,
            transaction: self.core.transaction_state(id).await?,
            valid: self.core.session_valid(id).await,
            busy,
        })
    }
    pub async fn disconnect(&self, id: Uuid, force: bool) -> DbResult<()> {
        let info = self.session(id).await?;
        if !force && (info.busy || info.transaction != TransactionState::Idle) {
            return Err(error(
                "CLOSE_CONFIRMATION_REQUIRED",
                "Cancel/resolve the transaction or explicitly disconnect",
            ));
        }
        let execution = { lock(&self.sessions)?.get(&id).and_then(|s| s.running) };
        if let Some(execution) = execution {
            self.cancel(execution).await?;
        }
        self.core.disconnect(id).await?;
        if let Some(execution) = execution {
            self.wait_execution(execution).await?;
        }
        lock(&self.sessions)?.remove(&id);
        Ok(())
    }
    pub async fn delete_profile(&self, id: Uuid, force: bool) -> DbResult<()> {
        let sessions = lock(&self.sessions)?
            .iter()
            .filter(|(_, s)| s.profile.id == id)
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        for id in sessions {
            self.disconnect(id, force).await?;
        }
        lock(&self.executions)?.retain(|_, e| e.info.connection_id != id);
        self.persistence.delete_profile(id)?;
        lock(&self.memory_passwords)?.remove(&id);
        Ok(())
    }
    pub async fn auto_commit(&self, id: Uuid, enabled: bool) -> DbResult<SessionInfo> {
        let info = self.session(id).await?;
        if info.busy || info.transaction != TransactionState::Idle {
            return Err(error(
                "TRANSACTION_RESOLUTION_REQUIRED",
                "Wait for execution and Commit/Rollback before changing mode",
            ));
        }
        lock(&self.sessions)?
            .get_mut(&id)
            .ok_or_else(|| error("SESSION_NOT_FOUND", "Session is closed"))?
            .auto_commit = enabled;
        self.session(id).await
    }
    pub fn snapshot(&self, id: Uuid) -> DbResult<ExecutionInfo> {
        lock(&self.executions)?
            .get(&id)
            .map(|e| e.info.clone())
            .ok_or_else(|| error("EXECUTION_NOT_FOUND", "Execution is closed"))
    }
    pub fn page(
        &self,
        id: Uuid,
        result: Uuid,
        offset: u64,
        limit: usize,
    ) -> DbResult<Vec<WireRow>> {
        let spool = {
            let executions = lock(&self.executions)?;
            let e = executions
                .get(&id)
                .ok_or_else(|| error("EXECUTION_NOT_FOUND", "Execution is closed"))?;
            e.views
                .get(&result)
                .or_else(|| e.spools.get(&result))
                .cloned()
                .ok_or_else(|| error("RESULT_NOT_FOUND", "Result is closed"))?
        };
        lock(&spool)?.page(offset, limit)
    }
    pub async fn cancel(&self, id: Uuid) -> DbResult<()> {
        let query = {
            let mut executions = lock(&self.executions)?;
            let execution = executions
                .get_mut(&id)
                .ok_or_else(|| error("EXECUTION_NOT_FOUND", "Execution is closed"))?;
            execution.cancel.cancel();
            execution.active_query
        };
        if let Some(query) = query {
            self.core.cancel(query).await;
        }
        Ok(())
    }
    pub fn close_execution(&self, id: Uuid) -> DbResult<()> {
        let mut executions = lock(&self.executions)?;
        if executions
            .get(&id)
            .is_some_and(|e| !e.info.status.terminal())
        {
            return Err(error(
                "EXECUTION_BUSY",
                "Cancel and wait for execution cleanup before closing results",
            ));
        }
        executions.remove(&id);
        Ok(())
    }
    pub fn transform_result(
        &self,
        id: Uuid,
        result: Uuid,
        column: Option<usize>,
        descending: bool,
        filter: &str,
    ) -> DbResult<ExecutionInfo> {
        let source = {
            let executions = lock(&self.executions)?;
            let e = executions
                .get(&id)
                .ok_or_else(|| error("EXECUTION_NOT_FOUND", "Execution is closed"))?;
            if !e.info.status.terminal() {
                return Err(error(
                    "EXECUTION_BUSY",
                    "Wait for execution cleanup before sorting/filtering results",
                ));
            }
            let info = e
                .info
                .results
                .iter()
                .find(|r| r.id == result)
                .ok_or_else(|| error("RESULT_NOT_FOUND", "Result is closed"))?;
            if column.is_some_and(|c| c >= info.columns.len()) || filter.len() > 4096 {
                return Err(error(
                    "INVALID_FILTER",
                    "Choose a valid result column and bounded filter",
                ));
            }
            e.spools
                .get(&result)
                .cloned()
                .ok_or_else(|| error("RESULT_NOT_FOUND", "Result is closed"))?
        };
        let view = if column.is_none() && filter.is_empty() {
            None
        } else {
            Some(lock(&source)?.transform(column, descending, filter)?)
        };
        let rows = match &view {
            Some(v) => v.rows(),
            None => lock(&source)?.rows(),
        };
        let mut executions = lock(&self.executions)?;
        let e = executions
            .get_mut(&id)
            .ok_or_else(|| error("EXECUTION_NOT_FOUND", "Execution is closed"))?;
        if let Some(view) = view {
            e.views.insert(result, Arc::new(Mutex::new(view)));
        } else {
            e.views.remove(&result);
        }
        if let Some(info) = e.info.results.iter_mut().find(|r| r.id == result) {
            info.rows = rows;
        }
        Ok(e.info.clone())
    }
    pub async fn start(self: &Arc<Self>, session: Uuid, sql: String) -> DbResult<Uuid> {
        self.start_bound(session, sql, Vec::new()).await
    }
    async fn start_bound(
        self: &Arc<Self>,
        session: Uuid,
        sql: String,
        parameters: Vec<String>,
    ) -> DbResult<Uuid> {
        let (profile, auto_commit) = {
            let sessions = lock(&self.sessions)?;
            let s = sessions
                .get(&session)
                .ok_or_else(|| error("SESSION_NOT_FOUND", "Session is closed"))?;
            (s.profile.clone(), s.auto_commit)
        };
        let statements = prepare_script(profile.engine, &sql)?;
        let id = Uuid::new_v4();
        let cancel = CancellationToken::new();
        {
            let mut executions = lock(&self.executions)?;
            if executions.len() >= 64 {
                return Err(error(
                    "RESULT_CAPACITY_EXCEEDED",
                    "Close old result tabs before running another script",
                ));
            }
            let mut sessions = lock(&self.sessions)?;
            let s = sessions
                .get_mut(&session)
                .ok_or_else(|| error("SESSION_NOT_FOUND", "Session is closed"))?;
            if s.running.is_some() {
                return Err(error(
                    "SESSION_BUSY",
                    "This SQL tab already has an active execution",
                ));
            }
            s.running = Some(id);
            executions.insert(
                id,
                Execution {
                    info: ExecutionInfo {
                        id,
                        session_id: session,
                        connection_id: profile.id,
                        status: QueryStatus::Created,
                        results: Vec::new(),
                        error: None,
                        started_at_ms: now_ms(),
                        finished_at_ms: None,
                        persistence_error: None,
                    },
                    spools: HashMap::new(),
                    views: HashMap::new(),
                    cancel: cancel.clone(),
                    active_query: None,
                },
            );
        }
        let host = self.clone();
        tokio::spawn(async move {
            let outcome = host
                .run_script(
                    id,
                    session,
                    &profile,
                    auto_commit,
                    statements,
                    parameters,
                    cancel.clone(),
                )
                .await;
            let mut info = match host.snapshot(id) {
                Ok(info) => info,
                Err(_) => return,
            };
            info.finished_at_ms = Some(now_ms());
            match outcome {
                Ok(status) => info.status = status,
                Err(e) => {
                    info.status = QueryStatus::Failed;
                    info.error = Some(e);
                }
            }
            if profile.history_enabled {
                let document = Document {
                    id,
                    connection_id: profile.id,
                    kind: "history".into(),
                    name: format!("Execution {}", info.started_at_ms),
                    content: serde_json::json!({"sql":sql,"execution":info}),
                    created_at_ms: info.started_at_ms,
                };
                if let Err(e) = host.persistence.save_document(&document) {
                    info.persistence_error = Some(e);
                }
            }
            if let Ok(mut sessions) = lock(&host.sessions)
                && let Some(s) = sessions.get_mut(&session)
            {
                s.running = None;
            }
            if let Ok(mut executions) = lock(&host.executions)
                && let Some(execution) = executions.get_mut(&id)
            {
                execution.info = info;
                execution.active_query = None;
            }
        });
        Ok(id)
    }
    #[allow(clippy::too_many_arguments)] // Explicit protocol execution context.
    async fn run_script(
        &self,
        id: Uuid,
        session: Uuid,
        profile: &Profile,
        auto_commit: bool,
        statements: Vec<db_core::ScriptStatement>,
        parameters: Vec<String>,
        cancel: CancellationToken,
    ) -> DbResult<QueryStatus> {
        query_runtime::run_script(
            query_runtime::ScriptContext {
                core: &self.core,
                session,
                engine: profile.engine,
                auto_commit,
                timeout: profile.query_timeout_ms.map(Duration::from_millis),
                parameters,
                cancel,
            },
            statements,
            &DesktopResults {
                host: self,
                execution: id,
            },
        )
        .await
    }
    pub async fn shutdown(&self) -> DbResult<()> {
        self.edits.clear();
        let executions = lock(&self.executions)?.keys().copied().collect::<Vec<_>>();
        for id in &executions {
            self.cancel(*id).await?;
        }
        self.core.shutdown().await?;
        for id in executions {
            self.wait_execution(id).await?;
        }
        lock(&self.executions)?.clear();
        lock(&self.sessions)?.clear();
        lock(&self.memory_passwords)?.clear();
        Ok(())
    }
    async fn wait_execution(&self, id: Uuid) -> DbResult<()> {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if lock(&self.executions)?
                    .get(&id)
                    .is_none_or(|e| e.info.status.terminal())
                {
                    return Ok(());
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .map_err(|_| {
            error(
                "CLEANUP_PENDING",
                "Execution persistence/cleanup is still pending; retry close",
            )
        })?
    }
    pub async fn request(
        self: &Arc<Self>,
        request: serde_json::Value,
    ) -> DbResult<serde_json::Value> {
        let operation = request
            .get("operation")
            .and_then(|v| v.as_str())
            .ok_or_else(|| error("INVALID_REQUEST", "Operation required"))?;
        let args = request
            .get("args")
            .cloned()
            .unwrap_or(serde_json::json!({}));
        let id = |key: &str| -> DbResult<Uuid> {
            args.get(key)
                .and_then(|v| v.as_str())
                .and_then(|v| Uuid::parse_str(v).ok())
                .ok_or_else(|| error("INVALID_REQUEST", "Valid resource identifier required"))
        };
        let password = || {
            args.get("password")
                .and_then(|v| v.as_str())
                .map(String::from)
        };
        let force = args.get("force").and_then(|v| v.as_bool()).unwrap_or(false);
        match operation {
            "handshake" => {
                if args.get("version").and_then(|v| v.as_u64()) != Some(IPC_VERSION as u64) {
                    return Err(error(
                        "IPC_VERSION_MISMATCH",
                        "Update the application and frontend together",
                    ));
                }
                Ok(serde_json::json!({"version":IPC_VERSION}))
            }
            "profiles" => serde_json::to_value(self.persistence.profiles()?).map_err(json_error),
            "save_profile" => {
                let profile = serde_json::from_value(
                    args.get("profile")
                        .cloned()
                        .ok_or_else(|| error("INVALID_REQUEST", "Connection required"))?,
                )
                .map_err(json_error)?;
                serde_json::to_value(
                    self.save_profile(
                        profile,
                        password(),
                        args.get("remember")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false),
                    )?,
                )
                .map_err(json_error)
            }
            "test" => {
                let profile = serde_json::from_value(
                    args.get("profile")
                        .cloned()
                        .ok_or_else(|| error("INVALID_REQUEST", "Connection required"))?,
                )
                .map_err(json_error)?;
                self.test(profile, password()).await?;
                Ok(serde_json::Value::Null)
            }
            "connect" => {
                serde_json::to_value(self.connect(id("connection_id")?, password()).await?)
                    .map_err(json_error)
            }
            "session" => {
                serde_json::to_value(self.session(id("session_id")?).await?).map_err(json_error)
            }
            "disconnect" => {
                self.disconnect(id("session_id")?, force).await?;
                Ok(serde_json::Value::Null)
            }
            "delete_profile" => {
                self.delete_profile(id("connection_id")?, force).await?;
                Ok(serde_json::Value::Null)
            }
            "auto_commit" => serde_json::to_value(
                self.auto_commit(
                    id("session_id")?,
                    args.get("enabled")
                        .and_then(|v| v.as_bool())
                        .ok_or_else(|| error("INVALID_REQUEST", "Auto commit mode required"))?,
                )
                .await?,
            )
            .map_err(json_error),
            "execute" => {
                let sql = args
                    .get("sql")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| error("INVALID_REQUEST", "SQL required"))?;
                Ok(
                    serde_json::json!({"execution_id":self.start(id("session_id")?,sql.into()).await?}),
                )
            }
            "table" => {
                let session = id("session_id")?;
                let profile = lock(&self.sessions)?
                    .get(&session)
                    .map(|s| s.profile.clone())
                    .ok_or_else(|| error("SESSION_NOT_FOUND", "Session is closed"))?;
                let request: query_runtime::TableRequest =
                    serde_json::from_value(args.clone()).map_err(json_error)?;
                let (sql, parameters) = query_runtime::table_query(&profile, &request)?;
                Ok(
                    serde_json::json!({"execution_id":self.start_bound(session,sql,parameters).await?}),
                )
            }
            "execution" => {
                serde_json::to_value(self.snapshot(id("execution_id")?)?).map_err(json_error)
            }
            "cancel" => {
                self.cancel(id("execution_id")?).await?;
                Ok(serde_json::Value::Null)
            }
            "close_execution" => {
                self.close_execution(id("execution_id")?)?;
                Ok(serde_json::Value::Null)
            }
            "transform_result" => serde_json::to_value(
                self.transform_result(
                    id("execution_id")?,
                    id("result_id")?,
                    args.get("column")
                        .and_then(|v| v.as_u64())
                        .map(|v| v as usize),
                    args.get("descending")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                    args.get("filter").and_then(|v| v.as_str()).unwrap_or(""),
                )?,
            )
            .map_err(json_error),
            "page" => serde_json::to_value(
                self.page(
                    id("execution_id")?,
                    id("result_id")?,
                    args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0),
                    args.get("limit")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(100)
                        .try_into()
                        .map_err(|_| error("INVALID_PAGE", "Page limit invalid"))?,
                )?,
            )
            .map_err(json_error),
            "metadata" => {
                let profile = self.persistence.profile(id("connection_id")?)?;
                let metadata = serde_json::from_value(
                    args.get("request")
                        .cloned()
                        .ok_or_else(|| error("INVALID_REQUEST", "Metadata request required"))?,
                )
                .map_err(json_error)?;
                let response = serde_json::to_value(
                    self.core
                        .metadata(&self.config(&profile, password())?, &metadata)
                        .await?,
                )
                .map_err(json_error)?;
                if serde_json::to_vec(&response).map_err(json_error)?.len() > 8 * 1024 * 1024 {
                    return Err(error(
                        "METADATA_CAPACITY_EXCEEDED",
                        "Metadata response exceeds 8 MiB; choose a narrower schema",
                    ));
                }
                Ok(response)
            }
            "edit_snapshot" => {
                let connection = id("connection_id")?;
                let tab = id("tab_id")?;
                let object: ObjectId = serde_json::from_value(
                    args.get("object")
                        .cloned()
                        .ok_or_else(|| error("INVALID_EDIT", "Table required"))?,
                )
                .map_err(json_error)?;
                let profile = self.persistence.profile(connection)?;
                let session = self.connect(connection, None).await?;
                let result = async {
                    let metadata = self
                        .core
                        .metadata(
                            &self.config(&profile, None)?,
                            &MetadataRequest::Inspect {
                                object: object.clone(),
                            },
                        )
                        .await?;
                    self.edits
                        .snapshot_page(
                            &self.core,
                            session.id,
                            &profile,
                            query_runtime::edit::EditOwner {
                                user: "desktop".into(),
                                tab: tab.to_string(),
                                scope: "local".into(),
                            },
                            object,
                            metadata,
                            args.get("offset")
                                .and_then(|v| v.as_u64())
                                .and_then(|v| u32::try_from(v).ok())
                                .unwrap_or(0),
                        )
                        .await
                }
                .await;
                let _ = self.disconnect(session.id, true).await;
                serde_json::to_value(result?).map_err(json_error)
            }
            "edit_readback" => {
                let owner = query_runtime::edit::EditOwner {
                    user: "desktop".into(),
                    tab: id("tab_id")?.to_string(),
                    scope: "local".into(),
                };
                serde_json::to_value(self.edits.readback(
                    &owner,
                    id("connection_id")?,
                    id("token")?,
                )?)
                .map_err(json_error)
            }
            "edit_preview" => {
                let owner = query_runtime::edit::EditOwner {
                    user: "desktop".into(),
                    tab: id("tab_id")?.to_string(),
                    scope: "local".into(),
                };
                let batch = serde_json::from_value(
                    args.get("batch")
                        .cloned()
                        .ok_or_else(|| error("INVALID_EDIT", "Batch required"))?,
                )
                .map_err(json_error)?;
                self.edits.preview(&owner, id("connection_id")?, &batch)
            }
            "edit_discard" => {
                self.edits.discard_owner(&query_runtime::edit::EditOwner {
                    user: "desktop".into(),
                    tab: id("tab_id")?.to_string(),
                    scope: "local".into(),
                });
                Ok(serde_json::Value::Null)
            }
            "edit_apply" => {
                let connection = id("connection_id")?;
                let tab = id("tab_id")?;
                let operation_id = id("idempotency_key")?;
                let batch: query_runtime::edit::EditBatch = serde_json::from_value(
                    args.get("batch")
                        .cloned()
                        .ok_or_else(|| error("INVALID_EDIT", "Batch required"))?,
                )
                .map_err(json_error)?;
                if batch.tab_id != tab {
                    return Err(error(
                        "INVALID_EDIT",
                        "Edit tab does not match snapshot owner",
                    ));
                }
                let owner = query_runtime::edit::EditOwner {
                    user: "desktop".into(),
                    tab: tab.to_string(),
                    scope: "local".into(),
                };
                let fingerprint = serde_json::to_vec(&batch).map_err(json_error)?;
                if let Some(outcome) =
                    self.persistence
                        .admit_mutation(operation_id, connection, tab, &fingerprint)?
                {
                    return Ok(serde_json::json!({"id":operation_id,"state":outcome}));
                }
                let prepared = (|| {
                    self.edits.preview(&owner, connection, &batch)?;
                    Ok::<_, DbError>((
                        self.persistence.profile(connection)?,
                        self.edits.object(&owner, connection, batch.token)?,
                    ))
                })();
                let (profile, object) = match prepared {
                    Ok(v) => v,
                    Err(e) => {
                        self.persistence.finish_mutation(operation_id, "Failed")?;
                        return Err(e);
                    }
                };
                let cancellation = CancellationToken::new();
                lock(&self.edit_sessions)?
                    .insert(operation_id, (Uuid::nil(), cancellation.clone()));
                let host = self.clone();
                tokio::spawn(async move {
                    let started = std::time::Instant::now();
                    let remaining = || Duration::from_secs(30).saturating_sub(started.elapsed());
                    let result = async {
                        if cancellation.is_cancelled() {
                            return Ok(query_runtime::edit::EditOutcome::Failed);
                        }
                        let session =
                            tokio::time::timeout(remaining(), host.connect(connection, None))
                                .await
                                .map_err(|_| {
                                    error("EDIT_DEADLINE", "Edit admission deadline exceeded")
                                })??;
                        if let Some(entry) = lock(&host.edit_sessions)?.get_mut(&operation_id) {
                            entry.0 = session.id;
                        }
                        let result = async {
                            if cancellation.is_cancelled() {
                                return Ok(query_runtime::edit::EditOutcome::Failed);
                            }
                            let metadata = tokio::time::timeout(
                                remaining(),
                                host.core.metadata(
                                    &host.config(&profile, None)?,
                                    &MetadataRequest::Inspect { object },
                                ),
                            )
                            .await
                            .map_err(|_| {
                                error("EDIT_DEADLINE", "Edit metadata deadline exceeded")
                            })??;
                            host.edits
                                .apply_with_deadline(
                                    &host.core,
                                    session.id,
                                    &profile,
                                    &owner,
                                    &batch,
                                    &metadata,
                                    remaining(),
                                )
                                .await
                        }
                        .await;
                        let _ = host.disconnect(session.id, true).await;
                        result
                    }
                    .await;
                    let state = format!(
                        "{:?}",
                        result.unwrap_or(query_runtime::edit::EditOutcome::Unknown)
                    );
                    let _ = host.persistence.finish_mutation(operation_id, &state);
                    if let Ok(mut sessions) = host.edit_sessions.lock() {
                        sessions.remove(&operation_id);
                    }
                });
                Ok(serde_json::json!({"id":operation_id,"state":"Applying"}))
            }
            "edit_cancel" => {
                let operation_id = id("mutation_id")?;
                self.persistence.mutation_status(
                    operation_id,
                    id("connection_id")?,
                    id("tab_id")?,
                )?;
                let session = {
                    let sessions = lock(&self.edit_sessions)?;
                    sessions.get(&operation_id).map(|(session, token)| {
                        token.cancel();
                        *session
                    })
                };
                if let Some(session) = session.filter(|s| !s.is_nil()) {
                    let _ = self.disconnect(session, true).await;
                }
                Ok(serde_json::json!({"id":operation_id,"cancel_requested":true}))
            }
            "edit_status" => self.persistence.mutation_status(
                id("idempotency_key")?,
                id("connection_id")?,
                id("tab_id")?,
            ),
            "search_documents" => self.persistence.search_documents(
                id("connection_id")?,
                serde_json::from_value(
                    args.get("search")
                        .cloned()
                        .ok_or_else(|| error("INVALID_SEARCH", "Search required"))?,
                )
                .map_err(json_error)?,
            ),
            "favorite" => {
                self.persistence.favorite(
                    id("connection_id")?,
                    id("id")?,
                    args.get("favorite")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                )?;
                Ok(serde_json::Value::Null)
            }
            "documents" => serde_json::to_value(self.persistence.document_headers(
                id("connection_id")?,
                args.get("kind").and_then(|v| v.as_str()).unwrap_or("saved"),
                args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0),
                args.get("limit").and_then(|v| v.as_u64()).unwrap_or(100),
            )?)
            .map_err(json_error),
            "document" => serde_json::to_value(self.persistence.document(
                id("connection_id")?,
                args.get("kind").and_then(|v| v.as_str()).unwrap_or("saved"),
                id("id")?,
            )?)
            .map_err(json_error),
            "rename_document" => {
                self.persistence.rename_document(
                    id("connection_id")?,
                    args.get("kind").and_then(|v| v.as_str()).unwrap_or("saved"),
                    id("id")?,
                    args.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                )?;
                Ok(serde_json::Value::Null)
            }
            "save_document" => {
                let document = serde_json::from_value(
                    args.get("document")
                        .cloned()
                        .ok_or_else(|| error("INVALID_REQUEST", "Document required"))?,
                )
                .map_err(json_error)?;
                self.persistence.save_document(&document)?;
                Ok(serde_json::Value::Null)
            }
            "delete_documents" => {
                let kind = args.get("kind").and_then(|v| v.as_str()).unwrap_or("saved");
                self.persistence.delete_documents(
                    id("connection_id")?,
                    kind,
                    args.get("id")
                        .and_then(|v| v.as_str())
                        .and_then(|v| Uuid::parse_str(v).ok()),
                )?;
                Ok(serde_json::Value::Null)
            }
            "layout" => self.persistence.layout(),
            "save_layout" => {
                self.persistence.save_layout(&args)?;
                Ok(serde_json::Value::Null)
            }
            "prepare_script" => {
                let profile = self.persistence.profile(id("connection_id")?)?;
                let sql = args.get("sql").and_then(|v| v.as_str()).unwrap_or("");
                Ok(serde_json::Value::Array(
                    prepare_script(profile.engine, sql)?
                        .iter()
                        .map(|s| serde_json::json!({"sql":s.sql,"start":s.start,"end":s.end}))
                        .collect(),
                ))
            }
            _ => Err(error("UNKNOWN_OPERATION", "Operation is not available")),
        }
    }
}

struct DesktopResults<'a> {
    host: &'a DesktopHost,
    execution: Uuid,
}
impl query_runtime::ScriptResults for DesktopResults<'_> {
    fn begin(&self, statement: usize) -> DbResult<Uuid> {
        let id = Uuid::new_v4();
        let spool = Spool::new(
            &self.host.spool_directory,
            id,
            self.host.spool_total.clone(),
        )?;
        let mut executions = lock(&self.host.executions)?;
        let e = executions
            .get_mut(&self.execution)
            .ok_or_else(|| error("EXECUTION_NOT_FOUND", "Execution is closed"))?;
        e.spools.insert(id, Arc::new(Mutex::new(spool)));
        e.info.results.push(ResultInfo {
            id,
            statement,
            columns: Vec::new(),
            rows: 0,
            terminal: None,
        });
        e.info.status = QueryStatus::Running;
        Ok(id)
    }
    fn active(&self, query: Option<Uuid>) -> DbResult<()> {
        let mut executions = lock(&self.host.executions)?;
        let e = executions
            .get_mut(&self.execution)
            .ok_or_else(|| error("EXECUTION_NOT_FOUND", "Execution is closed"))?;
        e.active_query = query;
        Ok(())
    }
    fn columns(&self, result: Uuid, columns: &[Column]) -> DbResult<()> {
        let mut executions = lock(&self.host.executions)?;
        let e = executions
            .get_mut(&self.execution)
            .ok_or_else(|| error("EXECUTION_NOT_FOUND", "Execution is closed"))?;
        let bytes = e
            .info
            .results
            .iter()
            .try_fold(0usize, |n, r| -> DbResult<usize> {
                Ok(n.saturating_add(serde_json::to_vec(&r.columns).map_err(json_error)?.len()))
            })?;
        if bytes.saturating_add(serde_json::to_vec(columns).map_err(json_error)?.len())
            > 2 * 1024 * 1024
        {
            return Err(error(
                "METADATA_CAPACITY_EXCEEDED",
                "Execution metadata exceeds 2 MiB; available results are partial",
            ));
        }
        if let Some(r) = e.info.results.iter_mut().find(|r| r.id == result) {
            r.columns = columns.to_vec();
        }
        Ok(())
    }
    fn rows(&self, result: Uuid, rows: &[ResultRow]) -> DbResult<()> {
        let spool = lock(&self.host.executions)?
            .get(&self.execution)
            .and_then(|e| e.spools.get(&result))
            .cloned()
            .ok_or_else(|| error("RESULT_NOT_FOUND", "Result storage missing"))?;
        let (written, count) = {
            let mut spool = lock(&spool)?;
            let written = spool.append(rows);
            (written, spool.rows())
        };
        if let Some(e) = lock(&self.host.executions)?.get_mut(&self.execution) {
            if let Some(r) = e.info.results.iter_mut().find(|r| r.id == result) {
                r.rows = count;
            }
            e.info.status = QueryStatus::Streaming;
        }
        written
    }
    fn terminal(&self, result: Uuid, event: serde_json::Value) -> DbResult<()> {
        if let Some(e) = lock(&self.host.executions)?.get_mut(&self.execution)
            && let Some(r) = e.info.results.iter_mut().find(|r| r.id == result)
        {
            r.terminal = Some(event);
        }
        Ok(())
    }
}
