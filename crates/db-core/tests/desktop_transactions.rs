use db_core::*;
use db_types::*;
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
async fn execute(core: &DatabaseCore, session: Uuid, sql: &str) -> QueryEvent {
    let mut stream = core
        .execute(QueryRequest {
            session_id: session,
            sql: sql.into(),
            timeout: Some(Duration::from_secs(10)),
        })
        .await
        .unwrap();
    while let Some(event) = stream.recv().await {
        if event.data.terminal() {
            return event;
        }
    }
    panic!("missing terminal")
}
#[tokio::test]
#[ignore = "requires disposable Docker databases"]
async fn protocol_transaction_status_tracks_raw_sql_and_implicit_commit() {
    let core = DatabaseCore::new(
        vec![
            Arc::new(db_postgres::PostgresAdapter),
            Arc::new(db_mysql::MySqlAdapter),
        ],
        RuntimeLimits::default(),
        vec![],
    )
    .unwrap();
    for engine in [Engine::Postgres, Engine::MySql] {
        let session = core.connect(config(engine)).await.unwrap();
        assert_eq!(
            core.transaction_state(session).await.unwrap(),
            TransactionState::Idle
        );
        execute(&core, session, "BEGIN").await;
        assert_eq!(
            core.transaction_state(session).await.unwrap(),
            TransactionState::Active
        );
        if engine == Engine::Postgres {
            execute(&core, session, "SELECT 1/0").await;
            assert_eq!(
                core.transaction_state(session).await.unwrap(),
                TransactionState::Failed
            );
        } else {
            execute(
                &core,
                session,
                "CREATE TABLE IF NOT EXISTS phase2_tx_status_probe (id INT)",
            )
            .await;
            assert_eq!(
                core.transaction_state(session).await.unwrap(),
                TransactionState::Idle
            );
            execute(&core, session, "DROP TABLE phase2_tx_status_probe").await;
        }
        execute(&core, session, "ROLLBACK").await;
        assert_eq!(
            core.transaction_state(session).await.unwrap(),
            TransactionState::Idle
        );
        execute(&core, session, "BEGIN").await;
        let mut sleeping = core
            .execute(QueryRequest {
                session_id: session,
                sql: if engine == Engine::Postgres {
                    "SELECT pg_sleep(20)"
                } else {
                    "SELECT SLEEP(20)"
                }
                .into(),
                timeout: None,
            })
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        core.cancel(sleeping.query_id).await;
        while let Some(event) = sleeping.recv().await {
            if event.data.terminal() {
                break;
            }
        }
        assert_eq!(
            core.transaction_state(session).await.unwrap(),
            if engine == Engine::Postgres {
                TransactionState::Failed
            } else {
                TransactionState::Active
            }
        );
        execute(&core, session, "ROLLBACK").await;
        execute(&core, session, "BEGIN").await;
        execute(&core, session, "COMMIT").await;
        assert_eq!(
            core.transaction_state(session).await.unwrap(),
            TransactionState::Idle
        );
        core.disconnect(session).await.unwrap();
    }
}
#[tokio::test]
#[ignore = "requires disposable Docker databases"]
async fn parameterized_table_filters_do_not_execute_sql_literals() {
    let core = DatabaseCore::new(
        vec![
            Arc::new(db_postgres::PostgresAdapter),
            Arc::new(db_mysql::MySqlAdapter),
        ],
        RuntimeLimits::default(),
        vec![],
    )
    .unwrap();
    for engine in [Engine::Postgres, Engine::MySql] {
        let session = core.connect(config(engine)).await.unwrap();
        let sql = if engine == Engine::Postgres {
            "SELECT CAST($1 AS TEXT)"
        } else {
            "SELECT CAST(? AS CHAR)"
        };
        let malicious = "x'; DROP TABLE users; --";
        let mut stream = core
            .execute_bound(
                QueryRequest {
                    session_id: session,
                    sql: sql.into(),
                    timeout: None,
                },
                vec![malicious.into()],
            )
            .await
            .unwrap();
        let mut received = false;
        while let Some(event) = stream.recv().await {
            if let QueryEventData::Rows { rows, .. } = event.data {
                assert_eq!(rows[0][0], DbValue::Text(malicious.into()));
                received = true;
            }
        }
        assert!(received);
        core.disconnect(session).await.unwrap();
    }
}
