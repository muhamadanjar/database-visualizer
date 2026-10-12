use crate::{BackendSession, DatabaseAdapter, ResultSink, validate_metadata, validate_sql};
use async_trait::async_trait;
use db_types::*;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex as StdMutex, Weak,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore, mpsc, watch};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct RuntimeLimits {
    pub sessions_per_profile: usize,
    pub sessions_total: usize,
    pub metadata_per_profile: usize,
    pub metadata_total: usize,
    pub control_per_profile: usize,
    pub control_total: usize,
    pub batch_rows: usize,
    pub batch_bytes: usize,
    pub queue_bytes: usize,
    pub consumer_timeout: Duration,
    pub cleanup_timeout: Duration,
}
impl Default for RuntimeLimits {
    fn default() -> Self {
        Self {
            sessions_per_profile: 4,
            sessions_total: 16,
            metadata_per_profile: 1,
            metadata_total: 4,
            control_per_profile: 1,
            control_total: 4,
            batch_rows: 256,
            batch_bytes: 1024 * 1024,
            queue_bytes: 4 * 1024 * 1024,
            consumer_timeout: Duration::from_secs(30),
            cleanup_timeout: Duration::from_secs(5),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtectedTarget {
    pub host: String,
    pub port: u16,
    pub database: String,
}
struct Profile {
    sessions: Arc<Semaphore>,
    metadata: Arc<Semaphore>,
    control: Arc<Semaphore>,
}
struct Session {
    config: ConnectionConfig,
    adapter: Arc<dyn DatabaseAdapter>,
    backend: Mutex<Box<dyn BackendSession>>,
    valid: AtomicBool,
    busy: AtomicBool,
    profile: Arc<Profile>,
    permits: StdMutex<Option<Vec<OwnedSemaphorePermit>>>,
}
struct ActiveQuery {
    session_id: SessionId,
    cancel: CancellationToken,
}
struct Inner {
    limits: RuntimeLimits,
    adapters: HashMap<Engine, Arc<dyn DatabaseAdapter>>,
    sessions: Mutex<HashMap<SessionId, Arc<Session>>>,
    queries: Mutex<HashMap<QueryId, Arc<ActiveQuery>>>,
    profiles: StdMutex<HashMap<ConnectionId, Weak<Profile>>>,
    session_slots: Arc<Semaphore>,
    metadata_slots: Arc<Semaphore>,
    control_slots: Arc<Semaphore>,
    protected: Vec<ProtectedTarget>,
    shutdown: CancellationToken,
}
#[derive(Clone)]
pub struct DatabaseCore {
    inner: Arc<Inner>,
    owners: Arc<()>,
}
impl Drop for DatabaseCore {
    fn drop(&mut self) {
        if Arc::strong_count(&self.owners) == 1 {
            self.inner.shutdown.cancel();
        }
    }
}
impl DatabaseCore {
    pub fn new(
        adapters: Vec<Arc<dyn DatabaseAdapter>>,
        limits: RuntimeLimits,
        protected: Vec<ProtectedTarget>,
    ) -> DbResult<Self> {
        if [
            limits.sessions_total,
            limits.sessions_per_profile,
            limits.metadata_total,
            limits.metadata_per_profile,
            limits.control_total,
            limits.control_per_profile,
            limits.queue_bytes,
        ]
        .iter()
        .any(|n| *n > Semaphore::MAX_PERMITS)
            || limits.sessions_total == 0
            || limits.sessions_per_profile == 0
            || limits.metadata_total == 0
            || limits.metadata_per_profile == 0
            || limits.control_total == 0
            || limits.control_per_profile == 0
            || limits.batch_rows == 0
            || limits.batch_bytes == 0
            || limits.queue_bytes < limits.batch_bytes
            || limits.queue_bytes > u32::MAX as usize
            || limits.consumer_timeout.is_zero()
            || limits.cleanup_timeout.is_zero()
        {
            return Err(DbError::new(
                "INVALID_LIMITS",
                "configuration",
                "Limits must be positive, with batch bytes within queue capacity",
            ));
        }
        let mut map = HashMap::new();
        for adapter in adapters {
            if map.insert(adapter.engine(), adapter).is_some() {
                return Err(DbError::new(
                    "DUPLICATE_ADAPTER",
                    "configuration",
                    "Register only one adapter per engine",
                ));
            }
        }
        Ok(Self {
            owners: Arc::new(()),
            inner: Arc::new(Inner {
                session_slots: Arc::new(Semaphore::new(limits.sessions_total)),
                metadata_slots: Arc::new(Semaphore::new(limits.metadata_total)),
                control_slots: Arc::new(Semaphore::new(limits.control_total)),
                limits,
                adapters: map,
                sessions: Mutex::new(HashMap::new()),
                queries: Mutex::new(HashMap::new()),
                profiles: StdMutex::new(HashMap::new()),
                protected,
                shutdown: CancellationToken::new(),
            }),
        })
    }
    fn check_config(&self, config: &ConnectionConfig) -> DbResult<()> {
        if self.inner.shutdown.is_cancelled() {
            return Err(DbError::new(
                "HOST_CLOSED",
                "connection",
                "Core has been shut down",
            ));
        }
        if config.connect_timeout.is_zero() {
            return Err(DbError::new(
                "INVALID_TIMEOUT",
                "configuration",
                "Connect timeout must be positive",
            ));
        }
        if self.inner.protected.iter().any(|p| {
            p.host.eq_ignore_ascii_case(&config.host)
                && p.port == config.port
                && p.database == config.database
        }) {
            return Err(DbError::new(
                "PROTECTED_TARGET",
                "permission",
                "Customer query execution cannot target the configured control plane",
            ));
        }
        Ok(())
    }
    fn profile(&self, id: ConnectionId) -> DbResult<Arc<Profile>> {
        let mut profiles = self.inner.profiles.lock().map_err(|_| DbError::unknown())?;
        profiles.retain(|_, p| p.strong_count() > 0);
        if let Some(profile) = profiles.get(&id).and_then(Weak::upgrade) {
            return Ok(profile);
        }
        let l = &self.inner.limits;
        let profile = Arc::new(Profile {
            sessions: Arc::new(Semaphore::new(l.sessions_per_profile)),
            metadata: Arc::new(Semaphore::new(l.metadata_per_profile)),
            control: Arc::new(Semaphore::new(l.control_per_profile)),
        });
        profiles.insert(id, Arc::downgrade(&profile));
        Ok(profile)
    }
    fn adapter(&self, engine: Engine) -> DbResult<Arc<dyn DatabaseAdapter>> {
        self.inner.adapters.get(&engine).cloned().ok_or_else(|| {
            DbError::new(
                "UNSUPPORTED_ENGINE",
                "configuration",
                "No adapter is registered for this engine",
            )
        })
    }
    pub fn capabilities(&self, engine: Engine) -> DbResult<Capabilities> {
        Ok(self.adapter(engine)?.capabilities())
    }
    pub async fn connect(&self, config: ConnectionConfig) -> DbResult<SessionId> {
        self.check_config(&config)?;
        let profile = self.profile(config.id)?;
        let permits = vec![slot(&self.inner.session_slots)?, slot(&profile.sessions)?];
        let adapter = self.adapter(config.engine)?;
        let backend = tokio::time::timeout(config.connect_timeout, adapter.connect(&config))
            .await
            .map_err(|_| {
                DbError::new(
                    "CONNECT_TIMEOUT",
                    "timeout",
                    "Database connection timed out",
                )
            })??;
        if self.inner.shutdown.is_cancelled() {
            return Err(DbError::new(
                "HOST_CLOSED",
                "connection",
                "Core has been shut down",
            ));
        }
        let id = Uuid::new_v4();
        self.inner.sessions.lock().await.insert(
            id,
            Arc::new(Session {
                config,
                adapter,
                backend: Mutex::new(backend),
                valid: AtomicBool::new(true),
                busy: AtomicBool::new(false),
                profile,
                permits: StdMutex::new(Some(permits)),
            }),
        );
        Ok(id)
    }
    pub async fn test_connection(&self, config: ConnectionConfig) -> DbResult<()> {
        let id = self.connect(config).await?;
        self.disconnect(id).await
    }
    pub async fn reconnect(&self, id: SessionId) -> DbResult<SessionId> {
        let config = self.session(id).await?.config.clone();
        self.disconnect(id).await?;
        self.connect(config).await
    }
    async fn session(&self, id: SessionId) -> DbResult<Arc<Session>> {
        self.inner
            .sessions
            .lock()
            .await
            .get(&id)
            .cloned()
            .ok_or_else(|| {
                DbError::new(
                    "SESSION_NOT_FOUND",
                    "connection",
                    "Session is not registered",
                )
            })
    }
    pub async fn session_valid(&self, id: SessionId) -> bool {
        self.session(id)
            .await
            .map(|s| s.valid.load(Ordering::Acquire))
            .unwrap_or(false)
    }
    pub async fn transaction_state(&self, id: SessionId) -> DbResult<TransactionState> {
        let session = self.session(id).await?;
        if !session.valid.load(Ordering::Acquire) || session.busy.load(Ordering::Acquire) {
            return Ok(TransactionState::Unknown);
        }
        let Ok(backend) = session.backend.try_lock() else {
            return Ok(TransactionState::Unknown);
        };
        Ok(backend.transaction_state())
    }
    pub async fn metadata(
        &self,
        config: &ConnectionConfig,
        request: &MetadataRequest,
    ) -> DbResult<MetadataResponse> {
        self.check_config(config)?;
        validate_metadata(config, request)?;
        let profile = self.profile(config.id)?;
        let _permits = [slot(&self.inner.metadata_slots)?, slot(&profile.metadata)?];
        tokio::time::timeout(
            config.connect_timeout,
            self.adapter(config.engine)?.metadata(config, request),
        )
        .await
        .map_err(|_| DbError::new("METADATA_TIMEOUT", "timeout", "Metadata request timed out"))?
    }
    pub async fn execute(&self, request: QueryRequest) -> DbResult<QueryStream> {
        self.execute_bound(request, Vec::new()).await
    }
    pub async fn execute_bound(
        &self,
        request: QueryRequest,
        parameters: Vec<String>,
    ) -> DbResult<QueryStream> {
        self.execute_parameters(request, crate::BoundParameters::Text(parameters))
            .await
    }
    pub async fn execute_typed(
        &self,
        request: QueryRequest,
        parameters: Vec<DbValue>,
    ) -> DbResult<QueryStream> {
        self.execute_parameters(request, crate::BoundParameters::Typed(parameters))
            .await
    }
    async fn execute_parameters(
        &self,
        request: QueryRequest,
        parameters: crate::BoundParameters,
    ) -> DbResult<QueryStream> {
        let session = self.session(request.session_id).await?;
        if !session.valid.load(Ordering::Acquire) {
            return Err(DbError::unknown());
        }
        validate_sql(session.config.engine, &request.sql)?;
        if request.timeout.is_some_and(|d| d.is_zero()) {
            return Err(DbError::new(
                "INVALID_TIMEOUT",
                "configuration",
                "Query timeout must be positive",
            ));
        }
        session
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| {
                DbError::new(
                    "SESSION_BUSY",
                    "query",
                    "Session already has an active query",
                )
            })?;
        let query_id = Uuid::new_v4();
        let cancel = CancellationToken::new();
        let (tx, rx) = mpsc::channel(32);
        let (terminal_tx, terminal_rx) = watch::channel(None);
        let (status_tx, status_rx) = watch::channel(QueryStatus::Created);
        let active = Arc::new(ActiveQuery {
            session_id: request.session_id,
            cancel: cancel.clone(),
        });
        self.inner.queries.lock().await.insert(query_id, active);
        let inner = self.inner.clone();
        let stream = QueryStream {
            query_id,
            rx,
            terminal: terminal_rx,
            status: status_rx,
            cancel: cancel.clone(),
            terminal_emitted: false,
        };
        tokio::spawn(async move {
            run_query(
                inner,
                session,
                request,
                parameters,
                query_id,
                cancel,
                QueryChannels {
                    tx,
                    terminal: terminal_tx,
                    status: status_tx,
                },
            )
            .await;
        });
        Ok(stream)
    }
    /// True means cancellation was requested; the terminal event confirms the actual result.
    pub async fn cancel(&self, id: QueryId) -> bool {
        if let Some(query) = self.inner.queries.lock().await.get(&id) {
            query.cancel.cancel();
            true
        } else {
            false
        }
    }
    pub async fn disconnect(&self, id: SessionId) -> DbResult<()> {
        let session = self
            .inner
            .sessions
            .lock()
            .await
            .remove(&id)
            .ok_or_else(|| {
                DbError::new(
                    "SESSION_NOT_FOUND",
                    "connection",
                    "Session is not registered",
                )
            })?;
        session.valid.store(false, Ordering::Release);
        let queries = self
            .inner
            .queries
            .lock()
            .await
            .values()
            .filter(|q| q.session_id == id)
            .cloned()
            .collect::<Vec<_>>();
        for query in queries {
            query.cancel.cancel();
        }
        let result = tokio::time::timeout(self.inner.limits.cleanup_timeout, async {
            session.backend.lock().await.close().await
        })
        .await;
        release(&session);
        match result {
            Ok(r) => r,
            Err(_) => Err(DbError::unknown()),
        }
    }
    pub async fn shutdown(&self) -> DbResult<()> {
        self.inner.shutdown.cancel();
        let queries = self
            .inner
            .queries
            .lock()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for query in queries {
            query.cancel.cancel();
        }
        let ids = self
            .inner
            .sessions
            .lock()
            .await
            .keys()
            .copied()
            .collect::<Vec<_>>();
        let mut error = None;
        for id in ids {
            if let Err(e) = self.disconnect(id).await {
                error = Some(e);
            }
        }
        match error {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}
fn slot(semaphore: &Arc<Semaphore>) -> DbResult<OwnedSemaphorePermit> {
    semaphore.clone().try_acquire_owned().map_err(|_| {
        DbError::new(
            "CAPACITY_EXCEEDED",
            "capacity",
            "Configured connection capacity is exhausted",
        )
    })
}
fn release(session: &Session) {
    if let Ok(mut permits) = session.permits.lock() {
        permits.take();
    }
}
struct Packet {
    event: QueryEvent,
    _permit: OwnedSemaphorePermit,
}
pub struct QueryStream {
    pub query_id: QueryId,
    rx: mpsc::Receiver<Packet>,
    terminal: watch::Receiver<Option<QueryEvent>>,
    status: watch::Receiver<QueryStatus>,
    cancel: CancellationToken,
    terminal_emitted: bool,
}
impl QueryStream {
    pub fn status(&self) -> QueryStatus {
        *self.status.borrow()
    }
    pub async fn recv(&mut self) -> Option<QueryEvent> {
        if self.terminal_emitted {
            return None;
        }
        if let Some(packet) = self.rx.recv().await {
            return Some(packet.event);
        }
        loop {
            if let Some(event) = self.terminal.borrow().clone() {
                self.terminal_emitted = true;
                return Some(event);
            }
            if self.terminal.changed().await.is_err() {
                self.terminal_emitted = true;
                return None;
            }
        }
    }
    pub async fn wait_terminal(&mut self) -> Option<QueryEvent> {
        loop {
            if let Some(event) = self.terminal.borrow().clone() {
                return Some(event);
            }
            if self.terminal.changed().await.is_err() {
                return None;
            }
        }
    }
}
impl Drop for QueryStream {
    fn drop(&mut self) {
        if !self.terminal_emitted {
            self.cancel.cancel();
        }
    }
}
struct BatchSink {
    status: watch::Sender<QueryStatus>,
    tx: mpsc::Sender<Packet>,
    budget: Arc<Semaphore>,
    limits: RuntimeLimits,
    cancel: CancellationToken,
    query_id: QueryId,
    session_id: SessionId,
    connection_id: ConnectionId,
    started_at_ms: u64,
    rows: Vec<ResultRow>,
    bytes: usize,
    batch: u64,
    discard: bool,
}
impl BatchSink {
    fn event(&self, data: QueryEventData) -> QueryEvent {
        QueryEvent {
            query_id: self.query_id,
            session_id: self.session_id,
            connection_id: self.connection_id,
            started_at_ms: self.started_at_ms,
            data,
        }
    }
    async fn send(&mut self, data: QueryEventData, bytes: usize) -> DbResult<()> {
        if self.cancel.is_cancelled() || self.discard {
            self.discard = true;
            return Ok(());
        }
        if bytes > self.limits.batch_bytes {
            return Err(DbError::new(
                "RESULT_ROW_TOO_LARGE",
                "result",
                "Result payload exceeds the configured batch byte limit",
            ));
        }
        self.status.send_replace(data.status());
        let budget = self.budget.clone();
        let tx = self.tx.clone();
        let event = self.event(data);
        let send = async move {
            let permit = budget
                .acquire_many_owned(bytes.max(1) as u32)
                .await
                .map_err(|_| DbError::unknown())?;
            tx.send(Packet {
                event,
                _permit: permit,
            })
            .await
            .map_err(|_| DbError::new("CONSUMER_CLOSED", "result", "Result consumer closed"))
        };
        tokio::select! {
            _=self.cancel.cancelled()=>{self.discard=true;Ok(())},
            result=tokio::time::timeout(self.limits.consumer_timeout,send)=>result.map_err(|_|DbError::new("CONSUMER_TIMEOUT","timeout","Result consumer did not read before the deadline"))?,
        }
    }
    async fn flush(&mut self) -> DbResult<()> {
        if self.rows.is_empty() {
            return Ok(());
        }
        self.batch += 1;
        let rows = std::mem::take(&mut self.rows);
        let bytes = std::mem::take(&mut self.bytes);
        self.send(
            QueryEventData::Rows {
                batch: self.batch,
                rows,
            },
            bytes,
        )
        .await
    }
}
#[async_trait]
impl ResultSink for BatchSink {
    async fn columns(&mut self, columns: Vec<Column>) -> DbResult<()> {
        let bytes = columns
            .iter()
            .map(|c| c.name.len() + c.database_type.len())
            .sum();
        self.send(QueryEventData::Columns { columns }, bytes).await
    }
    async fn row(&mut self, row: ResultRow) -> DbResult<()> {
        if self.cancel.is_cancelled() {
            self.discard = true;
            self.rows.clear();
            self.bytes = 0;
            return Ok(());
        }
        let bytes = row.iter().map(DbValue::payload_bytes).sum::<usize>().max(1);
        if bytes > self.limits.batch_bytes {
            return Err(DbError::new(
                "RESULT_ROW_TOO_LARGE",
                "result",
                "A row exceeds the configured batch byte limit",
            ));
        }
        if self.rows.len() >= self.limits.batch_rows || self.bytes + bytes > self.limits.batch_bytes
        {
            self.flush().await?;
        }
        self.bytes += bytes;
        self.rows.push(row);
        if self.rows.len() >= self.limits.batch_rows || self.bytes >= self.limits.batch_bytes {
            self.flush().await?;
        }
        Ok(())
    }
}
async fn request_cancel(inner: &Inner, session: &Session, backend_id: u64) -> DbResult<()> {
    let _permits = [slot(&inner.control_slots)?, slot(&session.profile.control)?];
    session.adapter.cancel(&session.config, backend_id).await
}
struct QueryChannels {
    tx: mpsc::Sender<Packet>,
    terminal: watch::Sender<Option<QueryEvent>>,
    status: watch::Sender<QueryStatus>,
}
async fn run_query(
    inner: Arc<Inner>,
    session: Arc<Session>,
    request: QueryRequest,
    parameters: crate::BoundParameters,
    query_id: QueryId,
    cancel: CancellationToken,
    channels: QueryChannels,
) {
    let started = Instant::now();
    let started_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let mut sink = BatchSink {
        tx: channels.tx,
        status: channels.status,
        budget: Arc::new(Semaphore::new(inner.limits.queue_bytes)),
        limits: inner.limits.clone(),
        cancel: cancel.clone(),
        query_id,
        session_id: request.session_id,
        connection_id: session.config.id,
        started_at_ms,
        rows: Vec::new(),
        bytes: 0,
        batch: 0,
        discard: false,
    };
    let _ = sink.send(QueryEventData::QueryStarted, 1).await;
    let mut backend = session.backend.lock().await;
    let backend_id = backend.backend_id();
    let mut valid = session.valid.load(Ordering::Acquire);
    let mut cleanup_end = None;
    let not_executed = cancel.is_cancelled() || !valid || inner.shutdown.is_cancelled();
    let result = if not_executed {
        Err(DbError::cancelled())
    } else {
        let job = async {
            match &parameters {
                crate::BoundParameters::Text(values) => {
                    backend.execute_bound(&request.sql, values, &mut sink).await
                }
                crate::BoundParameters::Typed(values) => {
                    backend.execute_typed(&request.sql, values, &mut sink).await
                }
            }
        };
        tokio::pin!(job);
        let deadline = async {
            match request.timeout {
                Some(d) => tokio::time::sleep(d).await,
                None => std::future::pending::<()>().await,
            }
        };
        tokio::pin!(deadline);
        tokio::select! {
            biased;
            result=&mut job=>result,
            _=async {tokio::select!{_=cancel.cancelled()=>{},_=inner.shutdown.cancelled()=>{},_=&mut deadline=>{}}}=>{
                cancel.cancel();
                let end=tokio::time::Instant::now()+inner.limits.cleanup_timeout;
                cleanup_end=Some(end);
                match tokio::time::timeout_at(end,request_cancel(&inner,&session,backend_id)).await {
                    Ok(Ok(()))=>match tokio::time::timeout_at(end,&mut job).await {Ok(r)=>r,Err(_)=>{valid=false;Err(DbError::unknown())}},
                    _=>{valid=false;Err(DbError::unknown())}
                }
            }
        }
    };
    let mut result = result;
    let cleanup_end =
        cleanup_end.unwrap_or_else(|| tokio::time::Instant::now() + inner.limits.cleanup_timeout);
    if let Err(error) = &result
        && matches!(
            error.code.as_str(),
            "RESULT_ROW_TOO_LARGE"
                | "UNSUPPORTED_DATA_TYPE"
                | "VALUE_DECODE_FAILED"
                | "CONSUMER_CLOSED"
                | "CONSUMER_TIMEOUT"
        )
    {
        cancel.cancel();
        if !matches!(
            tokio::time::timeout_at(cleanup_end, request_cancel(&inner, &session, backend_id))
                .await,
            Ok(Ok(()))
        ) {
            valid = false;
            result = Err(DbError::unknown());
        }
    }
    if result.as_ref().err().is_some_and(|e| {
        matches!(
            e.code.as_str(),
            "CONNECTION_ERROR" | "QUERY_OUTCOME_UNKNOWN"
        )
    }) {
        valid = false;
    }
    if valid
        && !matches!(
            tokio::time::timeout_at(cleanup_end, backend.synchronize()).await,
            Ok(Ok(()))
        )
    {
        valid = false;
        result = Err(DbError::unknown());
    }
    if valid && let Err(e) = sink.flush().await {
        result = Err(e);
    }
    if !session.valid.load(Ordering::Acquire) || inner.shutdown.is_cancelled() {
        valid = false;
    }
    if !valid {
        session.valid.store(false, Ordering::Release);
        let _ = tokio::time::timeout_at(cleanup_end, backend.close()).await;
        release(&session);
    }
    let elapsed_ms = started.elapsed().as_millis() as u64;
    let data = match result {
        Ok(mut summary) => {
            summary.results_complete = !sink.discard;
            QueryEventData::QueryCompleted {
                summary,
                elapsed_ms,
            }
        }
        Err(error) if error.code == "QUERY_CANCELLED" => QueryEventData::QueryCancelled {
            outcome: if not_executed {
                DataOutcome::NotExecuted
            } else {
                DataOutcome::Unknown
            },
            session_valid: valid,
            elapsed_ms,
        },
        Err(error) => QueryEventData::QueryFailed {
            error,
            outcome: DataOutcome::Unknown,
            session_valid: valid,
            elapsed_ms,
        },
    };
    session.busy.store(false, Ordering::Release);
    sink.status.send_replace(data.status());
    channels.terminal.send_replace(Some(sink.event(data)));
    inner.queries.lock().await.remove(&query_id);
}
