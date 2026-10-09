//! MySql driver implementation. No host or transport dependencies.
use async_trait::async_trait;
use db_core::{BackendSession, DatabaseAdapter, ResultSink, validate_metadata};
use db_types::*;
use futures_util::TryStreamExt;
use sqlx::mysql::{MySqlConnectOptions, MySqlRow, MySqlSslMode, types::MySqlTime};
use sqlx::{
    Column as SqlColumn, ConnectOptions, Connection, Either, Executor, Row, TypeInfo, ValueRef,
};
mod metadata;

pub struct MySqlAdapter;
fn options(config: &ConnectionConfig) -> MySqlConnectOptions {
    MySqlConnectOptions::new()
        .host(&config.host)
        .port(config.port)
        .username(&config.username)
        .password(&config.password)
        .database(&config.database)
        .disable_statement_logging()
        .ssl_mode(match config.ssl {
            SslMode::Disable => MySqlSslMode::Disabled,
            SslMode::Prefer => MySqlSslMode::Preferred,
            SslMode::Require => MySqlSslMode::Required,
            SslMode::VerifyCa => MySqlSslMode::VerifyCa,
            SslMode::VerifyFull => MySqlSslMode::VerifyIdentity,
        })
}
async fn open(config: &ConnectionConfig) -> DbResult<sqlx::MySqlConnection> {
    tokio::time::timeout(
        config.connect_timeout,
        sqlx::MySqlConnection::connect_with(&options(config)),
    )
    .await
    .map_err(|_| {
        DbError::new(
            "CONNECT_TIMEOUT",
            "timeout",
            "Database connection timed out",
        )
    })?
    .map_err(map_error)
}
#[async_trait]
impl DatabaseAdapter for MySqlAdapter {
    fn engine(&self) -> Engine {
        Engine::MySql
    }
    async fn connect(&self, config: &ConnectionConfig) -> DbResult<Box<dyn BackendSession>> {
        let mut connection = open(config).await?;
        let id: u64 = sqlx::query_scalar("SELECT CONNECTION_ID()")
            .fetch_one(&mut connection)
            .await
            .map_err(map_error)?;
        Ok(Box::new(Session {
            connection: Some(connection),
            id: id as u64,
        }))
    }
    async fn cancel(&self, config: &ConnectionConfig, backend_id: u64) -> DbResult<()> {
        let mut connection = open(config).await?;
        // backend_id is captured from this session, never supplied as SQL by a user.
        connection
            .execute(format!("KILL QUERY {backend_id}").as_str())
            .await
            .map_err(map_error)?;
        connection.close().await.map_err(map_error)
    }
    async fn metadata(
        &self,
        config: &ConnectionConfig,
        request: &MetadataRequest,
    ) -> DbResult<MetadataResponse> {
        validate_metadata(config, request)?;
        let mut connection = open(config).await?;
        let response = metadata::read(&mut connection, config, request).await;
        let closed = connection.close().await.map_err(map_error);
        match response {
            Ok(r) => {
                closed?;
                Ok(r)
            }
            Err(e) => Err(e),
        }
    }
}
struct Session {
    connection: Option<sqlx::MySqlConnection>,
    id: u64,
}
#[async_trait]
impl BackendSession for Session {
    fn backend_id(&self) -> u64 {
        self.id
    }
    async fn execute(
        &mut self,
        sql: &str,
        sink: &mut dyn ResultSink,
    ) -> DbResult<ExecutionSummary> {
        let connection = self.connection.as_mut().ok_or_else(DbError::unknown)?;
        let mut emitted = false;
        let is_query = db_core::is_query(Engine::MySql, sql);
        if is_query {
            let description = connection.describe(sql).await.map_err(map_error)?;
            let columns = description
                .columns()
                .iter()
                .map(|c| Column {
                    name: c.name().into(),
                    database_type: c.type_info().name().into(),
                })
                .collect();
            sink.columns(columns).await?;
            emitted = true;
        }
        let mut summary = ExecutionSummary::default();
        let mut stream = connection.fetch_many(sql);
        while let Some(item) = stream.try_next().await.map_err(map_error)? {
            match item {
                Either::Left(result) => {
                    if !is_query {
                        summary.affected_rows += result.rows_affected();
                    }
                }
                Either::Right(row) => {
                    if !emitted {
                        sink.columns(
                            row.columns()
                                .iter()
                                .map(|c| Column {
                                    name: c.name().into(),
                                    database_type: c.type_info().name().into(),
                                })
                                .collect(),
                        )
                        .await?;
                        emitted = true;
                    }
                    sink.row(decode(&row)?).await?;
                    summary.rows_returned += 1;
                }
            }
        }
        if !emitted {
            sink.columns(Vec::new()).await?;
        }
        Ok(summary)
    }
    async fn synchronize(&mut self) -> DbResult<()> {
        let connection = self.connection.as_mut().ok_or_else(DbError::unknown)?;
        match connection.ping().await {
            Ok(()) => Ok(()),
            Err(sqlx::Error::Database(_)) => connection.ping().await.map_err(map_error),
            Err(e) => Err(map_error(e)),
        }
    }
    async fn close(&mut self) -> DbResult<()> {
        match self.connection.take() {
            Some(c) => c.close_hard().await.map_err(map_error),
            None => Ok(()),
        }
    }
}

