//! Disposable-fixture benchmark; creates and removes an isolated schema.
use db_core::*;
use db_types::*;
use sqlx::{Connection, Executor};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use uuid::Uuid;
#[tokio::main]
async fn main() {
    let cleanup = std::env::args().nth(1);
    let schema = cleanup
        .clone()
        .unwrap_or_else(|| format!("bench_{}", Uuid::new_v4().simple()));
    let mut connection =
        sqlx::PgConnection::connect("postgres://studio:studio_test@127.0.0.1:55432/studio_test")
            .await
            .expect("disposable fixture unavailable");
    if cleanup.as_deref() == Some("--list") {
        let schemas:Vec<String>=sqlx::query_scalar("SELECT nspname FROM pg_namespace WHERE nspname LIKE 'bench_%' AND nspowner=(SELECT oid FROM pg_roles WHERE rolname=current_user)").fetch_all(&mut connection).await.unwrap();
        println!("{}", serde_json::json!(schemas));
        return;
    }
    assert!(
        schema
            .strip_prefix("bench_")
            .is_some_and(|id| Uuid::parse_str(id).is_ok()),
        "invalid benchmark schema"
    );
    let started = Instant::now();
    if cleanup.is_some() {
        cleanup_schema(&mut connection, &schema).await;
        return;
    }
    connection
        .execute(format!("CREATE SCHEMA {schema}").as_str())
        .await
        .unwrap();
    for batch in 0..100 {
        let sql = (batch * 100..batch * 100 + 100)
            .map(|n| format!("CREATE VIEW {schema}.object_{n:05} AS SELECT {n} AS id;"))
            .collect::<String>();
        connection.execute(sql.as_str()).await.unwrap();
    }
    let setup = started.elapsed().as_millis();
    let core = DatabaseCore::new(
        vec![Arc::new(db_postgres::PostgresAdapter)],
        RuntimeLimits::default(),
        vec![],
    )
    .unwrap();
    let config = ConnectionConfig {
        id: Uuid::new_v4(),
        engine: Engine::Postgres,
        host: "127.0.0.1".into(),
        tcp_address: None,
        port: 55432,
        username: "studio".into(),
        password: "studio_test".into(),
        database: "studio_test".into(),
        ssl: SslMode::Disable,
        connect_timeout: Duration::from_secs(10),
    };
    let started = Instant::now();
    let response = core
        .metadata(
            &config,
            &MetadataRequest::Objects {
                schema: Some(schema.clone()),
                kind: ObjectKind::View,
            },
        )
        .await;
    let elapsed = started.elapsed().as_millis();
    cleanup_schema(&mut connection, &schema).await;
    let response = response.unwrap();
    assert_eq!(response.objects.len(), 10000);
    println!(
        "{}",
        serde_json::json!({"engine":"Postgres","objects":response.objects.len(),"setup_ms":setup,"metadata_ms":elapsed,"dto_bytes":serde_json::to_vec(&response).unwrap().len(),"scope":"core metadata, excluding native UI"})
    );
}
async fn cleanup_schema(connection: &mut sqlx::PgConnection, schema: &str) {
    // Drop in bounded transactions: DROP SCHEMA CASCADE of 10k relations exceeds
    // the stock fixture's max_locks_per_transaction budget.
    for batch in 0..100 {
        let sql = (batch * 100..batch * 100 + 100)
            .map(|n| format!("DROP VIEW IF EXISTS {schema}.object_{n:05};"))
            .collect::<String>();
        connection.execute(sql.as_str()).await.unwrap();
    }
    connection
        .execute(format!("DROP SCHEMA {schema}").as_str())
        .await
        .unwrap();
}
