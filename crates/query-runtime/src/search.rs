//! Bounded encrypted-document search. Storage owners authenticate opaque cursors.
use crate::{error, now_ms};
use db_types::DbResult;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SearchRequest {
    pub kind: String,
    #[serde(default)]
    pub query: String,
    pub from_ms: Option<u64>,
    pub to_ms: Option<u64>,
    pub status: Option<String>,
    #[serde(default)]
    pub favorites_only: bool,
    pub cursor: Option<String>,
}
impl SearchRequest {
    pub fn validate(&self) -> DbResult<()> {
        if !matches!(self.kind.as_str(), "saved" | "history")
            || self.query.len() > 512
            || self
                .status
                .as_ref()
                .is_some_and(|s| !matches!(s.as_str(), "Completed" | "Failed" | "Cancelled"))
            || self.from_ms.is_some_and(|v| v > i64::MAX as u64)
            || self.to_ms.is_some_and(|v| v > i64::MAX as u64)
            || self.from_ms.zip(self.to_ms).is_some_and(|(a, b)| a > b)
            || self.cursor.as_ref().is_some_and(|s| s.len() > 8192)
        {
            return Err(error(
                "INVALID_SEARCH",
                "Search scope or filters are invalid",
            ));
        }
        Ok(())
    }
    pub fn matches(
        &self,
        name: &str,
        content: &serde_json::Value,
        created: u64,
        favorite: bool,
    ) -> bool {
        !self.from_ms.is_some_and(|v| created < v)
            && !self.to_ms.is_some_and(|v| created > v)
            && (!self.favorites_only || favorite)
            && self
                .status
                .as_ref()
                .is_none_or(|s| content["execution"]["status"].as_str() == Some(s.as_str()))
            && (name.to_lowercase().contains(&self.query.to_lowercase())
                || content["sql"]
                    .as_str()
                    .is_some_and(|sql| sql.to_lowercase().contains(&self.query.to_lowercase())))
    }
}
#[derive(Serialize, Deserialize)]
pub struct SearchCursor {
    pub request: SearchRequest,
    pub watermark: u64,
    pub before_ms: u64,
    pub before_id: Uuid,
    pub expires_ms: u64,
}
impl SearchCursor {
    pub fn initial(request: &SearchRequest) -> Self {
        let mut request = request.clone();
        request.cursor = None;
        Self {
            request,
            watermark: now_ms(),
            before_ms: u64::MAX / 2,
            before_id: Uuid::max(),
            expires_ms: now_ms() + 600_000,
        }
    }
    pub fn validate(&self, request: &SearchRequest) -> DbResult<()> {
        let mut request = request.clone();
        request.cursor = None;
        if self.request != request || self.expires_ms < now_ms() {
            return Err(error(
                "SEARCH_CURSOR_EXPIRED",
                "Restart search after filters change or cursor expires",
            ));
        }
        Ok(())
    }
}
pub struct SearchBudget {
    started: Instant,
    pub scanned: usize,
    bytes: usize,
}
impl Default for SearchBudget {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            scanned: 0,
            bytes: 0,
        }
    }
}
impl SearchBudget {
    pub fn remaining(&self) -> Duration {
        Duration::from_secs(2).saturating_sub(self.started.elapsed())
    }
    pub fn admit(&mut self, bytes: usize) -> bool {
        if self.scanned >= 1000
            || self.bytes.saturating_add(bytes) > 10 * 1024 * 1024
            || self.started.elapsed() >= Duration::from_secs(2)
        {
            return false;
        }
        self.scanned += 1;
        self.bytes += bytes;
        true
    }
}
