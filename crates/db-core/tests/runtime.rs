//! Deterministic failure-path policy tests. Real driver coverage lives in foundation.rs.
use async_trait::async_trait;
use db_core::*;
use db_types::*;
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
struct Adapter {
    cancelled: CancellationToken,
    started: Arc<AtomicUsize>,
    reject: bool,
    hang_sync: bool,
}
struct Backend {
    cancelled: CancellationToken,
    started: Arc<AtomicUsize>,
    hang_sync: bool,
}
#[async_trait]
impl DatabaseAdapter for Adapter {
    fn engine(&self) -> Engine {
        Engine::Postgres
    }
    async fn connect(&self, _: &ConnectionConfig) -> DbResult<Box<dyn BackendSession>> {
        Ok(Box::new(Backend {
            cancelled: self.cancelled.clone(),
            started: self.started.clone(),
            hang_sync: self.hang_sync,
        }))
    }
    async fn metadata(
        &self,
        _: &ConnectionConfig,
        _: &MetadataRequest,
    ) -> DbResult<MetadataResponse> {
        Ok(MetadataResponse::default())
    }
    async fn cancel(&self, _: &ConnectionConfig, _: u64) -> DbResult<()> {
        if self.reject {
            Err(DbError::new(
                "CANCEL_REJECTED",
                "permission",
                "Cancellation denied",
            ))
        } else {
            self.cancelled.cancel();
            Ok(())
        }
    }
}
#[async_trait]
impl BackendSession for Backend {
    fn backend_id(&self) -> u64 {
        1
    }
    async fn execute(&mut self, _: &str, _: &mut dyn ResultSink) -> DbResult<ExecutionSummary> {
        self.started.fetch_add(1, Ordering::SeqCst);
        self.cancelled.cancelled().await;
        Err(DbError::cancelled())
    }
    async fn synchronize(&mut self) -> DbResult<()> {
        if self.hang_sync {
            std::future::pending::<()>().await;
        }
        Ok(())
    }
    async fn close(&mut self) -> DbResult<()> {
        Ok(())
    }
}
fn config() -> ConnectionConfig {
    ConnectionConfig {
        id: Uuid::new_v4(),
        engine: Engine::Postgres,
        host: "test-host".into(),
        port: 5432,
        username: "test".into(),
        password: "redacted".into(),
        database: "fixture".into(),
        ssl: SslMode::Require,
        connect_timeout: Duration::from_secs(1),
    }
}
fn setup(reject: bool, hang_sync: bool) -> (DatabaseCore, Arc<AtomicUsize>) {
    let started = Arc::new(AtomicUsize::new(0));
    let adapter = Adapter {
        cancelled: CancellationToken::new(),
        started: started.clone(),
        reject,
        hang_sync,
    };
    let limits = RuntimeLimits {
        sessions_per_profile: 1,
        cleanup_timeout: Duration::from_millis(50),
        ..Default::default()
    };
    (
        DatabaseCore::new(vec![Arc::new(adapter)], limits, Vec::new()).unwrap(),
        started,
    )
}
async fn wait_started(started: &AtomicUsize) {
    tokio::time::timeout(Duration::from_secs(1), async {
        while started.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn denied_cancel_invalidates_session_and_releases_quota() {
    let (core, started) = setup(true, false);
    let config = config();
    let id = core.connect(config.clone()).await.unwrap();
    let mut stream = core
        .execute(QueryRequest {
            session_id: id,
            sql: "SELECT 1".into(),
            timeout: None,
        })
        .await
        .unwrap();
    wait_started(&started).await;
    core.cancel(stream.query_id).await;
    let event = stream.wait_terminal().await.unwrap();
    assert!(
        matches!(event.data,QueryEventData::QueryFailed {ref error,session_valid:false,..} if error.code=="QUERY_OUTCOME_UNKNOWN")
    );
    assert!(!core.session_valid(id).await);
    let next = core.reconnect(id).await.unwrap();
    assert_ne!(id, next);
    core.disconnect(next).await.unwrap();
}
#[tokio::test]
async fn cleanup_uses_one_deadline_and_marks_uncertainty() {
    let (core, started) = setup(false, true);
    let id = core.connect(config()).await.unwrap();
    let mut stream = core
        .execute(QueryRequest {
            session_id: id,
            sql: "SELECT 1".into(),
            timeout: None,
        })
        .await
        .unwrap();
    wait_started(&started).await;
    let start = Instant::now();
    core.cancel(stream.query_id).await;
    let event = stream.wait_terminal().await.unwrap();
    assert!(start.elapsed() < Duration::from_millis(200));
    assert!(matches!(
        event.data,
        QueryEventData::QueryFailed {
            session_valid: false,
            ..
        }
    ));
}
#[tokio::test]
async fn cancellation_before_start_never_executes_sql() {
    let (core, started) = setup(false, false);
    let id = core.connect(config()).await.unwrap();
    let mut stream = core
        .execute(QueryRequest {
            session_id: id,
            sql: "SELECT 1".into(),
            timeout: None,
        })
        .await
        .unwrap();
    core.cancel(stream.query_id).await;
    let event = stream.wait_terminal().await.unwrap();
    assert!(matches!(
        event.data,
        QueryEventData::QueryCancelled {
            outcome: DataOutcome::NotExecuted,
            ..
        }
    ));
    assert_eq!(started.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn query_timeout_requests_real_cancellation() {
    let (core, _) = setup(false, false);
    let id = core.connect(config()).await.unwrap();
    let mut stream = core
        .execute(QueryRequest {
            session_id: id,
            sql: "SELECT 1".into(),
            timeout: Some(Duration::from_millis(20)),
        })
        .await
        .unwrap();
    let event = stream.wait_terminal().await.unwrap();
    assert!(matches!(
        event.data,
        QueryEventData::QueryCancelled {
            session_valid: true,
            ..
        }
    ));
}
#[tokio::test]
async fn dropping_host_cancels_running_work() {
    let (core, started) = setup(false, false);
    let id = core.connect(config()).await.unwrap();
    let mut stream = core
        .execute(QueryRequest {
            session_id: id,
            sql: "SELECT 1".into(),
            timeout: None,
        })
        .await
        .unwrap();
    wait_started(&started).await;
    drop(core);
    let event = tokio::time::timeout(Duration::from_secs(1), stream.wait_terminal())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        event.data,
        QueryEventData::QueryCancelled {
            session_valid: false,
            ..
        }
    ));
}
#[test]
fn invalid_limits_return_errors() {
    let limits = RuntimeLimits {
        batch_bytes: 2,
        queue_bytes: 1,
        ..Default::default()
    };
    assert!(DatabaseCore::new(Vec::new(), limits, Vec::new()).is_err());
}
