use super::map_error;
use db_types::*;
use sqlx::{Column as SqlColumn, MySqlConnection, Row};
// MySQL 8.4 exposes catalog identifiers as VARBINARY. Decode their UTF-8
// losslessly without claiming their SQL type is VARCHAR; decoding still checks UTF-8.

pub async fn read(
    connection: &mut MySqlConnection,
    config: &ConnectionConfig,
    request: &MetadataRequest,
) -> DbResult<MetadataResponse> {
    let mut response = MetadataResponse::default();
    match request {
        MetadataRequest::Databases => {
            for row in sqlx::query(
                "SELECT SCHEMA_NAME AS name FROM information_schema.SCHEMATA ORDER BY SCHEMA_NAME",
            )
            .fetch_all(&mut *connection)
            .await
            .map_err(map_error)?
            {
                let name: String = row.try_get_unchecked("name").map_err(map_error)?;
                response.objects.push(ObjectId {
                    engine: config.engine,
                    database: name.clone(),
                    schema: None,
                    name,
                    kind: ObjectKind::Database,
                });
            }
        }
        MetadataRequest::Objects { schema, kind } => {
            if schema.is_some() {
                return Err(DbError::new(
                    "METADATA_SCOPE_MISMATCH",
                    "metadata",
                    "MySQL identifiers use database, not schema",
                ));
            }
            if matches!(
                kind,
                ObjectKind::Function | ObjectKind::Procedure | ObjectKind::Trigger
            ) {
                let (sql, filter) = if *kind == ObjectKind::Trigger {
                    (
                        "SELECT TRIGGER_NAME AS name FROM information_schema.TRIGGERS WHERE TRIGGER_SCHEMA=? ORDER BY TRIGGER_NAME",
                        None,
                    )
                } else {
                    (
                        "SELECT ROUTINE_NAME AS name FROM information_schema.ROUTINES WHERE ROUTINE_SCHEMA=? AND ROUTINE_TYPE=? ORDER BY ROUTINE_NAME",
                        Some(if *kind == ObjectKind::Function {
                            "FUNCTION"
                        } else {
                            "PROCEDURE"
                        }),
                    )
                };
                let mut query = sqlx::query(sql).bind(&config.database);
                if let Some(filter) = filter {
                    query = query.bind(filter);
                }
                for row in query.fetch_all(&mut *connection).await.map_err(map_error)? {
                    response.objects.push(ObjectId {
                        engine: config.engine,
                        database: config.database.clone(),
                        schema: None,
                        name: row.try_get_unchecked("name").map_err(map_error)?,
                        kind: *kind,
                    });
                }
                return Ok(response);
            }
            let table_type = match kind {
                ObjectKind::Table => "BASE TABLE",
                ObjectKind::View => "VIEW",
                _ => {
                    return Err(DbError::new(
                        "UNSUPPORTED_CAPABILITY",
                        "metadata",
                        "Unsupported object collection",
                    ));
                }
            };
            let rows=sqlx::query("SELECT TABLE_NAME AS name FROM information_schema.TABLES WHERE TABLE_SCHEMA=? AND TABLE_TYPE=? ORDER BY TABLE_NAME").bind(&config.database).bind(table_type).fetch_all(&mut *connection).await.map_err(map_error)?;
            for row in rows {
                response.objects.push(ObjectId {
                    engine: config.engine,
                    database: config.database.clone(),
                    schema: None,
                    name: row.try_get_unchecked("name").map_err(map_error)?,
                    kind: *kind,
                });
            }
        }
        MetadataRequest::Inspect { object } => {
            if object.schema.is_some() {
                return Err(DbError::new(
                    "METADATA_SCOPE_MISMATCH",
                    "metadata",
                    "MySQL identifiers use database, not schema",
                ));
            }
            let kind = match object.kind {
                ObjectKind::Table => "TABLE",
                ObjectKind::View => "VIEW",
                ObjectKind::Function => "FUNCTION",
                ObjectKind::Procedure => "PROCEDURE",
                ObjectKind::Trigger => "TRIGGER",
                _ => {
                    return Err(DbError::new(
                        "UNSUPPORTED_CAPABILITY",
                        "metadata",
                        "Object definition is not supported",
                    ));
                }
            };
            let quote = |name: &str| format!("`{}`", name.replace('`', "``"));
            let ddl_sql = format!(
                "SHOW CREATE {kind} {}.{}",
                quote(&object.database),
                quote(&object.name)
            );
            let ddl_row = sqlx::query(&ddl_sql)
                .fetch_one(&mut *connection)
                .await
                .map_err(map_error)?;
            let ddl_column = if kind == "TRIGGER" {
                "SQL Original Statement"
            } else {
                match kind {
                    "TABLE" => "Create Table",
                    "VIEW" => "Create View",
                    "FUNCTION" => "Create Function",
                    _ => "Create Procedure",
                }
            };
            // SHOW CREATE prepared responses can expose column names without a
            // matching driver name-map entry. Resolve the actual column ordinal.
            let ddl_index = ddl_row
                .columns()
                .iter()
                .position(|c| c.name() == ddl_column)
                .ok_or_else(|| {
                    DbError::new(
                        "DDL_UNAVAILABLE",
                        "metadata",
                        "Object definition is not available",
                    )
                })?;
            response.ddl = ddl_row
                .try_get_unchecked::<Option<String>, _>(ddl_index)
                .map_err(map_error)?;
            if matches!(
                object.kind,
                ObjectKind::Function | ObjectKind::Procedure | ObjectKind::Trigger
            ) {
                return Ok(response);
            }
            let columns=sqlx::query("SELECT COLUMN_NAME,COLUMN_TYPE,IS_NULLABLE,COLUMN_DEFAULT,CAST(ORDINAL_POSITION AS SIGNED) AS ordinal FROM information_schema.COLUMNS WHERE TABLE_SCHEMA=? AND TABLE_NAME=? ORDER BY ORDINAL_POSITION").bind(&object.database).bind(&object.name).fetch_all(&mut *connection).await.map_err(map_error)?;
            for row in columns {
                response.columns.push(ColumnMetadata {
                    name: row.try_get_unchecked("COLUMN_NAME").map_err(map_error)?,
                    data_type: row.try_get_unchecked("COLUMN_TYPE").map_err(map_error)?,
                    nullable: row
                        .try_get_unchecked::<String, _>("IS_NULLABLE")
                        .map_err(map_error)?
                        == "YES",
                    default: row.try_get_unchecked("COLUMN_DEFAULT").map_err(map_error)?,
                    ordinal: row.try_get("ordinal").map_err(map_error)?,
                });
            }
            if object.kind == ObjectKind::Table {
                let keys=sqlx::query_scalar::<_,String>("SELECT COLUMN_NAME FROM information_schema.KEY_COLUMN_USAGE WHERE TABLE_SCHEMA=? AND TABLE_NAME=? AND CONSTRAINT_NAME='PRIMARY' ORDER BY ORDINAL_POSITION").bind(&object.database).bind(&object.name).fetch_all(&mut *connection).await.map_err(map_error)?;
                let generated=sqlx::query_scalar::<_,String>("SELECT COLUMN_NAME FROM information_schema.COLUMNS WHERE TABLE_SCHEMA=? AND TABLE_NAME=? AND (EXTRA LIKE '%auto_increment%' OR EXTRA LIKE '%GENERATED%') ORDER BY ORDINAL_POSITION").bind(&object.database).bind(&object.name).fetch_all(&mut *connection).await.map_err(map_error)?;
                let storage=sqlx::query_scalar::<_,String>("SELECT COALESCE(ENGINE,'') FROM information_schema.TABLES WHERE TABLE_SCHEMA=? AND TABLE_NAME=?").bind(&object.database).bind(&object.name).fetch_one(&mut *connection).await.map_err(map_error)?;
                // Fail closed when a trigger can write to storage outside the verified base table.
                let triggers:i64=sqlx::query_scalar("SELECT COUNT(*) FROM information_schema.TRIGGERS WHERE EVENT_OBJECT_SCHEMA=? AND EVENT_OBJECT_TABLE=?").bind(&object.database).bind(&object.name).fetch_one(&mut *connection).await.map_err(map_error)?;
                response.editing = Some(EditingMetadata {
                    primary_key: keys,
                    generated,
                    transactional: storage.eq_ignore_ascii_case("InnoDB") && triggers == 0,
                    warnings: vec![
                        if triggers > 0 {
                            "Trigger dependency storage is unverified; editing is disabled"
                        } else {
                            "Constraints and FK cascades apply"
                        }
                        .into(),
                    ],
                });
            }
            let indexes=sqlx::query("SELECT INDEX_NAME AS name,GROUP_CONCAT(COALESCE(COLUMN_NAME,EXPRESSION) ORDER BY SEQ_IN_INDEX SEPARATOR ', ') AS definition,CAST(MIN(NON_UNIQUE)=0 AS UNSIGNED) AS is_unique FROM information_schema.STATISTICS WHERE TABLE_SCHEMA=? AND TABLE_NAME=? GROUP BY INDEX_NAME ORDER BY INDEX_NAME").bind(&object.database).bind(&object.name).fetch_all(&mut *connection).await.map_err(map_error)?;
            for row in indexes {
                response.indexes.push(IndexMetadata {
                    name: row.try_get_unchecked("name").map_err(map_error)?,
                    definition: row.try_get_unchecked("definition").map_err(map_error)?,
                    unique: row.try_get::<u64, _>("is_unique").map_err(map_error)? != 0,
                });
            }
            let constraints=sqlx::query("SELECT t.CONSTRAINT_NAME AS name,t.CONSTRAINT_TYPE AS kind,CASE WHEN t.CONSTRAINT_TYPE='CHECK' THEN CONCAT('CHECK (',MAX(cc.CHECK_CLAUSE),')') ELSE CONCAT(t.CONSTRAINT_TYPE,' (',COALESCE(GROUP_CONCAT(k.COLUMN_NAME ORDER BY k.ORDINAL_POSITION SEPARATOR ', '),''),')',COALESCE(CONCAT(' REFERENCES ',MAX(k.REFERENCED_TABLE_SCHEMA),'.',MAX(k.REFERENCED_TABLE_NAME),' (',GROUP_CONCAT(k.REFERENCED_COLUMN_NAME ORDER BY k.ORDINAL_POSITION SEPARATOR ', '),') ON UPDATE ',MAX(rc.UPDATE_RULE),' ON DELETE ',MAX(rc.DELETE_RULE)),'')) END AS definition FROM information_schema.TABLE_CONSTRAINTS t LEFT JOIN information_schema.KEY_COLUMN_USAGE k ON k.CONSTRAINT_SCHEMA=t.CONSTRAINT_SCHEMA AND k.TABLE_NAME=t.TABLE_NAME AND k.CONSTRAINT_NAME=t.CONSTRAINT_NAME LEFT JOIN information_schema.CHECK_CONSTRAINTS cc ON cc.CONSTRAINT_SCHEMA=t.CONSTRAINT_SCHEMA AND cc.CONSTRAINT_NAME=t.CONSTRAINT_NAME LEFT JOIN information_schema.REFERENTIAL_CONSTRAINTS rc ON rc.CONSTRAINT_SCHEMA=t.CONSTRAINT_SCHEMA AND rc.CONSTRAINT_NAME=t.CONSTRAINT_NAME AND rc.TABLE_NAME=t.TABLE_NAME WHERE t.TABLE_SCHEMA=? AND t.TABLE_NAME=? GROUP BY t.CONSTRAINT_NAME,t.CONSTRAINT_TYPE ORDER BY t.CONSTRAINT_NAME").bind(&object.database).bind(&object.name).fetch_all(&mut *connection).await.map_err(map_error)?;
            for row in constraints {
                response.constraints.push(ConstraintMetadata {
                    name: row.try_get_unchecked("name").map_err(map_error)?,
                    kind: row.try_get_unchecked("kind").map_err(map_error)?,
                    definition: row.try_get_unchecked("definition").map_err(map_error)?,
                });
            }
        }
        _ => {
            return Err(DbError::new(
                "UNSUPPORTED_CAPABILITY",
                "metadata",
                "MySQL uses database namespaces and has no sequences",
            ));
        }
    }
    Ok(response)
}
