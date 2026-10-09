//! Session coordination and bounded query streams, independent of concrete drivers.
mod runtime;
use async_trait::async_trait;
use db_types::*;
pub use runtime::*;
use sqlparser::{
    ast::Statement,
    dialect::{MySqlDialect, PostgreSqlDialect},
    parser::Parser,
    tokenizer::{Token, Tokenizer, Whitespace},
};

// async-trait adds must_use to an already must-use boxed Future.
#[allow(clippy::double_must_use)]
#[async_trait]
pub trait ResultSink: Send {
    async fn columns(&mut self, columns: Vec<Column>) -> DbResult<()>;
    async fn row(&mut self, row: ResultRow) -> DbResult<()>;
}
// async-trait adds must_use to an already must-use boxed Future.
#[allow(clippy::double_must_use)]
#[async_trait]
pub trait BackendSession: Send {
    fn backend_id(&self) -> u64;
    async fn execute(&mut self, sql: &str, sink: &mut dyn ResultSink)
    -> DbResult<ExecutionSummary>;
    async fn synchronize(&mut self) -> DbResult<()>;
    async fn close(&mut self) -> DbResult<()>;
}
// async-trait adds must_use to an already must-use boxed Future.
#[allow(clippy::double_must_use)]
#[async_trait]
pub trait DatabaseAdapter: Send + Sync {
    fn engine(&self) -> Engine;
    fn capabilities(&self) -> Capabilities {
        Capabilities::for_engine(self.engine())
    }
    async fn connect(&self, config: &ConnectionConfig) -> DbResult<Box<dyn BackendSession>>;
    async fn metadata(
        &self,
        config: &ConnectionConfig,
        request: &MetadataRequest,
    ) -> DbResult<MetadataResponse>;
    async fn cancel(&self, config: &ConnectionConfig, backend_id: u64) -> DbResult<()>;
}

pub fn validate_sql(engine: Engine, sql: &str) -> DbResult<()> {
    let dialect: &dyn sqlparser::dialect::Dialect = match engine {
        Engine::Postgres => &PostgreSqlDialect {},
        Engine::MySql => &MySqlDialect {},
    };
    // MySQL executable comments are code on the server but whitespace to the
    // parser. Reject them lexically; comment-shaped text inside literals is fine.
    if engine == Engine::MySql {
        let tokens = Tokenizer::new(dialect, sql)
            .tokenize()
            .map_err(|_| unsupported())?;
        if tokens.iter().any(|t|matches!(t,Token::Whitespace(Whitespace::MultiLineComment(body)) if body.starts_with('!') || body.starts_with("M!"))) {return Err(unsupported());}
    }
    let statements = Parser::parse_sql(dialect, sql).map_err(|_| unsupported())?;
    if statements.len() != 1
        || matches!(
            statements[0],
            Statement::Call(_)
                | Statement::Execute { .. }
                | Statement::Prepare { .. }
                | Statement::Copy { .. }
        )
    {
        return Err(unsupported());
    }
    Ok(())
}
pub fn is_query(engine: Engine, sql: &str) -> bool {
    let dialect: &dyn sqlparser::dialect::Dialect = match engine {
        Engine::Postgres => &PostgreSqlDialect {},
        Engine::MySql => &MySqlDialect {},
    };
    Parser::parse_sql(dialect, sql)
        .is_ok_and(|s| s.len() == 1 && matches!(s[0], Statement::Query(_)))
}
fn unsupported() -> DbError {
    DbError::new(
        "UNSUPPORTED_STATEMENT",
        "syntax",
        "Foundation requires one supported statement and does not execute CALL, dynamic SQL, COPY, or executable comments",
    )
}

pub fn validate_metadata(config: &ConnectionConfig, request: &MetadataRequest) -> DbResult<()> {
    match request {
        MetadataRequest::Schemas if config.engine == Engine::MySql => {
            return Err(DbError::new(
                "UNSUPPORTED_CAPABILITY",
                "metadata",
                "MySQL uses database namespaces",
            ));
        }
        MetadataRequest::Objects {
            kind: ObjectKind::Sequence,
            ..
        } if config.engine == Engine::MySql => {
            return Err(DbError::new(
                "UNSUPPORTED_CAPABILITY",
                "metadata",
                "MySQL has no sequence capability",
            ));
        }
        MetadataRequest::Inspect { object }
            if object.engine != config.engine || object.database != config.database =>
        {
            return Err(DbError::new(
                "METADATA_SCOPE_MISMATCH",
                "metadata",
                "Inspect requires an object in the connected engine and database",
            ));
        }
        _ => {}
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn statement_validation_never_splits_literals() {
        for engine in [Engine::Postgres, Engine::MySql] {
            assert!(validate_sql(engine, "SELECT ';'").is_ok());
            assert!(validate_sql(engine, "SELECT 1; SELECT 2").is_err());
            assert!(validate_sql(engine, "CALL change_data()").is_err());
            assert!(validate_sql(engine, "").is_err());
        }
        assert!(validate_sql(Engine::MySql, "SELECT 1; /*!50000 CALL change_data() */").is_err());
        assert!(validate_sql(Engine::MySql, "SELECT '/*!50000 CALL change_data() */'").is_ok());
        assert!(validate_sql(Engine::MySql, "EXECUTE hidden_call").is_err());
        assert!(validate_sql(Engine::Postgres, "DO $$ BEGIN PERFORM 1; END $$").is_err());
    }
}
