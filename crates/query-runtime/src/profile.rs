use crate::error;
use db_types::*;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Profile {
    pub id: Uuid,
    #[serde(default)]
    pub revision: u32,
    pub name: String,
    pub engine: Engine,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub database: String,
    pub ssl: String,
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout_ms: u64,
    pub query_timeout_ms: Option<u64>,
    #[serde(default)]
    pub history_enabled: bool,
    #[serde(default)]
    pub restore_sql: bool,
}
fn default_connect_timeout() -> u64 {
    10_000
}
impl Profile {
    pub fn config(&self, password: String) -> DbResult<ConnectionConfig> {
        if self.name.trim().is_empty()
            || self.host.trim().is_empty()
            || self.username.trim().is_empty()
            || self.database.trim().is_empty()
            || self.port == 0
            || self.connect_timeout_ms == 0
            || self.connect_timeout_ms > 120_000
            || self.query_timeout_ms == Some(0)
        {
            return Err(error(
                "INVALID_PROFILE",
                "Provide name, host, port, user, database, and valid timeouts",
            ));
        }
        let ssl = match self.ssl.as_str() {
            "verify_full" => SslMode::VerifyFull,
            "verify_ca" => SslMode::VerifyCa,
            "require" => SslMode::Require,
            "prefer" => SslMode::Prefer,
            "disable" => SslMode::Disable,
            _ => return Err(error("INVALID_TLS", "Choose a supported TLS mode")),
        };
        Ok(ConnectionConfig {
            id: self.id,
            engine: self.engine,
            host: self.host.clone(),
            tcp_address: None,
            port: self.port,
            username: self.username.clone(),
            password,
            database: self.database.clone(),
            ssl,
            connect_timeout: Duration::from_millis(self.connect_timeout_ms),
        })
    }
}
