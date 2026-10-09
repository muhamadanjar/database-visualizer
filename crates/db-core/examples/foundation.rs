//! Development smoke example, not a production CLI.
use db_core::{DatabaseCore, ProtectedTarget, RuntimeLimits};
use db_types::*;
use std::{sync::Arc, time::Duration};
use uuid::Uuid;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let engine = match std::env::args().nth(1).as_deref() {
        Some("mysql") => Engine::MySql,
        Some("postgres") | None => Engine::Postgres,
        _ => return Err("Use postgres or mysql".into()),
    };
    let port_name = if engine == Engine::Postgres {
        "STUDIO_PG_PORT"
    } else {
        "STUDIO_MYSQL_PORT"
    };
    let port = std::env::var(port_name)
        .unwrap_or_else(|_| {
            if engine == Engine::Postgres {
                "55432".into()
            } else {
                "53306".into()
            }
        })
        .parse()?;
    let config = ConnectionConfig {
        id: Uuid::new_v4(),
        engine,
        host: "127.0.0.1".into(),
        port,
        username: "studio".into(),
        password: std::env::var("STUDIO_TEST_PASSWORD").unwrap_or_else(|_| "studio_test".into()),
        database: "studio_test".into(),
        ssl: SslMode::Disable,
        connect_timeout: Duration::from_secs(10),
    };
    let core = DatabaseCore::new(
        vec![
            Arc::new(db_postgres::PostgresAdapter),
            Arc::new(db_mysql::MySqlAdapter),
        ],
        RuntimeLimits::default(),
        vec![ProtectedTarget {
            host: "127.0.0.1".into(),
            port: std::env::var("STUDIO_APP_PORT")
                .unwrap_or_else(|_| "55433".into())
                .parse()?,
            database: "studio_app".into(),
        }],
    )?;
    let databases = core.metadata(&config, &MetadataRequest::Databases).await?;
    println!("Visible databases: {}", databases.objects.len());
    let id = core.connect(config).await?;
    let mut stream = core
        .execute(QueryRequest {
            session_id: id,
            sql: "SELECT 42 AS answer, 'Foundation ready' AS message".into(),
            timeout: Some(Duration::from_secs(10)),
        })
        .await?;
    while let Some(event) = stream.recv().await {
        println!("{}", serde_json::to_string(&event)?);
    }
    core.shutdown().await?;
    Ok(())
}
