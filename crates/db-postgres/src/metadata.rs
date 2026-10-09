use super::map_error;
use db_types::*;
use sqlx::{PgConnection, Row};

pub async fn read(
    connection: &mut PgConnection,
    config: &ConnectionConfig,
    request: &MetadataRequest,
) -> DbResult<MetadataResponse> {
    let mut response = MetadataResponse::default();
    match request {
        MetadataRequest::Databases | MetadataRequest::Schemas => {
            let (sql, kind) = if matches!(request, MetadataRequest::Databases) {
                (
                    "SELECT datname AS name FROM pg_database WHERE datallowconn AND NOT datistemplate ORDER BY datname",
                    ObjectKind::Database,
                )
            } else {
                (
                    "SELECT schema_name AS name FROM information_schema.schemata ORDER BY schema_name",
                    ObjectKind::Schema,
                )
            };
            for row in sqlx::query(sql)
                .fetch_all(&mut *connection)
                .await
                .map_err(map_error)?
            {
                let name: String = row.try_get("name").map_err(map_error)?;
                response.objects.push(ObjectId {
                    engine: config.engine,
                    database: if kind == ObjectKind::Database {
                        name.clone()
                    } else {
                        config.database.clone()
                    },
                    schema: if kind == ObjectKind::Schema {
                        Some(name.clone())
                    } else {
                        None
                    },
                    name,
                    kind,
                });
            }
        }
        MetadataRequest::Objects { schema, kind } => {
            let kinds = match kind {
                ObjectKind::Table => vec!["r", "p", "f"],
                ObjectKind::View => vec!["v", "m"],
                ObjectKind::Sequence => vec!["S"],
                _ => {
                    return Err(DbError::new(
                        "UNSUPPORTED_CAPABILITY",
                        "metadata",
                        "Unsupported object collection",
                    ));
                }
            };
            let rows=sqlx::query("SELECT c.relname AS name,n.nspname AS schema FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE c.relkind::text=ANY($1) AND ($2::text IS NULL OR n.nspname=$2) AND n.nspname NOT LIKE 'pg_%' AND n.nspname <> 'information_schema' ORDER BY n.nspname,c.relname").bind(kinds).bind(schema).fetch_all(&mut *connection).await.map_err(map_error)?;
            for row in rows {
                response.objects.push(ObjectId {
                    engine: config.engine,
                    database: config.database.clone(),
                    schema: Some(row.try_get("schema").map_err(map_error)?),
                    name: row.try_get("name").map_err(map_error)?,
                    kind: *kind,
                });
            }
        }
        MetadataRequest::Inspect { object } => {
            let schema = object.schema.as_deref().ok_or_else(|| {
                DbError::new(
                    "MISSING_SCHEMA",
                    "metadata",
                    "PostgreSQL object identifiers require a schema",
                )
            })?;
            let columns=sqlx::query("SELECT cols.column_name,pg_catalog.format_type(a.atttypid,a.atttypmod) AS data_type,cols.is_nullable,cols.column_default,cols.ordinal_position::bigint AS ordinal FROM information_schema.columns cols JOIN pg_namespace n ON n.nspname=cols.table_schema JOIN pg_class c ON c.relnamespace=n.oid AND c.relname=cols.table_name JOIN pg_attribute a ON a.attrelid=c.oid AND a.attname=cols.column_name AND NOT a.attisdropped WHERE cols.table_catalog=$1 AND cols.table_schema=$2 AND cols.table_name=$3 ORDER BY cols.ordinal_position").bind(&object.database).bind(schema).bind(&object.name).fetch_all(&mut *connection).await.map_err(map_error)?;
            for row in columns {
                response.columns.push(ColumnMetadata {
                    name: row.try_get("column_name").map_err(map_error)?,
                    data_type: row.try_get("data_type").map_err(map_error)?,
                    nullable: row.try_get::<String, _>("is_nullable").map_err(map_error)? == "YES",
                    default: row.try_get("column_default").map_err(map_error)?,
                    ordinal: row.try_get("ordinal").map_err(map_error)?,
                });
            }
            let indexes=sqlx::query("SELECT i.relname AS name,pg_get_indexdef(x.indexrelid) AS definition,x.indisunique AS is_unique FROM pg_index x JOIN pg_class t ON t.oid=x.indrelid JOIN pg_namespace n ON n.oid=t.relnamespace JOIN pg_class i ON i.oid=x.indexrelid WHERE n.nspname=$1 AND t.relname=$2 ORDER BY i.relname").bind(schema).bind(&object.name).fetch_all(&mut *connection).await.map_err(map_error)?;
            for row in indexes {
                response.indexes.push(IndexMetadata {
                    name: row.try_get("name").map_err(map_error)?,
                    definition: row.try_get("definition").map_err(map_error)?,
                    unique: row.try_get("is_unique").map_err(map_error)?,
                });
            }
            let constraints=sqlx::query("SELECT c.conname AS name,c.contype::text AS kind,pg_get_constraintdef(c.oid,true) AS definition FROM pg_constraint c JOIN pg_class t ON t.oid=c.conrelid JOIN pg_namespace n ON n.oid=t.relnamespace WHERE n.nspname=$1 AND t.relname=$2 ORDER BY c.conname").bind(schema).bind(&object.name).fetch_all(&mut *connection).await.map_err(map_error)?;
            for row in constraints {
                let code: String = row.try_get("kind").map_err(map_error)?;
                response.constraints.push(ConstraintMetadata {
                    name: row.try_get("name").map_err(map_error)?,
                    kind: match code.as_str() {
                        "p" => "PRIMARY KEY",
                        "u" => "UNIQUE",
                        "f" => "FOREIGN KEY",
                        "c" => "CHECK",
                        _ => code.as_str(),
                    }
                    .into(),
                    definition: row.try_get("definition").map_err(map_error)?,
                });
            }
        }
    }
    Ok(response)
}
