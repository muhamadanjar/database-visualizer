use db_types::{DbError, DbResult, DbValue, Engine, ObjectId, ObjectKind, SchemaChangeRequest, SchemaColumn, SchemaConstraint, SchemaConstraintKind, SchemaIndex, SchemaOperation};
use sqlparser::{ast::{GroupByExpr, SelectItem, SetExpr, Statement}, dialect::{MySqlDialect, PostgreSqlDialect}, parser::Parser};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SchemaPreview {
    pub sql: String,
    pub destructive: bool,
    pub affected_name: Option<String>,
}

pub fn require_confirmation(preview: &SchemaPreview, confirmation: Option<&db_types::DestructiveConfirmation>) -> DbResult<()> {
    if !preview.destructive { return Ok(()); }
    let Some(value) = confirmation else { return Err(DbError::new("DESTRUCTIVE_CONFIRMATION_REQUIRED", "schema", "Destructive schema changes require confirmation")); };
    if !value.confirmed || preview.affected_name.as_deref() != Some(value.typed_name.as_str()) { return Err(DbError::new("DESTRUCTIVE_CONFIRMATION_MISMATCH", "schema", "Type the exact affected object name and confirm the destructive change")); }
    Ok(())
}

pub fn preview(engine: Engine, request: &SchemaChangeRequest) -> DbResult<SchemaPreview> {
    let table = &request.table;
    if table.engine != engine || table.kind != ObjectKind::Table || table.name.is_empty() {
        return Err(invalid("A base table in the connected engine is required"));
    }
    let qtable = qualified(engine, table.schema.as_deref(), &table.name)?;
    let (sql, destructive, affected_name) = match &request.operation {
        SchemaOperation::CreateTable { columns } => { if columns.is_empty() { return Err(invalid("At least one table column is required")); } (format!("CREATE TABLE {qtable} ({})", columns.iter().map(|c| column(engine, c)).collect::<DbResult<Vec<_>>>()?.join(", ")), false, None) },
        SchemaOperation::RenameTable { new_name } => (format!("ALTER TABLE {qtable} RENAME TO {}", ident(engine, new_name)?), true, Some(table.name.clone())),
        SchemaOperation::DropTable => (format!("DROP TABLE {qtable}"), true, Some(table.name.clone())),
        SchemaOperation::AddColumn { column: c } => (format!("ALTER TABLE {qtable} ADD COLUMN {}", column(engine, c)?), false, None),
        SchemaOperation::AlterColumn { column: c } => alter_column(engine, &qtable, c)?,
        SchemaOperation::RenameColumn { name, new_name } => (format!("ALTER TABLE {qtable} RENAME COLUMN {} TO {}", ident(engine, name)?, ident(engine, new_name)?), true, Some(name.clone())),
        SchemaOperation::DropColumn { name } => (format!("ALTER TABLE {qtable} DROP COLUMN {}", ident(engine, name)?), true, Some(name.clone())),
        SchemaOperation::CreateIndex { index } => index_sql(engine, &qtable, index)?,
        SchemaOperation::DropIndex { name } => drop_index(engine, &qtable, table.schema.as_deref(), name)?,
        SchemaOperation::AddConstraint { constraint } => (format!("ALTER TABLE {qtable} ADD {}", constraint_sql(engine, constraint)?), false, None),
        SchemaOperation::DropConstraint { name, kind } => (drop_constraint(engine, &qtable, name, *kind)?, true, Some(if engine == Engine::MySql && *kind == SchemaConstraintKind::PrimaryKey { "PRIMARY".into() } else { name.clone() })),
    };
    Ok(SchemaPreview { sql: sql + ";", destructive, affected_name })
}

pub fn preview_for_database(engine: Engine, database: &str, request: &SchemaChangeRequest) -> DbResult<SchemaPreview> {
    if request.table.database != database {
        return Err(DbError::new("SCHEMA_SCOPE_MISMATCH", "schema", "Schema changes must target the connected database"));
    }
    preview(engine, request)
}

