use db_core::*;
use db_types::*;
use sqlx::{Connection, Executor};
use std::{sync::Arc, time::Duration};
use uuid::Uuid;
fn config(engine: Engine) -> ConnectionConfig {
    ConnectionConfig {
        id: Uuid::new_v4(),
        engine,
        host: "127.0.0.1".into(),
        tcp_address: None,
        port: if engine == Engine::Postgres {
            55432
        } else {
            53306
        },
        username: "studio".into(),
        password: "studio_test".into(),
        database: "studio_test".into(),
        ssl: SslMode::Disable,
        connect_timeout: Duration::from_secs(10),
    }
}
#[tokio::test]
#[ignore = "requires disposable Docker databases"]
async fn postgres_extended_objects_have_inspection_definitions() {
    let config = config(Engine::Postgres);
    let schema = format!("desktop_{}", Uuid::new_v4().simple());
    let mut connection =
        sqlx::PgConnection::connect("postgres://studio:studio_test@127.0.0.1:55432/studio_test")
            .await
            .unwrap();
    connection.execute(format!("CREATE SCHEMA {schema}; CREATE TABLE {schema}.items (id INTEGER PRIMARY KEY); CREATE MATERIALIZED VIEW {schema}.snapshot AS SELECT id FROM {schema}.items; CREATE TYPE {schema}.mood AS ENUM ('ok','bad'); CREATE TYPE {schema}.pair AS (id INTEGER,label TEXT); CREATE TYPE {schema}.span AS RANGE (SUBTYPE=integer); CREATE DOMAIN {schema}.positive AS integer CHECK(VALUE>0); CREATE FUNCTION {schema}.answer() RETURNS integer LANGUAGE SQL AS 'SELECT 42'; CREATE PROCEDURE {schema}.noop() LANGUAGE SQL AS 'SELECT 1'; CREATE FUNCTION {schema}.on_item() RETURNS trigger LANGUAGE plpgsql AS 'BEGIN RETURN NEW; END'; CREATE TRIGGER item_change BEFORE INSERT ON {schema}.items FOR EACH ROW EXECUTE FUNCTION {schema}.on_item();").as_str()).await.unwrap();
    let core = DatabaseCore::new(
        vec![Arc::new(db_postgres::PostgresAdapter)],
        RuntimeLimits::default(),
        vec![],
    )
    .unwrap();
    for kind in [
        ObjectKind::MaterializedView,
        ObjectKind::Function,
        ObjectKind::Procedure,
        ObjectKind::Trigger,
        ObjectKind::CustomType,
    ] {
        let collection = core
            .metadata(
                &config,
                &MetadataRequest::Objects {
                    schema: Some(schema.clone()),
                    kind,
                },
            )
            .await
            .unwrap();
        assert!(!collection.objects.is_empty(), "{kind:?}");
        for object in collection.objects {
            let inspected = core
                .metadata(&config, &MetadataRequest::Inspect { object })
                .await
                .unwrap();
            assert!(inspected.ddl.is_some(), "{kind:?}");
            if kind == ObjectKind::MaterializedView {
                assert_eq!(inspected.columns.len(), 1);
            }
        }
    }
    connection
        .execute(format!("DROP SCHEMA {schema} CASCADE").as_str())
        .await
        .unwrap();
    core.shutdown().await.unwrap();
}
#[tokio::test]
#[ignore = "requires disposable Docker databases"]
async fn mysql_routines_and_triggers_have_show_create_definitions() {
    let config = config(Engine::MySql);
    let name = format!("desktop_{}", Uuid::new_v4().simple());
    let mut connection =
        sqlx::MySqlConnection::connect("mysql://root:studio_test@127.0.0.1:53306/studio_test")
            .await
            .unwrap();
    for sql in [
        format!("CREATE TABLE {name}(id INT)"),
        format!(
            "CREATE DEFINER='studio'@'%' FUNCTION {name}_f() RETURNS INT DETERMINISTIC NO SQL RETURN 42"
        ),
        format!("CREATE DEFINER='studio'@'%' PROCEDURE {name}_p() SELECT 42"),
        format!(
            "CREATE DEFINER='studio'@'%' TRIGGER {name}_t BEFORE INSERT ON {name} FOR EACH ROW SET NEW.id=42"
        ),
    ] {
        connection.execute(sql.as_str()).await.unwrap();
    }
    let core = DatabaseCore::new(
        vec![Arc::new(db_mysql::MySqlAdapter)],
        RuntimeLimits::default(),
        vec![],
    )
    .unwrap();
    for kind in [
        ObjectKind::Function,
        ObjectKind::Procedure,
        ObjectKind::Trigger,
    ] {
        let objects = core
            .metadata(&config, &MetadataRequest::Objects { schema: None, kind })
            .await
            .unwrap();
        let object = objects
            .objects
            .into_iter()
            .find(|o| o.name.starts_with(&name))
            .unwrap();
        let inspected = core
            .metadata(&config, &MetadataRequest::Inspect { object })
            .await
            .unwrap();
        assert!(inspected.ddl.unwrap().contains("CREATE"));
    }
    for sql in [
        format!("DROP TABLE {name}"),
        format!("DROP FUNCTION {name}_f"),
        format!("DROP PROCEDURE {name}_p"),
    ] {
        connection.execute(sql.as_str()).await.unwrap();
    }
    core.shutdown().await.unwrap();
}
