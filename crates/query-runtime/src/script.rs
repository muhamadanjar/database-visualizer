use db_core::{DatabaseCore, ScriptStatement, is_transaction_control};
use db_types::*;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Callbacks store results in a platform-owned, authorized registry. They never run SQL.
pub trait ScriptResults: Send + Sync {
    fn begin(&self, statement: usize) -> DbResult<Uuid>;
    fn active(&self, query: Option<Uuid>) -> DbResult<()>;
    fn columns(&self, result: Uuid, columns: &[Column]) -> DbResult<()>;
    fn rows(&self, result: Uuid, rows: &[ResultRow]) -> DbResult<()>;
    fn terminal(&self, result: Uuid, event: serde_json::Value) -> DbResult<()>;
}
pub struct ScriptContext<'a> {
    pub core: &'a DatabaseCore,
    pub session: Uuid,
    pub engine: Engine,
    pub auto_commit: bool,
    pub timeout: Option<Duration>,
    pub parameters: Vec<String>,
    pub cancel: CancellationToken,
}
pub fn safe_terminal(event: QueryEvent) -> DbResult<serde_json::Value> {
    let mut value = serde_json::to_value(event)
        .map_err(|_| crate::error("RESULT_ENCODING_FAILED", "Result encoding failed"))?;
    if let Some(summary) = value.get_mut("summary").and_then(|v| v.as_object_mut()) {
        for key in ["affected_rows", "rows_returned"] {
            if let Some(n) = summary.get(key).and_then(|v| v.as_u64()) {
                summary.insert(key.into(), serde_json::json!(n.to_string()));
            }
        }
    }
    Ok(value)
}
pub async fn run_script(
    context: ScriptContext<'_>,
    statements: Vec<ScriptStatement>,
    sink: &dyn ScriptResults,
) -> DbResult<QueryStatus> {
    let ScriptContext {
        core,
        session,
        engine,
        auto_commit,
        timeout,
        parameters,
        cancel,
    } = context;
    for (ordinal, statement) in statements.into_iter().enumerate() {
        if cancel.is_cancelled() {
            return Ok(QueryStatus::Cancelled);
        }
        if !auto_commit && !is_transaction_control(engine, &statement.sql) {
            match core.transaction_state(session).await? {
                TransactionState::Idle => {
                    let mut begin = core
                        .execute(QueryRequest {
                            session_id: session,
                            sql: "BEGIN".into(),
                            timeout: Some(Duration::from_secs(10)),
                        })
                        .await?;
                    let mut completed = false;
                    while let Some(event) = begin.recv().await {
                        if event.data.terminal() {
                            completed = matches!(event.data, QueryEventData::QueryCompleted { .. });
                        }
                    }
                    if !completed {
                        return Err(crate::error(
                            "BEGIN_FAILED",
                            "Transaction could not be started",
                        ));
                    }
                }
                TransactionState::Active => {}
                TransactionState::Failed => {
                    return Err(crate::error(
                        "TRANSACTION_FAILED",
                        "Rollback the failed transaction before executing",
                    ));
                }
                TransactionState::Unknown => {
                    return Err(crate::error(
                        "TRANSACTION_UNKNOWN",
                        "Reconnect or resolve the unknown transaction state",
                    ));
                }
            }
        }
        let result = sink.begin(ordinal)?;
        let mut stream = core
            .execute_bound(
                QueryRequest {
                    session_id: session,
                    sql: statement.sql,
                    timeout,
                },
                parameters.clone(),
            )
            .await?;
        sink.active(Some(stream.query_id))?;
        if cancel.is_cancelled() {
            core.cancel(stream.query_id).await;
        }
        let mut failure = None;
        let mut status = QueryStatus::Failed;
        while let Some(event) = stream.recv().await {
            let write = match &event.data {
                QueryEventData::Columns { columns } => sink.columns(result, columns),
                QueryEventData::Rows { rows, .. } if failure.is_none() => sink.rows(result, rows),
                data if data.terminal() => {
                    status = data.status();
                    let mut terminal = safe_terminal(event)?;
                    if failure.is_some()
                        && let Some(summary) = terminal.get_mut("summary")
                    {
                        summary["results_complete"] = false.into();
                    }
                    sink.terminal(result, terminal)?;
                    sink.active(None)?;
                    break;
                }
                _ => Ok(()),
            };
            if let Err(error) = write
                && failure.is_none()
            {
                failure = Some(error);
                cancel.cancel();
                core.cancel(stream.query_id).await;
            }
        }
        if let Some(error) = failure {
            return Err(error);
        }
        if status != QueryStatus::Completed {
            return Ok(status);
        }
    }
    Ok(QueryStatus::Completed)
}