fn alter_column(engine: Engine, table: &str, c: &SchemaColumn) -> DbResult<(String, bool, Option<String>)> {
    let name = ident(engine, &c.name)?;
    let ty = data_type(engine, &c.data_type)?;
    let mut parts = if engine == Engine::MySql { vec![format!("MODIFY COLUMN {} {}{}{}", name, ty, if c.nullable { "" } else { " NOT NULL" }, c.default.as_ref().map(|v| format!(" DEFAULT {}", literal(v))).transpose()?.unwrap_or_default())] } else { vec![format!("ALTER COLUMN {name} TYPE {ty}")] };
    if engine == Engine::Postgres {
        if c.nullable { parts.push(format!("ALTER COLUMN {name} DROP NOT NULL")); } else { parts.push(format!("ALTER COLUMN {name} SET NOT NULL")); }
        if let Some(value) = &c.default { parts.push(format!("ALTER COLUMN {name} SET DEFAULT {}", literal(value)?)); } else { parts.push(format!("ALTER COLUMN {name} DROP DEFAULT")); }
    }
    Ok((format!("ALTER TABLE {table} {}", parts.join(", ")), true, Some(c.name.clone())))
}

fn column(engine: Engine, c: &SchemaColumn) -> DbResult<String> {
    let mut result = format!("{} {}", ident(engine, &c.name)?, data_type(engine, &c.data_type)?);
    if !c.nullable { result.push_str(" NOT NULL"); }
    if let Some(value) = &c.default { result.push_str(&format!(" DEFAULT {}", literal(value)?)); }
    Ok(result)
}
fn index_sql(engine: Engine, table: &str, i: &SchemaIndex) -> DbResult<(String, bool, Option<String>)> {
    names(engine, &i.columns)?;
    let columns = i.columns.iter().map(|n| ident(engine, n)).collect::<DbResult<Vec<_>>>()?.join(", ");
    Ok((format!("CREATE {}INDEX {} ON {table} ({columns})", if i.unique { "UNIQUE " } else { "" }, ident(engine, &i.name)?), false, None))
}
fn drop_index(engine: Engine, table: &str, schema: Option<&str>, name: &str) -> DbResult<(String, bool, Option<String>)> {
    let q = ident(engine, name)?;
    let sql = if engine == Engine::MySql {
        format!("ALTER TABLE {table} DROP INDEX {q}")
    } else {
        let qualified_index = qualified(engine, schema, name)?;
        format!("DROP INDEX {qualified_index}")
    };
    Ok((sql, true, Some(name.to_owned())))
}
fn drop_constraint(engine: Engine, table: &str, name: &str, kind: SchemaConstraintKind) -> DbResult<String> {
    let quoted = ident(engine, name)?;
    if engine == Engine::Postgres { return Ok(format!("ALTER TABLE {table} DROP CONSTRAINT {quoted}")); }
    Ok(match kind {
        SchemaConstraintKind::PrimaryKey => format!("ALTER TABLE {table} DROP PRIMARY KEY"),
        SchemaConstraintKind::Unique => format!("ALTER TABLE {table} DROP INDEX {quoted}"),
        SchemaConstraintKind::ForeignKey => format!("ALTER TABLE {table} DROP FOREIGN KEY {quoted}"),
        SchemaConstraintKind::Check => format!("ALTER TABLE {table} DROP CHECK {quoted}"),
    })
}
fn constraint_sql(engine: Engine, c: &SchemaConstraint) -> DbResult<String> {
    match c {
        SchemaConstraint::PrimaryKey { name, columns } => named_columns(engine, "PRIMARY KEY", name, columns),
        SchemaConstraint::Unique { name, columns } => named_columns(engine, "UNIQUE", name, columns),
        SchemaConstraint::ForeignKey { name, columns, referenced_table, referenced_columns } => {
            if referenced_table.engine != engine || referenced_table.kind != ObjectKind::Table || columns.len() != referenced_columns.len() || columns.is_empty() { return Err(invalid("Foreign-key table and columns must match the connected engine")); }
            Ok(format!("CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {} ({})", ident(engine, name)?, names(engine, columns)?, qualified(engine, referenced_table.schema.as_deref(), &referenced_table.name)?, names(engine, referenced_columns)?))
        }
        SchemaConstraint::Check { name, expression } => { fragment(engine, expression)?; Ok(format!("CONSTRAINT {} CHECK ({})", ident(engine, name)?, expression)) }
    }
}
fn named_columns(engine: Engine, kind: &str, name: &str, columns: &[String]) -> DbResult<String> { if columns.is_empty() { return Err(invalid("At least one constraint column is required")); } Ok(format!("CONSTRAINT {} {kind} ({})", ident(engine, name)?, names(engine, columns)?)) }
fn names(engine: Engine, values: &[String]) -> DbResult<String> { if values.is_empty() { return Err(invalid("At least one column is required")); } Ok(values.iter().map(|v| ident(engine, v)).collect::<DbResult<Vec<_>>>()?.join(", ")) }
fn qualified(engine: Engine, schema: Option<&str>, name: &str) -> DbResult<String> { let mut parts = Vec::new(); if let Some(s) = schema { parts.push(ident(engine, s)?); } parts.push(ident(engine, name)?); Ok(parts.join(".")) }
fn ident(engine: Engine, value: &str) -> DbResult<String> { if value.is_empty() || value.contains('\0') { return Err(invalid("Identifier cannot be empty")); } match engine { Engine::Postgres => Ok(format!("\"{}\"", value.replace('"', "\"\""))), Engine::MySql => Ok(format!("`{}`", value.replace('`', "``"))) } }
fn dialect(engine: Engine) -> &'static dyn sqlparser::dialect::Dialect { match engine { Engine::Postgres => &PostgreSqlDialect {}, Engine::MySql => &MySqlDialect {} } }
fn data_type(engine: Engine, value: &str) -> DbResult<String> {
    let value = value.trim();
    if value.is_empty() || value.contains('\0') { return Err(invalid("Unsupported data type")); }
    let sql = format!("CREATE TABLE __schema_change_type (__schema_change_column {value})");
    let statements = Parser::parse_sql(dialect(engine), &sql).map_err(|_| invalid("Unsupported data type"))?;
    match statements.as_slice() {
        [Statement::CreateTable(create)] if create.columns.len() == 1 && create.constraints.is_empty() && create.columns[0].options.is_empty() => Ok(value.to_owned()),
        _ => Err(invalid("Data type must parse as one column definition")),
    }
}
fn fragment(engine: Engine, value: &str) -> DbResult<()> {
    let value = value.trim();
    if value.is_empty() || value.contains('\0') { return Err(invalid("Unsupported constraint expression")); }
    let statements = Parser::parse_sql(dialect(engine), &format!("SELECT {value}")).map_err(|_| invalid("CHECK must be one SQL expression"))?;
    let valid = match statements.as_slice() {
        [Statement::Query(query)] if query.with.is_none() && query.order_by.is_none() && query.limit_clause.is_none() && query.fetch.is_none() && query.locks.is_empty() && query.for_clause.is_none() && query.settings.is_none() && query.pipe_operators.is_empty() => match query.body.as_ref() {
            SetExpr::Select(select) => select.projection.len() == 1 && matches!(select.projection[0], SelectItem::UnnamedExpr(_)) && select.from.is_empty() && select.lateral_views.is_empty() && select.prewhere.is_none() && select.selection.is_none() && select.connect_by.is_empty() && matches!(&select.group_by, GroupByExpr::Expressions(expressions, modifiers) if expressions.is_empty() && modifiers.is_empty()) && select.cluster_by.is_empty() && select.distribute_by.is_empty() && select.sort_by.is_empty() && select.having.is_none() && select.named_window.is_empty() && select.qualify.is_none(),
            _ => false,
        },
        _ => false,
    };
    if valid { Ok(()) } else { Err(invalid("CHECK must be one SQL expression")) }
}
fn literal(value: &DbValue) -> DbResult<String> { match value { DbValue::Null => Ok("NULL".into()), DbValue::Boolean(v) => Ok(if *v { "TRUE" } else { "FALSE" }.into()), DbValue::Signed(v) => Ok(v.to_string()), DbValue::Unsigned(v) => Ok(v.to_string()), DbValue::Float(v) | DbValue::Decimal(v) => { if v.parse::<f64>().is_ok() { Ok(v.clone()) } else { Err(invalid("Numeric default must be a number")) } }, DbValue::Text(v) | DbValue::Date(v) | DbValue::Time(v) | DbValue::Timestamp(v) => Ok(format!("'{}'", v.replace('\'', "''"))), _ => Err(invalid("Default must be a nullable or typed literal")) } }
fn invalid(message: &str) -> DbError { DbError::new("INVALID_SCHEMA_CHANGE", "schema", message) }

