use db_types::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: Uuid,
    pub connection_id: Uuid,
    pub revision: u32,
    pub auto_commit: bool,
    pub transaction: TransactionState,
    pub valid: bool,
    pub busy: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResultInfo {
    pub id: Uuid,
    pub statement: usize,
    pub columns: Vec<Column>,
    pub rows: u64,
    pub terminal: Option<serde_json::Value>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecutionInfo {
    pub id: Uuid,
    pub session_id: Uuid,
    pub connection_id: Uuid,
    pub status: QueryStatus,
    pub results: Vec<ResultInfo>,
    pub error: Option<DbError>,
    pub started_at_ms: u64,
    pub finished_at_ms: Option<u64>,
    pub persistence_error: Option<DbError>,
}
