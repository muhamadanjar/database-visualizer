use crate::validate_sql;
use db_types::*;
use sqlparser::{
    dialect::{Dialect, MySqlDialect, PostgreSqlDialect},
    tokenizer::{Token, Tokenizer},
};

pub fn is_transaction_control(engine: Engine, sql: &str) -> bool {
    use sqlparser::{ast::Statement, parser::Parser};
    let dialect: &dyn Dialect = match engine {
        Engine::Postgres => &PostgreSqlDialect {},
        Engine::MySql => &MySqlDialect {},
    };
    Parser::parse_sql(dialect, sql).is_ok_and(|statements| {
        matches!(
            statements.first(),
            Some(
                Statement::StartTransaction { .. }
                    | Statement::Commit { .. }
                    | Statement::Rollback { .. }
            )
        )
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptStatement {
    pub sql: String,
    pub start: usize,
    pub end: usize,
}

/// Preflight the entire script and preserve source text. No statement executes here.
pub fn prepare_script(engine: Engine, sql: &str) -> DbResult<Vec<ScriptStatement>> {
    if sql.len() > 1024 * 1024 {
        return Err(DbError::new(
            "SCRIPT_TOO_LARGE",
            "capacity",
            "SQL is limited to 1 MiB",
        ));
    }
    let dialect: &dyn Dialect = match engine {
        Engine::Postgres => &PostgreSqlDialect {},
        Engine::MySql => &MySqlDialect {},
    };
    let tokens = Tokenizer::new(dialect, sql)
        .tokenize_with_location()
        .map_err(|_| DbError::new("UNSUPPORTED_STATEMENT", "syntax", "SQL could not be parsed"))?;
    let offsets: Vec<usize> = std::iter::once(0)
        .chain(sql.match_indices('\n').map(|(i, _)| i + 1))
        .collect();
    let mut boundaries = Vec::new();
    for token in &tokens {
        if token.token == Token::SemiColon {
            let line = token.span.start.line.saturating_sub(1) as usize;
            let start = *offsets.get(line).ok_or_else(DbError::unknown)?;
            let column = token.span.start.column.saturating_sub(1) as usize;
            let offset = sql[start..]
                .char_indices()
                .nth(column)
                .map(|(i, _)| start + i)
                .ok_or_else(DbError::unknown)?;
            boundaries.push(offset + 1);
        }
    }
    boundaries.push(sql.len());
    let mut result = Vec::new();
    let mut start = 0;
    for end in boundaries {
        let text = &sql[start..end];
        let part = Tokenizer::new(dialect, text)
            .tokenize()
            .map_err(|_| DbError::unknown())?;
        if part
            .iter()
            .any(|t| !matches!(t, Token::Whitespace(_) | Token::SemiColon | Token::EOF))
        {
            validate_sql(engine, text)?;
            result.push(ScriptStatement {
                sql: text.into(),
                start,
                end,
            });
        }
        start = end;
    }
    if result.is_empty() || result.len() > 1000 {
        return Err(DbError::new(
            "INVALID_SCRIPT",
            "syntax",
            "Provide between 1 and 1000 supported statements",
        ));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scripts_preserve_literals_comments_and_source() {
        let sql = "-- ü;\nSELECT '世界;'; /* ; */ SELECT $$x;y$$;";
        let statements = prepare_script(Engine::Postgres, sql).unwrap();
        assert_eq!(statements.len(), 2);
        for statement in statements {
            assert_eq!(statement.sql, sql[statement.start..statement.end]);
        }
        assert_eq!(
            prepare_script(Engine::MySql, "SELECT ';'; SELECT 2;")
                .unwrap()
                .len(),
            2
        );
    }
    #[test]
    fn preflight_rejects_entire_script_before_execution() {
        assert!(prepare_script(Engine::Postgres, "SELECT 1; CALL hidden();").is_err());
        assert!(prepare_script(Engine::MySql, "SELECT 1; /*!50000 CALL hidden() */").is_err());
        assert!(prepare_script(Engine::Postgres, "-- comment only").is_err());
    }
}