#[cfg(test)]
mod tests {
    use super::*;
    fn table(engine: Engine) -> ObjectId { ObjectId { engine, database: "db".into(), schema: Some("odd.schema".into()), name: "weird\"table".into(), kind: ObjectKind::Table } }
    #[test] fn quotes_identifiers_and_literals() { let r = SchemaChangeRequest { table: table(Engine::Postgres), operation: SchemaOperation::AddColumn { column: SchemaColumn { name: "x\"y".into(), data_type: "text".into(), nullable: true, default: Some(DbValue::Text("it's".into())) } }, confirmation: None }; assert_eq!(preview(Engine::Postgres, &r).unwrap().sql, "ALTER TABLE \"odd.schema\".\"weird\"\"table\" ADD COLUMN \"x\"\"y\" text DEFAULT 'it''s';"); }
    #[test] fn rejects_expression_defaults_and_multistatement_fragments() { let mut r = SchemaChangeRequest { table: table(Engine::MySql), operation: SchemaOperation::AddColumn { column: SchemaColumn { name: "x".into(), data_type: "text".into(), nullable: true, default: Some(DbValue::Opaque { database_type: "x".into(), format: "sql".into(), payload: vec![] }) } }, confirmation: None }; assert!(preview(Engine::MySql, &r).is_err()); r.operation = SchemaOperation::AddConstraint { constraint: SchemaConstraint::Check { name: "c".into(), expression: "1=1; DROP TABLE x".into() } }; assert!(preview(Engine::MySql, &r).is_err()); }
    #[test] fn maps_mysql_alter_and_drop_index_syntax() { let r = SchemaChangeRequest { table: table(Engine::MySql), operation: SchemaOperation::AlterColumn { column: SchemaColumn { name: "x".into(), data_type: "VARCHAR(10)".into(), nullable: false, default: None } }, confirmation: None }; let sql = preview(Engine::MySql, &r).unwrap().sql; assert!(sql.contains("MODIFY COLUMN")); assert!(!sql.contains("DEFAULT")); let r = SchemaChangeRequest { table: table(Engine::MySql), operation: SchemaOperation::DropIndex { name: "idx".into() }, confirmation: None }; assert!(preview(Engine::MySql, &r).unwrap().sql.starts_with("ALTER TABLE")); let r = SchemaChangeRequest { table: table(Engine::MySql), operation: SchemaOperation::DropConstraint { name: "fk".into(), kind: SchemaConstraintKind::ForeignKey }, confirmation: None }; assert!(preview(Engine::MySql, &r).unwrap().sql.contains("DROP FOREIGN KEY")); }
    #[test] fn postgres_alter_is_one_statement_and_confirms_column() { let r = SchemaChangeRequest { table: table(Engine::Postgres), operation: SchemaOperation::AlterColumn { column: SchemaColumn { name: "amount".into(), data_type: "INTEGER".into(), nullable: false, default: None } }, confirmation: None }; let p = preview(Engine::Postgres, &r).unwrap(); assert_eq!(p.sql.matches(';').count(), 1); assert_eq!(p.affected_name.as_deref(), Some("amount")); }
    #[test] fn alter_column_default_replacement_is_explicit_on_postgres() { let r = SchemaChangeRequest { table: table(Engine::Postgres), operation: SchemaOperation::AlterColumn { column: SchemaColumn { name: "amount".into(), data_type: "INTEGER".into(), nullable: false, default: None } }, confirmation: None }; assert!(preview(Engine::Postgres, &r).unwrap().sql.contains("DROP DEFAULT")); let r = SchemaChangeRequest { table: table(Engine::MySql), operation: SchemaOperation::AlterColumn { column: SchemaColumn { name: "amount".into(), data_type: "INTEGER".into(), nullable: false, default: Some(DbValue::Signed(7)) } }, confirmation: None }; assert!(preview(Engine::MySql, &r).unwrap().sql.contains("DEFAULT 7")); }
    #[test] fn destructive_confirmation_is_exact_and_fail_closed() { let r = SchemaChangeRequest { table: table(Engine::Postgres), operation: SchemaOperation::DropTable, confirmation: None }; let p = preview(Engine::Postgres, &r).unwrap(); assert_eq!(require_confirmation(&p, None).unwrap_err().code, "DESTRUCTIVE_CONFIRMATION_REQUIRED"); let wrong = db_types::DestructiveConfirmation { confirmed: true, typed_name: "other".into() }; assert_eq!(require_confirmation(&p, Some(&wrong)).unwrap_err().code, "DESTRUCTIVE_CONFIRMATION_MISMATCH"); let exact = db_types::DestructiveConfirmation { confirmed: true, typed_name: "weird\"table".into() }; assert!(require_confirmation(&p, Some(&exact)).is_ok()); }
    #[test] fn rejects_blank_data_type() { let r = SchemaChangeRequest { table: table(Engine::Postgres), operation: SchemaOperation::AddColumn { column: SchemaColumn { name: "x".into(), data_type: "   ".into(), nullable: true, default: None } }, confirmation: None }; assert!(preview(Engine::Postgres, &r).is_err()); }
    #[test] fn rejects_database_scope_mismatch_before_sql_generation() { let r = SchemaChangeRequest { table: table(Engine::Postgres), operation: SchemaOperation::DropTable, confirmation: None }; assert_eq!(super::preview_for_database(Engine::Postgres, "other", &r).unwrap_err().code, "SCHEMA_SCOPE_MISMATCH"); }
    #[test] fn postgres_drop_index_qualifies_selected_schema() { let r = SchemaChangeRequest { table: table(Engine::Postgres), operation: SchemaOperation::DropIndex { name: "idx_items".into() }, confirmation: None }; assert_eq!(preview(Engine::Postgres, &r).unwrap().sql, "DROP INDEX \"odd.schema\".\"idx_items\";"); }
    #[test] fn mysql_primary_key_drop_confirms_primary_name() { let r = SchemaChangeRequest { table: table(Engine::MySql), operation: SchemaOperation::DropConstraint { name: "ignored".into(), kind: SchemaConstraintKind::PrimaryKey }, confirmation: None }; let p = preview(Engine::MySql, &r).unwrap(); assert_eq!(p.affected_name.as_deref(), Some("PRIMARY")); assert!(p.sql.contains("DROP PRIMARY KEY")); }
    #[test] fn rejects_data_type_extra_column_injection() { let r = SchemaChangeRequest { table: table(Engine::Postgres), operation: SchemaOperation::AddColumn { column: SchemaColumn { name: "x".into(), data_type: "INTEGER, injected TEXT".into(), nullable: true, default: None } }, confirmation: None }; assert!(preview(Engine::Postgres, &r).is_err()); }
    #[test] fn check_accepts_one_expression_and_rejects_statement_injection() { let ok = SchemaChangeRequest { table: table(Engine::Postgres), operation: SchemaOperation::AddConstraint { constraint: SchemaConstraint::Check { name: "positive".into(), expression: "amount > 0".into() } }, confirmation: None }; assert!(preview(Engine::Postgres, &ok).is_ok()); let bad = SchemaChangeRequest { table: table(Engine::Postgres), operation: SchemaOperation::AddConstraint { constraint: SchemaConstraint::Check { name: "bad".into(), expression: "amount > 0); DROP TABLE users; --".into() } }, confirmation: None }; assert!(preview(Engine::Postgres, &bad).is_err()); }
}
