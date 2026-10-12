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
            if matches!(
                kind,
                ObjectKind::Function
                    | ObjectKind::Procedure
                    | ObjectKind::Trigger
                    | ObjectKind::CustomType
            ) {
                let sql = match kind {
                    ObjectKind::Function => {
                        "SELECT format('%I(%s)',p.proname,pg_get_function_identity_arguments(p.oid)) AS name,n.nspname AS schema FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace WHERE p.prokind<>'p' AND p.prokind<>'a' AND ($1::text IS NULL OR n.nspname=$1) AND n.nspname NOT LIKE 'pg_%' AND n.nspname<>'information_schema' ORDER BY 2,1"
                    }
                    ObjectKind::Procedure => {
                        "SELECT format('%I(%s)',p.proname,pg_get_function_identity_arguments(p.oid)) AS name,n.nspname AS schema FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace WHERE p.prokind='p' AND ($1::text IS NULL OR n.nspname=$1) AND n.nspname NOT LIKE 'pg_%' AND n.nspname<>'information_schema' ORDER BY 2,1"
                    }
                    ObjectKind::Trigger => {
                        "SELECT format('%I ON %I',t.tgname,c.relname) AS name,n.nspname AS schema FROM pg_trigger t JOIN pg_class c ON c.oid=t.tgrelid JOIN pg_namespace n ON n.oid=c.relnamespace WHERE NOT t.tgisinternal AND ($1::text IS NULL OR n.nspname=$1) AND n.nspname NOT LIKE 'pg_%' ORDER BY 2,1"
                    }
                    _ => {
                        "SELECT t.typname AS name,n.nspname AS schema FROM pg_type t JOIN pg_namespace n ON n.oid=t.typnamespace WHERE t.typtype IN ('e','d','c','r','m') AND (t.typrelid=0 OR EXISTS(SELECT 1 FROM pg_class c WHERE c.oid=t.typrelid AND c.relkind='c')) AND ($1::text IS NULL OR n.nspname=$1) AND n.nspname NOT LIKE 'pg_%' AND n.nspname<>'information_schema' ORDER BY 2,1"
                    }
                };
                let rows = sqlx::query(sql)
                    .bind(schema)
                    .fetch_all(&mut *connection)
                    .await
                    .map_err(map_error)?;
                for row in rows {
                    response.objects.push(ObjectId {
                        engine: config.engine,
                        database: config.database.clone(),
                        schema: Some(row.try_get("schema").map_err(map_error)?),
                        name: row.try_get("name").map_err(map_error)?,
                        kind: *kind,
                    });
                }
                return Ok(response);
            }
            let kinds = match kind {
                ObjectKind::Table => vec!["r", "p", "f"],
                ObjectKind::View => vec!["v"],
                ObjectKind::MaterializedView => vec!["m"],
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
            if matches!(object.kind, ObjectKind::Function | ObjectKind::Procedure) {
                response.ddl=sqlx::query_scalar("SELECT pg_get_functiondef(p.oid) FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace WHERE n.nspname=$1 AND format('%I(%s)',p.proname,pg_get_function_identity_arguments(p.oid))=$2 AND p.prokind<>'a'").bind(schema).bind(&object.name).fetch_optional(&mut *connection).await.map_err(map_error)?;
                return Ok(response);
            }
            if object.kind == ObjectKind::Trigger {
                response.ddl=sqlx::query_scalar("SELECT pg_get_triggerdef(t.oid,true) FROM pg_trigger t JOIN pg_class c ON c.oid=t.tgrelid JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 AND format('%I ON %I',t.tgname,c.relname)=$2 AND NOT t.tgisinternal").bind(schema).bind(&object.name).fetch_optional(&mut *connection).await.map_err(map_error)?;
                return Ok(response);
            }
            if object.kind == ObjectKind::CustomType {
                response.ddl=sqlx::query_scalar(r#"SELECT CASE
WHEN t.typtype='e' THEN format('CREATE TYPE %I.%I AS ENUM (%s);',n.nspname,t.typname,(SELECT string_agg(quote_literal(e.enumlabel),', ' ORDER BY e.enumsortorder) FROM pg_enum e WHERE e.enumtypid=t.oid))
WHEN t.typtype='d' THEN format('CREATE DOMAIN %I.%I AS %s%s%s%s;',n.nspname,t.typname,format_type(t.typbasetype,t.typtypmod),COALESCE(' DEFAULT '||t.typdefault,''),CASE WHEN t.typnotnull THEN ' NOT NULL' ELSE '' END,COALESCE((SELECT ' '||string_agg(format('CONSTRAINT %I %s',c.conname,pg_get_constraintdef(c.oid,true)),' ') FROM pg_constraint c WHERE c.contypid=t.oid),''))
WHEN t.typtype='c' THEN format('CREATE TYPE %I.%I AS (%s);',n.nspname,t.typname,(SELECT string_agg(format('%I %s',a.attname,format_type(a.atttypid,a.atttypmod)),', ' ORDER BY a.attnum) FROM pg_attribute a WHERE a.attrelid=t.typrelid AND a.attnum>0 AND NOT a.attisdropped))
WHEN t.typtype='r' THEN (SELECT format('CREATE TYPE %I.%I AS RANGE (SUBTYPE = %s, SUBTYPE_OPCLASS = %s%s%s%s, MULTIRANGE_TYPE_NAME = %I.%I);',n.nspname,t.typname,format_type(r.rngsubtype,NULL),(SELECT format('%I.%I',ons.nspname,op.opcname) FROM pg_opclass op JOIN pg_namespace ons ON ons.oid=op.opcnamespace WHERE op.oid=r.rngsubopc),CASE WHEN r.rngcollation<>0 THEN ', COLLATION = '||r.rngcollation::regcollation::text ELSE '' END,CASE WHEN r.rngcanonical<>0 THEN ', CANONICAL = '||r.rngcanonical::regproc::text ELSE '' END,CASE WHEN r.rngsubdiff<>0 THEN ', SUBTYPE_DIFF = '||r.rngsubdiff::regproc::text ELSE '' END,n.nspname,(SELECT typname FROM pg_type WHERE oid=r.rngmultitypid)) FROM pg_range r WHERE r.rngtypid=t.oid)
WHEN t.typtype='m' THEN (SELECT format('-- Multirange %I.%I is created by range type %s.',n.nspname,t.typname,format_type(r.rngtypid,NULL)) FROM pg_range r WHERE r.rngmultitypid=t.oid)
END FROM pg_type t JOIN pg_namespace n ON n.oid=t.typnamespace WHERE n.nspname=$1 AND t.typname=$2"#).bind(schema).bind(&object.name).fetch_optional(&mut *connection).await.map_err(map_error)?;
                return Ok(response);
            }
            let columns=sqlx::query("SELECT a.attname AS column_name,pg_catalog.format_type(a.atttypid,a.atttypmod) AS data_type,CASE WHEN a.attnotnull THEN 'NO' ELSE 'YES' END AS is_nullable,pg_get_expr(d.adbin,d.adrelid) AS column_default,a.attnum::bigint AS ordinal FROM pg_attribute a JOIN pg_class c ON c.oid=a.attrelid JOIN pg_namespace n ON n.oid=c.relnamespace LEFT JOIN pg_attrdef d ON d.adrelid=a.attrelid AND d.adnum=a.attnum WHERE n.nspname=$1 AND c.relname=$2 AND a.attnum>0 AND NOT a.attisdropped ORDER BY a.attnum").bind(schema).bind(&object.name).fetch_all(&mut *connection).await.map_err(map_error)?;
            for row in columns {
                response.columns.push(ColumnMetadata {
                    name: row.try_get("column_name").map_err(map_error)?,
                    data_type: row.try_get("data_type").map_err(map_error)?,
                    nullable: row.try_get::<String, _>("is_nullable").map_err(map_error)? == "YES",
                    default: row.try_get("column_default").map_err(map_error)?,
                    ordinal: row.try_get("ordinal").map_err(map_error)?,
                });
            }
            if object.kind == ObjectKind::Table {
                let keys=sqlx::query_scalar::<_,String>("SELECT a.attname FROM pg_constraint k JOIN pg_class c ON c.oid=k.conrelid JOIN pg_namespace n ON n.oid=c.relnamespace CROSS JOIN LATERAL unnest(k.conkey) WITH ORDINALITY x(attnum,ord) JOIN pg_attribute a ON a.attrelid=c.oid AND a.attnum=x.attnum WHERE n.nspname=$1 AND c.relname=$2 AND k.contype='p' ORDER BY x.ord").bind(schema).bind(&object.name).fetch_all(&mut *connection).await.map_err(map_error)?;
                let generated=sqlx::query_scalar::<_,String>("SELECT a.attname FROM pg_attribute a JOIN pg_class c ON c.oid=a.attrelid JOIN pg_namespace n ON n.oid=c.relnamespace LEFT JOIN pg_attrdef d ON d.adrelid=a.attrelid AND d.adnum=a.attnum WHERE n.nspname=$1 AND c.relname=$2 AND (a.attgenerated<>'' OR a.attidentity<>'' OR pg_get_expr(d.adbin,d.adrelid) LIKE 'nextval(%') ORDER BY a.attnum").bind(schema).bind(&object.name).fetch_all(&mut *connection).await.map_err(map_error)?;
                response.editing=Some(EditingMetadata{primary_key:keys,generated,transactional:true,warnings:vec!["Constraints, row policies and triggers apply; external trigger effects are not atomic".into()]});
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
            let quote = |name: &str| format!("\"{}\"", name.replace('"', "\"\""));
            let qualified = format!("{}.{}", quote(schema), quote(&object.name));
            if matches!(object.kind, ObjectKind::View | ObjectKind::MaterializedView) {
                let definition:Option<String>=sqlx::query_scalar("SELECT pg_get_viewdef(c.oid,true) FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 AND c.relname=$2 AND c.relkind IN ('v','m')").bind(schema).bind(&object.name).fetch_optional(&mut *connection).await.map_err(map_error)?;
                response.ddl = definition.map(|ddl| {
                    format!(
                        "CREATE {}VIEW {qualified} AS\n{ddl}",
                        if object.kind == ObjectKind::MaterializedView {
                            "MATERIALIZED "
                        } else {
                            ""
                        }
                    )
                });
            } else if object.kind == ObjectKind::Sequence {
                let row=sqlx::query("SELECT s.seqstart,s.seqincrement,s.seqmin,s.seqmax,s.seqcache,s.seqcycle,format_type(s.seqtypid,NULL) AS type FROM pg_sequence s JOIN pg_class c ON c.oid=s.seqrelid JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 AND c.relname=$2").bind(schema).bind(&object.name).fetch_optional(&mut *connection).await.map_err(map_error)?;
                if let Some(row) = row {
                    response.ddl = Some(format!(
                        "CREATE SEQUENCE {qualified} AS {} INCREMENT BY {} MINVALUE {} MAXVALUE {} START WITH {} CACHE {} {};",
                        row.try_get::<String, _>("type").map_err(map_error)?,
                        row.try_get::<i64, _>("seqincrement").map_err(map_error)?,
                        row.try_get::<i64, _>("seqmin").map_err(map_error)?,
                        row.try_get::<i64, _>("seqmax").map_err(map_error)?,
                        row.try_get::<i64, _>("seqstart").map_err(map_error)?,
                        row.try_get::<i64, _>("seqcache").map_err(map_error)?,
                        if row.try_get::<bool, _>("seqcycle").map_err(map_error)? {
                            "CYCLE"
                        } else {
                            "NO CYCLE"
                        }
                    ));
                }
            } else if !response.columns.is_empty() {
                let mut definitions = response
                    .columns
                    .iter()
                    .map(|c| {
                        format!(
                            "  {} {}{}{}",
                            quote(&c.name),
                            c.data_type,
                            if c.nullable { "" } else { " NOT NULL" },
                            c.default
                                .as_ref()
                                .map(|d| format!(" DEFAULT {d}"))
                                .unwrap_or_default()
                        )
                    })
                    .collect::<Vec<_>>();
                definitions.extend(
                    response
                        .constraints
                        .iter()
                        .map(|c| format!("  CONSTRAINT {} {}", quote(&c.name), c.definition)),
                );
                response.ddl = Some(format!(
                    "-- Reconstructed inspection DDL; not a backup/migration script.\nCREATE TABLE {qualified} (\n{}\n);\n{}",
                    definitions.join(",\n"),
                    response
                        .indexes
                        .iter()
                        .map(|i| format!("{};", i.definition))
                        .collect::<Vec<_>>()
                        .join("\n")
                ));
            }
        }
    }
    Ok(response)
}