fn map_error(error: sqlx::Error) -> DbError {
    if let Some(database) = error.as_database_error() {
        let code = database.code().map(|c| c.into_owned());
        if code.as_deref() == Some("70100") {
            return DbError::cancelled();
        }
        let number = database
            .try_downcast_ref::<sqlx::mysql::MySqlDatabaseError>()
            .map(|e| e.number());
        let category = match number {
            Some(1045) => "authentication",
            Some(1044 | 1142 | 1143 | 1227) => "permission",
            Some(1064) => "syntax",
            Some(1205 | 3024) => "timeout",
            _ => "database",
        };
        let mut result = DbError::new(
            "DATABASE_ERROR",
            category,
            "Database rejected the operation",
        );
        result.database_code = code;
        result
    } else {
        DbError::new(
            "CONNECTION_ERROR",
            "connection",
            "Database connection or protocol operation failed",
        )
    }
}
fn decode_error(_: sqlx::Error) -> DbError {
    DbError::new(
        "VALUE_DECODE_FAILED",
        "result",
        "Value could not be decoded losslessly",
    )
}

fn decode(row: &MySqlRow) -> DbResult<ResultRow> {
    row.columns()
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let raw = row.try_get_raw(i).map_err(decode_error)?;
            if raw.is_null() {
                return Ok(DbValue::Null);
            }
            let name = c.type_info().name();
            macro_rules! get {
                ($t:ty) => {
                    row.try_get::<$t, _>(i).map_err(decode_error)?
                };
            }
            Ok(match name {
                "BOOLEAN" => {
                    // BOOLEAN aliases TINYINT(1); values other than 0/1 remain integers.
                    let n = row.try_get_unchecked::<i64, _>(i).map_err(decode_error)?;
                    match n {
                        0 => DbValue::Boolean(false),
                        1 => DbValue::Boolean(true),
                        _ => DbValue::Signed(n),
                    }
                }
                "TINYINT" | "SMALLINT" | "INT" | "MEDIUMINT" | "BIGINT" => {
                    DbValue::Signed(get!(i64))
                }
                "TINYINT UNSIGNED" | "SMALLINT UNSIGNED" | "INT UNSIGNED"
                | "MEDIUMINT UNSIGNED" | "BIGINT UNSIGNED" => DbValue::Unsigned(get!(u64)),
                "FLOAT" => DbValue::Float(get!(f32).to_string()),
                "DOUBLE" => DbValue::Float(get!(f64).to_string()),
                "DECIMAL" => DbValue::Decimal(get!(bigdecimal::BigDecimal).to_string()),
                "CHAR" | "VARCHAR" | "TEXT" | "TINYTEXT" | "MEDIUMTEXT" | "LONGTEXT" => {
                    DbValue::Text(get!(String))
                }
                "BINARY" | "VARBINARY" | "BLOB" | "TINYBLOB" | "MEDIUMBLOB" | "LONGBLOB" => {
                    DbValue::Binary(get!(Vec<u8>))
                }
                "DATE" => DbValue::Date(get!(chrono::NaiveDate).to_string()),
                "TIME" => DbValue::Time(get!(MySqlTime).to_string()),
                "DATETIME" | "TIMESTAMP" => {
                    DbValue::Timestamp(get!(chrono::NaiveDateTime).to_string())
                }
                "JSON" => DbValue::Json(get!(serde_json::Value)),
                // This execution path uses the text protocol, preserving the original bytes.
                _ => DbValue::Opaque {
                    database_type: name.into(),
                    format: "Text".into(),
                    payload: row.try_get_unchecked::<Vec<u8>, _>(i).map_err(|_| {
                        DbError::new(
                            "UNSUPPORTED_DATA_TYPE",
                            "result",
                            "Raw payload is unavailable",
                        )
                    })?,
                },
            })
        })
        .collect()
}
