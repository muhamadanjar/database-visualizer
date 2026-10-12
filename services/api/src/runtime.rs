use crate::{
    auth::Principal,
    config::Config,
    error::{ApiError, Result},
    store::{Scope, Store},
};
use db_core::{DatabaseCore, RuntimeLimits, prepare_script};
use db_types::*;
use query_runtime::{
    ExecutionInfo, Profile, ResultInfo, ScriptContext, ScriptResults, SessionInfo, Spool, WireRow,
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex, atomic::AtomicU64},
    time::{Duration, Instant},
};
use tokio::sync::Mutex as AsyncMutex;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(serde::Deserialize)]
pub struct Transform {
    pub column: Option<usize>,
    #[serde(default)]
    pub descending: bool,
    #[serde(default)]
    pub filter: String,
}
const GIB: u64 = 1024 * 1024 * 1024;
fn lock<T>(value: &Mutex<T>) -> DbResult<std::sync::MutexGuard<'_, T>> {
    value
        .lock()
        .map_err(|_| query_runtime::error("REGISTRY_UNAVAILABLE", "Runtime registry unavailable"))
}
#[derive(Clone)]
struct LiveSession {
    owner: Principal,
    scope: Scope,
    profile: Profile,
    auto_commit: bool,
    running: Option<Uuid>,
    lease: Instant,
}
struct LiveExecution {
    owner: Principal,
    scope: Scope,
    info: ExecutionInfo,
    spools: HashMap<Uuid, Arc<Mutex<Spool>>>,
    views: HashMap<Uuid, Arc<Mutex<Spool>>>,
    cancel: CancellationToken,
    active: Option<Uuid>,
}
pub struct Runtime {
    pub edits: query_runtime::edit::EditRuntime,
    pub edit_cancellations: Mutex<HashMap<Uuid, CancellationToken>>,
    pub edit_sessions: Mutex<HashMap<Uuid, Uuid>>,
    core: DatabaseCore,
    store: Arc<Store>,
    config: Arc<Config>,
    sessions: Mutex<HashMap<Uuid, LiveSession>>,
    executions: Mutex<HashMap<Uuid, LiveExecution>>,
    admission: AsyncMutex<()>,
    core_ids: Mutex<HashMap<(Uuid, Scope, Uuid), Uuid>>,
    directory: PathBuf,
    total: Arc<AtomicU64>,
    user_bytes: Mutex<HashMap<Uuid, Arc<AtomicU64>>>,
    workspace_bytes: Mutex<HashMap<Scope, Arc<AtomicU64>>>,
    instance_file: Mutex<Option<std::fs::File>>,
    stopping: std::sync::atomic::AtomicBool,
}
impl Runtime {
    pub fn open(store: Arc<Store>, config: Arc<Config>) -> Result<Arc<Self>> {
        query_runtime::private_directory(&config.spool_directory)?;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(config.spool_directory.join("instance.lock"))
            .map_err(|_| ApiError::unavailable())?;
        file.try_lock().map_err(|_| {
            ApiError::new(
                axum::http::StatusCode::CONFLICT,
                "INSTANCE_IN_USE",
                "This result directory belongs to another API instance",
            )
        })?;
        query_runtime::cleanup_spools(&config.spool_directory)?;
        let core = DatabaseCore::new(
            vec![
                Arc::new(db_postgres::PostgresAdapter),
                Arc::new(db_mysql::MySqlAdapter),
            ],
            RuntimeLimits::default(),
            Vec::new(),
        )?;
        Ok(Arc::new(Self {
            edits: query_runtime::edit::EditRuntime::default(),
            edit_cancellations: Mutex::new(HashMap::new()),
            edit_sessions: Mutex::new(HashMap::new()),
            core,
            directory: config.spool_directory.clone(),
            config,
            store,
            sessions: Mutex::new(HashMap::new()),
            executions: Mutex::new(HashMap::new()),
            admission: AsyncMutex::new(()),
            core_ids: Mutex::new(HashMap::new()),
            total: Arc::new(AtomicU64::new(0)),
            user_bytes: Mutex::new(HashMap::new()),
            workspace_bytes: Mutex::new(HashMap::new()),
            instance_file: Mutex::new(Some(file)),
            stopping: std::sync::atomic::AtomicBool::new(false),
        }))
    }
    pub fn edit_owner(
        scope: Scope,
        owner: &Principal,
        tab_id: Uuid,
    ) -> Result<query_runtime::edit::EditOwner> {
        use base64::Engine;
        let tab = owner.tab.as_ref().ok_or_else(ApiError::invalid)?;
        Ok(query_runtime::edit::EditOwner {
            user: owner.user.to_string(),
            tab: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&owner.login)
                + "/"
                + &base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(tab)
                + "/"
                + &tab_id.to_string(),
            scope: format!("{}/{}", scope.organization, scope.workspace),
        })
    }
    pub async fn edit_snapshot(
        &self,
        scope: Scope,
        owner: Principal,
        connection: Uuid,
        object: ObjectId,
        tab_id: Uuid,
        offset: u32,
    ) -> Result<query_runtime::edit::EditSnapshot> {
        let session = self.connect(scope, owner.clone(), connection).await?;
        let profile = self.session_profile(scope, &owner, session.id)?;
        let result = async {
            let metadata = self
                .metadata(
                    scope,
                    connection,
                    MetadataRequest::Inspect {
                        object: object.clone(),
                    },
                )
                .await?;
            self.edits
                .snapshot_page(
                    &self.core,
                    session.id,
                    &profile,
                    Self::edit_owner(scope, &owner, tab_id)?,
                    object,
                    metadata,
                    offset,
                )
                .await
                .map_err(ApiError::from)
        }
        .await;
        let cleanup = self.disconnect(scope, &owner, session.id, true).await;
        if result.is_ok() {
            cleanup?;
        }
        result
    }
    pub async fn edit_apply(
        &self,
        scope: Scope,
        owner: Principal,
        connection: Uuid,
        id: Uuid,
        batch: query_runtime::edit::EditBatch,
        object: ObjectId,
    ) -> Result<query_runtime::edit::EditOutcome> {
        let started = Instant::now();
        let remaining = || Duration::from_secs(30).saturating_sub(started.elapsed());
        let session =
            tokio::time::timeout(remaining(), self.connect(scope, owner.clone(), connection))
                .await
                .map_err(|_| ApiError::unavailable())??;
        lock(&self.edit_sessions)?.insert(id, session.id);
        let result = async {
            if lock(&self.edit_cancellations)?
                .get(&id)
                .is_some_and(|c| c.is_cancelled())
            {
                return Ok(query_runtime::edit::EditOutcome::Failed);
            }
            let profile = self.session_profile(scope, &owner, session.id)?;
            let metadata = tokio::time::timeout(
                remaining(),
                self.metadata(scope, connection, MetadataRequest::Inspect { object }),
            )
            .await
            .map_err(|_| ApiError::unavailable())??;
            self.edits
                .apply_with_deadline(
                    &self.core,
                    session.id,
                    &profile,
                    &Self::edit_owner(scope, &owner, batch.tab_id)?,
                    &batch,
                    &metadata,
                    remaining(),
                )
                .await
                .map_err(ApiError::from)
        }
        .await;
        let _ = self.disconnect(scope, &owner, session.id, true).await;
        lock(&self.edit_sessions)?.remove(&id);
        lock(&self.edit_cancellations)?.remove(&id);
        result
    }
    async fn config(&self, scope: Scope, profile: &Profile) -> Result<ConnectionConfig> {
        let address = self.config.network.resolve(profile).await?;
        let password = self.store.password(scope, profile.id).await?;
        let mut config = profile.config(password.to_string())?;
        config.tcp_address = Some(address);
        Ok(config)
    }
    pub async fn test(&self, profile: Profile, password: String) -> Result<()> {
        let address = self.config.network.resolve(&profile).await?;
        let password = zeroize::Zeroizing::new(password);
        let mut config = profile.config(password.to_string())?;
        config.tcp_address = Some(address);
        self.core.test_connection(config).await?;
        Ok(())
    }
    pub async fn metadata(
        &self,
        scope: Scope,
        connection: Uuid,
        request: MetadataRequest,
    ) -> Result<MetadataResponse> {
        let profile = self.store.profile(scope, connection).await?;
        let response = self
            .core
            .metadata(&self.config(scope, &profile).await?, &request)
            .await?;
        if serde_json::to_vec(&response)
            .map_err(|_| ApiError::unavailable())?
            .len()
            > 8 * 1024 * 1024
        {
            return Err(ApiError::new(
                axum::http::StatusCode::CONFLICT,
                "METADATA_CAPACITY_EXCEEDED",
                "Narrow the requested metadata scope",
            ));
        }
        Ok(response)
    }
    pub async fn connect(
        &self,
        scope: Scope,
        owner: Principal,
        connection: Uuid,
    ) -> Result<SessionInfo> {
        if owner.tab.is_none() {
            return Err(ApiError::invalid());
        }
        let _gate = self.admission.lock().await;
        if self.stopping.load(std::sync::atomic::Ordering::Acquire) {
            return Err(ApiError::unavailable());
        }
        crate::auth::principal_for_hash_store(&self.store, owner.login.clone()).await?;
        if !self.store.role(scope, owner.user).await?.execute() {
            return Err(ApiError::denied());
        }
        let profile = self.store.profile(scope, connection).await?;
        let mut config = self.config(scope, &profile).await?;
        {
            let sessions = lock(&self.sessions)?;
            if sessions.len() >= 16
                || sessions
                    .values()
                    .filter(|s| s.owner.user == owner.user)
                    .count()
                    >= 8
                || sessions.values().filter(|s| s.scope == scope).count() >= 8
                || sessions
                    .values()
                    .filter(|s| {
                        s.owner.user == owner.user && s.scope == scope && s.profile.id == connection
                    })
                    .count()
                    >= 4
            {
                return Err(ApiError::new(
                    axum::http::StatusCode::CONFLICT,
                    "SESSION_CAPACITY_EXCEEDED",
                    "Close a session before opening another SQL tab",
                ));
            }
        }
        config.id = *lock(&self.core_ids)?
            .entry((owner.user, scope, connection))
            .or_insert_with(Uuid::new_v4);
        let id = self.core.connect(config).await?;
        if crate::auth::principal_for_hash_store(&self.store, owner.login.clone())
            .await
            .is_err()
            || !self
                .store
                .role(scope, owner.user)
                .await
                .is_ok_and(|role| role.execute())
        {
            self.core.disconnect(id).await?;
            return Err(ApiError::denied());
        }
        lock(&self.sessions)?.insert(
            id,
            LiveSession {
                owner: owner.clone(),
                scope,
                profile,
                auto_commit: true,
                running: None,
                lease: Instant::now() + Duration::from_secs(30),
            },
        );
        self.session(scope, &owner, id).await
    }
    fn owned_session(&self, scope: Scope, owner: &Principal, id: Uuid) -> Result<LiveSession> {
        let sessions = lock(&self.sessions)?;
        let value = sessions.get(&id).ok_or_else(ApiError::missing)?;
        if value.scope != scope
            || value.owner.user != owner.user
            || value.owner.login != owner.login
            || value.owner.tab.is_none()
            || value.owner.tab != owner.tab
        {
            return Err(ApiError::missing());
        }
        if value.lease < Instant::now() {
            return Err(ApiError::new(
                axum::http::StatusCode::GONE,
                "SESSION_LEASE_EXPIRED",
                "This tab session expired; reconnect explicitly without replaying SQL",
            ));
        }
        Ok(value.clone())
    }
    pub fn session_profile(&self, scope: Scope, owner: &Principal, id: Uuid) -> Result<Profile> {
        Ok(self.owned_session(scope, owner, id)?.profile)
    }
    pub async fn session(&self, scope: Scope, owner: &Principal, id: Uuid) -> Result<SessionInfo> {
        let live = self.owned_session(scope, owner, id)?;
        Ok(SessionInfo {
            id,
            connection_id: live.profile.id,
            revision: live.profile.revision,
            auto_commit: live.auto_commit,
            transaction: self.core.transaction_state(id).await?,
            valid: self.core.session_valid(id).await,
            busy: live.running.is_some(),
        })
    }
    pub fn renew(&self, scope: Scope, owner: &Principal, id: Uuid) -> Result<()> {
        self.owned_session(scope, owner, id)?;
        if let Some(live) = lock(&self.sessions)?.get_mut(&id) {
            live.lease = Instant::now() + Duration::from_secs(30);
        }
        Ok(())
    }
    pub async fn auto_commit(
        &self,
        scope: Scope,
        owner: &Principal,
        id: Uuid,
        enabled: bool,
    ) -> Result<SessionInfo> {
        let _gate = self.admission.lock().await;
        let info = self.session(scope, owner, id).await?;
        if info.busy || info.transaction != TransactionState::Idle {
            return Err(ApiError::new(
                axum::http::StatusCode::CONFLICT,
                "TRANSACTION_RESOLUTION_REQUIRED",
                "Wait and Commit/Rollback before changing mode",
            ));
        }
        lock(&self.sessions)?
            .get_mut(&id)
            .ok_or_else(ApiError::missing)?
            .auto_commit = enabled;
        self.session(scope, owner, id).await
    }
    pub async fn disconnect(
        &self,
        scope: Scope,
        owner: &Principal,
        id: Uuid,
        force: bool,
    ) -> Result<()> {
        let _gate = self.admission.lock().await;
        let live = {
            let values = lock(&self.sessions)?;
            let live = values.get(&id).ok_or_else(ApiError::missing)?;
            if live.scope != scope
                || live.owner.user != owner.user
                || live.owner.login != owner.login
                || live.owner.tab != owner.tab
            {
                return Err(ApiError::missing());
            }
            live.clone()
        };
        let state = if force {
            TransactionState::Unknown
        } else {
            self.core.transaction_state(id).await?
        };
        if !force && (live.running.is_some() || state != TransactionState::Idle) {
            return Err(ApiError::new(
                axum::http::StatusCode::CONFLICT,
                "TRANSACTION_RESOLUTION_REQUIRED",
                "Cancel or resolve the transaction, or explicitly disconnect",
            ));
        }
        if let Some(execution) = live.running {
            self.cancel(scope, owner, execution).await?;
        }
        self.core.disconnect(id).await?;
        if let Some(execution) = live.running {
            tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    if self
                        .snapshot(scope, owner, execution)
                        .is_ok_and(|info| info.status.terminal())
                    {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .map_err(|_| {
                ApiError::new(
                    axum::http::StatusCode::CONFLICT,
                    "CLEANUP_PENDING",
                    "Execution cleanup is pending",
                )
            })?;
        }
        lock(&self.sessions)?.remove(&id);
        Ok(())
    }
    pub fn snapshot(&self, scope: Scope, owner: &Principal, id: Uuid) -> Result<ExecutionInfo> {
        let executions = lock(&self.executions)?;
        let execution = executions.get(&id).ok_or_else(ApiError::missing)?;
        if execution.scope != scope
            || execution.owner.user != owner.user
            || execution.owner.login != owner.login
            || execution.owner.tab.is_none()
            || execution.owner.tab != owner.tab
        {
            return Err(ApiError::missing());
        }
        Ok(execution.info.clone())
    }
    pub fn page(
        &self,
        scope: Scope,
        owner: &Principal,
        id: Uuid,
        result: Uuid,
        offset: u64,
        limit: usize,
    ) -> Result<Vec<WireRow>> {
        self.snapshot(scope, owner, id)?;
        if limit == 0 || limit > 256 {
            return Err(ApiError::invalid());
        }
        let spool = {
            let executions = lock(&self.executions)?;
            let execution = executions.get(&id).ok_or_else(ApiError::missing)?;
            execution
                .views
                .get(&result)
                .or_else(|| execution.spools.get(&result))
                .cloned()
                .ok_or_else(ApiError::missing)?
        };
        Ok(lock(&spool)?.page(offset, limit)?)
    }
    pub fn transform(
        &self,
        scope: Scope,
        owner: &Principal,
        id: Uuid,
        result: Uuid,
        options: &Transform,
    ) -> Result<ExecutionInfo> {
        let Transform {
            column,
            descending,
            filter,
        } = options;
        let column = *column;
        let descending = *descending;
        let info = self.snapshot(scope, owner, id)?;
        let result_info = info
            .results
            .iter()
            .find(|r| r.id == result)
            .ok_or_else(ApiError::missing)?;
        if !info.status.terminal()
            || filter.len() > 4096
            || column.is_some_and(|c| c >= result_info.columns.len())
        {
            return Err(ApiError::invalid());
        }
        let source = lock(&self.executions)?
            .get(&id)
            .and_then(|e| e.spools.get(&result))
            .cloned()
            .ok_or_else(ApiError::missing)?;
        let view = if column.is_none() && filter.is_empty() {
            None
        } else {
            Some(lock(&source)?.transform(column, descending, filter)?)
        };
        let rows = match &view {
            Some(view) => view.rows(),
            None => lock(&source)?.rows(),
        };
        let mut executions = lock(&self.executions)?;
        let execution = executions.get_mut(&id).ok_or_else(ApiError::missing)?;
        if let Some(view) = view {
            execution.views.insert(result, Arc::new(Mutex::new(view)));
        } else {
            execution.views.remove(&result);
        }
        if let Some(result) = execution.info.results.iter_mut().find(|r| r.id == result) {
            result.rows = rows;
        }
        Ok(execution.info.clone())
    }
    pub async fn cancel(&self, scope: Scope, owner: &Principal, id: Uuid) -> Result<()> {
        self.snapshot(scope, owner, id)?;
        let query = {
            let mut executions = lock(&self.executions)?;
            let execution = executions.get_mut(&id).ok_or_else(ApiError::missing)?;
            execution.cancel.cancel();
            execution.active
        };
        if let Some(query) = query {
            self.core.cancel(query).await;
        }
        Ok(())
    }
    pub fn close(&self, scope: Scope, owner: &Principal, id: Uuid) -> Result<()> {
        if !self.snapshot(scope, owner, id)?.status.terminal() {
            return Err(ApiError::new(
                axum::http::StatusCode::CONFLICT,
                "EXECUTION_BUSY",
                "Cancel and wait before closing results",
            ));
        }
        lock(&self.executions)?.remove(&id);
        Ok(())
    }
    pub async fn start(
        self: &Arc<Self>,
        scope: Scope,
        owner: Principal,
        session: Uuid,
        id: Uuid,
        sql: String,
        parameters: Vec<String>,
    ) -> Result<()> {
        let _gate = self.admission.lock().await;
        if self.stopping.load(std::sync::atomic::Ordering::Acquire) {
            return Err(ApiError::unavailable());
        }
        crate::auth::principal_for_hash_store(&self.store, owner.login.clone()).await?;
        if !self.store.role(scope, owner.user).await?.execute() {
            return Err(ApiError::denied());
        }
        let live = self.owned_session(scope, &owner, session)?;
        let statements = prepare_script(live.profile.engine, &sql)?;
        let cancel = CancellationToken::new();
        {
            let mut executions = lock(&self.executions)?;
            if executions.len() >= 64 {
                return Err(ApiError::new(
                    axum::http::StatusCode::CONFLICT,
                    "RESULT_CAPACITY_EXCEEDED",
                    "Close old results before executing",
                ));
            }
            let mut sessions = lock(&self.sessions)?;
            let live_session = sessions.get_mut(&session).ok_or_else(ApiError::missing)?;
            if live_session.running.is_some() {
                return Err(ApiError::new(
                    axum::http::StatusCode::CONFLICT,
                    "SESSION_BUSY",
                    "This tab has an active execution",
                ));
            }
            live_session.running = Some(id);
            live_session.lease = Instant::now() + Duration::from_secs(30);
            executions.insert(
                id,
                LiveExecution {
                    owner: owner.clone(),
                    scope,
                    info: ExecutionInfo {
                        id,
                        session_id: session,
                        connection_id: live.profile.id,
                        status: QueryStatus::Created,
                        results: Vec::new(),
                        error: None,
                        started_at_ms: query_runtime::now_ms(),
                        finished_at_ms: None,
                        persistence_error: None,
                    },
                    spools: HashMap::new(),
                    views: HashMap::new(),
                    cancel: cancel.clone(),
                    active: None,
                },
            );
        }
        let runtime = self.clone();
        tokio::spawn(async move {
            let outcome = query_runtime::run_script(
                ScriptContext {
                    core: &runtime.core,
                    session,
                    engine: live.profile.engine,
                    auto_commit: live.auto_commit,
                    timeout: live.profile.query_timeout_ms.map(Duration::from_millis),
                    parameters,
                    cancel,
                },
                statements,
                &WebResults {
                    runtime: &runtime,
                    id,
                },
            )
            .await;
            let mut info = match runtime.snapshot(scope, &owner, id) {
                Ok(info) => info,
                Err(_) => return,
            };
            info.finished_at_ms = Some(query_runtime::now_ms());
            match outcome {
                Ok(status) => info.status = status,
                Err(error) => {
                    info.status = QueryStatus::Failed;
                    info.error = Some(error);
                }
            }
            if live.profile.history_enabled
                && let Err(error) = runtime
                    .store
                    .save_history(scope, owner.user, &live.profile, id, &sql, &info)
                    .await
            {
                info.persistence_error =
                    Some(DbError::new(error.code, "persistence", error.message));
            }
            if let Err(error) = runtime
                .store
                .finish_execution(scope, owner.user, &info)
                .await
            {
                info.persistence_error =
                    Some(DbError::new(error.code, "persistence", error.message));
            }
            if let Ok(mut sessions) = lock(&runtime.sessions)
                && let Some(session) = sessions.get_mut(&session)
            {
                session.running = None;
            }
            if let Ok(mut executions) = lock(&runtime.executions)
                && let Some(execution) = executions.get_mut(&id)
            {
                execution.info = info;
                execution.active = None;
            }
        });
        Ok(())
    }
    pub async fn revoke_login(&self, login: &[u8]) {
        use base64::Engine;
        self.edits.discard_tab_prefix(
            &(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(login) + "/"),
        );

        let values = lock(&self.sessions)
            .map(|sessions| {
                sessions
                    .iter()
                    .filter(|(_, session)| session.owner.login == login)
                    .map(|(id, session)| (*id, session.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for (id, session) in &values {
            let _ = id;
            if let Some(execution) = session.running {
                let _ = self.cancel(session.scope, &session.owner, execution).await;
            }
        }
        for (id, session) in values {
            let _ = self
                .disconnect(session.scope, &session.owner, id, true)
                .await;
        }
        if let Ok(mut executions) = lock(&self.executions) {
            executions.retain(|_, execution| {
                execution.owner.login != login || !execution.info.status.terminal()
            });
        }
    }
    pub async fn invalidate(&self, scope: Scope, user: Option<Uuid>, connection: Option<Uuid>) {
        if let Some(user) = user {
            let _ = sqlx::query("DELETE FROM saved_query_favorites WHERE organization_id=$1 AND workspace_id=$2 AND user_id=$3")
                .bind(scope.organization).bind(scope.workspace).bind(user).execute(&self.store.pool).await;
        }
        self.edits.discard_scope(
            &format!("{}/{}", scope.organization, scope.workspace),
            user.map(|u| u.to_string()).as_deref(),
            connection,
        );
        let values = lock(&self.sessions)
            .map(|sessions| {
                sessions
                    .iter()
                    .filter(|(_, s)| {
                        s.scope == scope
                            && user.is_none_or(|u| u == s.owner.user)
                            && connection.is_none_or(|c| c == s.profile.id)
                    })
                    .map(|(id, s)| (*id, s.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for (id, session) in &values {
            let _ = id;
            if let Some(execution) = session.running {
                let _ = self.cancel(session.scope, &session.owner, execution).await;
            }
        }
        for (id, session) in values {
            let _ = self.disconnect(scope, &session.owner, id, true).await;
        }
        if let Ok(mut executions) = lock(&self.executions) {
            executions.retain(|_, e| {
                !(e.scope == scope
                    && user.is_none_or(|u| u == e.owner.user)
                    && connection.is_none_or(|c| c == e.info.connection_id)
                    && e.info.status.terminal())
            });
        }
    }
    pub async fn sweep(&self) {
        self.edits.prune();
        let sessions = lock(&self.sessions)
            .map(|s| s.iter().map(|(id, s)| (*id, s.clone())).collect::<Vec<_>>())
            .unwrap_or_default();
        // Never let a control-plane outage postpone cancellation of expired local leases.
        for (_, session) in &sessions {
            if session.lease < Instant::now()
                && let Some(execution) = session.running
            {
                let _ = self.cancel(session.scope, &session.owner, execution).await;
            }
        }
        use futures_util::{StreamExt, stream};
        let checks = stream::iter(sessions)
            .map(|(id, session)| async move {
                let valid = if session.lease < Instant::now() {
                    false
                } else {
                    tokio::time::timeout(Duration::from_secs(1), async {
                        crate::auth::principal_for_hash_store(
                            &self.store,
                            session.owner.login.clone(),
                        )
                        .await
                        .is_ok()
                            && self
                                .store
                                .role(session.scope, session.owner.user)
                                .await
                                .is_ok_and(|role| role.execute())
                    })
                    .await
                    .unwrap_or(false)
                };
                (id, session, valid)
            })
            .buffer_unordered(16)
            .collect::<Vec<_>>()
            .await;
        for (_, session, valid) in &checks {
            if !valid && let Some(execution) = session.running {
                let _ = self.cancel(session.scope, &session.owner, execution).await;
            }
        }
        for (id, session, valid) in checks {
            if !valid {
                let _ = self
                    .disconnect(session.scope, &session.owner, id, true)
                    .await;
            }
        }
        let completed = lock(&self.executions)
            .map(|values| {
                values
                    .iter()
                    .filter(|(_, e)| e.info.status.terminal())
                    .map(|(id, e)| (*id, e.scope, e.owner.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for (id, scope, owner) in completed {
            if (crate::auth::principal_for_hash_store(&self.store, owner.login.clone())
                .await
                .is_err()
                || !self
                    .store
                    .role(scope, owner.user)
                    .await
                    .is_ok_and(|role| role.execute()))
                && let Ok(mut values) = lock(&self.executions)
            {
                values.remove(&id);
            }
        }
        if let Ok(mut executions) = lock(&self.executions) {
            let now = query_runtime::now_ms();
            executions.retain(|_, e| {
                e.info
                    .finished_at_ms
                    .is_none_or(|finished| now.saturating_sub(finished) < 600_000)
            });
        }
        if let Ok(mut values) = lock(&self.user_bytes) {
            values.retain(|_, value| Arc::strong_count(value) > 1);
        }
        if let Ok(mut values) = lock(&self.workspace_bytes) {
            values.retain(|_, value| Arc::strong_count(value) > 1);
        }
        if let Ok(sessions) = lock(&self.sessions)
            && let Ok(mut ids) = lock(&self.core_ids)
        {
            ids.retain(|(user, scope, connection), _| {
                sessions.values().any(|s| {
                    s.owner.user == *user && s.scope == *scope && s.profile.id == *connection
                })
            });
        }
    }
    pub fn metrics(&self, scope: Scope) -> Result<serde_json::Value> {
        let sessions = lock(&self.sessions)?
            .values()
            .filter(|s| s.scope == scope)
            .count();
        let values = lock(&self.executions)?;
        let executions = values.values().filter(|e| e.scope == scope).count();
        let active = values
            .values()
            .filter(|e| e.scope == scope && !e.info.status.terminal())
            .count();
        let bytes = lock(&self.workspace_bytes)?
            .get(&scope)
            .map_or(0, |value| value.load(std::sync::atomic::Ordering::Acquire));
        Ok(
            serde_json::json!({"sessions":sessions,"executions":executions,"active_executions":active,"spool_bytes":bytes,"session_limit":8,"spool_limit_bytes":2*GIB}),
        )
    }
    pub fn stop_admission(&self) {
        self.stopping
            .store(true, std::sync::atomic::Ordering::Release);
    }
    pub async fn shutdown(&self) -> Result<()> {
        self.edits.clear();
        self.stopping
            .store(true, std::sync::atomic::Ordering::Release);
        let values = lock(&self.sessions)?
            .iter()
            .map(|(id, s)| (*id, s.clone()))
            .collect::<Vec<_>>();
        for (id, session) in values {
            let _ = self
                .disconnect(session.scope, &session.owner, id, true)
                .await;
        }
        self.core.shutdown().await?;
        lock(&self.executions)?.clear();
        lock(&self.instance_file)?.take();
        Ok(())
    }
}
struct WebResults<'a> {
    runtime: &'a Runtime,
    id: Uuid,
}
impl ScriptResults for WebResults<'_> {
    fn begin(&self, statement: usize) -> DbResult<Uuid> {
        let (scope, user) = {
            let executions = lock(&self.runtime.executions)?;
            let e = executions
                .get(&self.id)
                .ok_or_else(|| query_runtime::error("EXECUTION_NOT_FOUND", "Execution closed"))?;
            (e.scope, e.owner.user)
        };
        let user_bytes = lock(&self.runtime.user_bytes)?
            .entry(user)
            .or_insert_with(|| Arc::new(AtomicU64::new(0)))
            .clone();
        let workspace_bytes = lock(&self.runtime.workspace_bytes)?
            .entry(scope)
            .or_insert_with(|| Arc::new(AtomicU64::new(0)))
            .clone();
        let id = Uuid::new_v4();
        let spool = Spool::with_limits(
            &self.runtime.directory,
            id,
            self.runtime.total.clone(),
            GIB,
            4 * GIB,
        )?
        .with_budgets(vec![(user_bytes, GIB), (workspace_bytes, 2 * GIB)]);
        let mut executions = lock(&self.runtime.executions)?;
        let e = executions
            .get_mut(&self.id)
            .ok_or_else(|| query_runtime::error("EXECUTION_NOT_FOUND", "Execution closed"))?;
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
        if let Some(e) = lock(&self.runtime.executions)?.get_mut(&self.id) {
            e.active = query;
        }
        Ok(())
    }
    fn columns(&self, result: Uuid, columns: &[Column]) -> DbResult<()> {
        let mut executions = lock(&self.runtime.executions)?;
        let e = executions
            .get_mut(&self.id)
            .ok_or_else(|| query_runtime::error("EXECUTION_NOT_FOUND", "Execution closed"))?;
        let bytes = e
            .info
            .results
            .iter()
            .map(|r| {
                serde_json::to_vec(&r.columns)
                    .map(|v| v.len())
                    .unwrap_or(usize::MAX)
            })
            .fold(0usize, usize::saturating_add);
        if bytes.saturating_add(
            serde_json::to_vec(columns)
                .map_err(|_| {
                    query_runtime::error("RESULT_ENCODING_FAILED", "Result encoding failed")
                })?
                .len(),
        ) > 2 * 1024 * 1024
        {
            return Err(query_runtime::error(
                "METADATA_CAPACITY_EXCEEDED",
                "Execution metadata capacity reached; results are partial",
            ));
        }
        if let Some(r) = e.info.results.iter_mut().find(|r| r.id == result) {
            r.columns = columns.to_vec();
        }
        Ok(())
    }
    fn rows(&self, result: Uuid, rows: &[ResultRow]) -> DbResult<()> {
        let spool = lock(&self.runtime.executions)?
            .get(&self.id)
            .and_then(|e| e.spools.get(&result))
            .cloned()
            .ok_or_else(|| query_runtime::error("RESULT_NOT_FOUND", "Result closed"))?;
        let (outcome, count) = {
            let mut spool = lock(&spool)?;
            let outcome = spool.append(rows);
            (outcome, spool.rows())
        };
        if let Some(e) = lock(&self.runtime.executions)?.get_mut(&self.id) {
            if let Some(r) = e.info.results.iter_mut().find(|r| r.id == result) {
                r.rows = count;
            }
            e.info.status = QueryStatus::Streaming;
        }
        outcome
    }
    fn terminal(&self, result: Uuid, event: serde_json::Value) -> DbResult<()> {
        if let Some(e) = lock(&self.runtime.executions)?.get_mut(&self.id)
            && let Some(r) = e.info.results.iter_mut().find(|r| r.id == result)
        {
            r.terminal = Some(event);
        }
        Ok(())
    }
}
