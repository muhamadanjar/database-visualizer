//! Platform-independent database contracts. No driver, transport, or UI dependencies.
use serde::{Deserialize, Serialize};
use std::{fmt, time::Duration};
use uuid::Uuid;

pub type DbResult<T> = Result<T, DbError>;
pub type ConnectionId = Uuid;
pub type SessionId = Uuid;
pub type QueryId = Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Engine {
    Postgres,
    MySql,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SslMode {
    Disable,
    Prefer,
    Require,
    VerifyCa,
    VerifyFull,
}

/// Keep credentials runtime-only; deliberately not serializable and Debug is redacted.
#[derive(Clone)]
pub struct ConnectionConfig {
    pub id: ConnectionId,
    pub engine: Engine,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database: String,
    pub ssl: SslMode,
    pub connect_timeout: Duration,
}
impl fmt::Debug for ConnectionConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConnectionConfig")
            .field("id", &self.id)
            .field("engine", &self.engine)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Capabilities {
    pub schemas: bool,
    pub views: bool,
    pub materialized_views: bool,
    pub sequences: bool,
    pub procedures: bool,
    pub functions: bool,
    pub triggers: bool,
    pub custom_types: bool,
    pub query_cancel: bool,
    pub explain: bool,
}
impl Capabilities {
    pub fn for_engine(engine: Engine) -> Self {
        let pg = engine == Engine::Postgres;
        Self {
            schemas: pg,
            views: true,
            materialized_views: pg,
            sequences: pg,
            procedures: true,
            functions: true,
            triggers: true,
            custom_types: pg,
            query_cancel: true,
            explain: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum DbValue {
    Null,
    Boolean(bool),
    Signed(i64),
    Unsigned(u64),
    Float(String),
    Decimal(String),
    Text(String),
    Binary(Vec<u8>),
    Date(String),
    Time(String),
    Timestamp(String),
    Json(serde_json::Value),
    Opaque {
        database_type: String,
        format: String,
        payload: Vec<u8>,
    },
}
impl DbValue {
    /// Logical payload size; excludes driver allocations and Rust allocator overhead.
    pub fn payload_bytes(&self) -> usize {
        match self {
            Self::Null => 1,
            Self::Boolean(_) => 1,
            Self::Signed(_) | Self::Unsigned(_) => 8,
            Self::Float(s)
            | Self::Decimal(s)
            | Self::Text(s)
            | Self::Date(s)
            | Self::Time(s)
            | Self::Timestamp(s) => s.len(),
            Self::Binary(b) => b.len(),
            Self::Json(j) => j.to_string().len(),
            Self::Opaque {
                database_type,
                format,
                payload,
            } => database_type.len() + format.len() + payload.len(),
        }
    }
}
pub type ResultRow = Vec<DbValue>;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Column {
    pub name: String,
    pub database_type: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueryStatus {
    Created,
    Running,
    Streaming,
    Completed,
    Failed,
    Cancelled,
}
impl QueryStatus {
    pub fn terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataOutcome {
    ExecutionCompleted,
    NotExecuted,
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionState {
    Unknown,
}

#[derive(Clone, Debug)]
pub struct QueryRequest {
    pub session_id: SessionId,
    pub sql: String,
    pub timeout: Option<Duration>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ExecutionSummary {
    /// False when cancellation/disconnection discarded rows even though SQL completed.
    pub results_complete: bool,
    pub affected_rows: u64,
    pub rows_returned: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueryEvent {
    pub query_id: QueryId,
    pub session_id: SessionId,
    pub connection_id: ConnectionId,
    pub started_at_ms: u64,
    #[serde(flatten)]
    pub data: QueryEventData,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QueryEventData {
    QueryStarted,
    Columns {
        columns: Vec<Column>,
    },
    Rows {
        batch: u64,
        rows: Vec<ResultRow>,
    },
    QueryCompleted {
        summary: ExecutionSummary,
        elapsed_ms: u64,
    },
    QueryFailed {
        error: DbError,
        outcome: DataOutcome,
        session_valid: bool,
        elapsed_ms: u64,
    },
    QueryCancelled {
        outcome: DataOutcome,
        session_valid: bool,
        elapsed_ms: u64,
    },
}
impl QueryEventData {
    pub fn status(&self) -> QueryStatus {
        match self {
            Self::QueryStarted | Self::Columns { .. } => QueryStatus::Running,
            Self::Rows { .. } => QueryStatus::Streaming,
            Self::QueryCompleted { .. } => QueryStatus::Completed,
            Self::QueryFailed { .. } => QueryStatus::Failed,
            Self::QueryCancelled { .. } => QueryStatus::Cancelled,
        }
    }
    pub fn terminal(&self) -> bool {
        matches!(
            self,
            Self::QueryCompleted { .. } | Self::QueryFailed { .. } | Self::QueryCancelled { .. }
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbError {
    pub code: String,
    pub category: String,
    pub message: String,
    pub database_code: Option<String>,
}
impl DbError {
    pub fn new(code: &str, category: &str, message: &str) -> Self {
        Self {
            code: code.into(),
            category: category.into(),
            message: message.into(),
            database_code: None,
        }
    }
    pub fn cancelled() -> Self {
        Self::new(
            "QUERY_CANCELLED",
            "cancelled",
            "Database confirmed query cancellation",
        )
    }
    pub fn unknown() -> Self {
        Self::new(
            "QUERY_OUTCOME_UNKNOWN",
            "connection",
            "Query outcome is unknown; reconnect is required",
        )
    }
}
impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for DbError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObjectKind {
    Database,
    Schema,
    Table,
    View,
    Sequence,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectId {
    pub engine: Engine,
    pub database: String,
    pub schema: Option<String>,
    pub name: String,
    pub kind: ObjectKind,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum MetadataRequest {
    Databases,
    Schemas,
    Objects {
        schema: Option<String>,
        kind: ObjectKind,
    },
    Inspect {
        object: ObjectId,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ColumnMetadata {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub default: Option<String>,
    pub ordinal: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IndexMetadata {
    pub name: String,
    pub definition: String,
    pub unique: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConstraintMetadata {
    pub name: String,
    pub kind: String,
    pub definition: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MetadataResponse {
    pub objects: Vec<ObjectId>,
    pub columns: Vec<ColumnMetadata>,
    pub indexes: Vec<IndexMetadata>,
    pub constraints: Vec<ConstraintMetadata>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capabilities_preserve_engine_differences() {
        assert!(Capabilities::for_engine(Engine::Postgres).sequences);
        assert!(!Capabilities::for_engine(Engine::MySql).schemas);
    }
    #[test]
    fn values_preserve_null_and_precision() {
        let v = DbValue::Decimal("9007199254740993.123456789".into());
        assert_eq!(
            serde_json::from_str::<DbValue>(&serde_json::to_string(&v).unwrap()).unwrap(),
            v
        );
        assert_ne!(DbValue::Null, DbValue::Text("NULL".into()));
        assert!(QueryStatus::Cancelled.terminal());
        assert!(!QueryStatus::Streaming.terminal());
    }
}
