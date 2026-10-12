use db_core::{DatabaseCore, ProtectedTarget, QueryStream, RuntimeLimits};
use db_types::*;
use std::{sync::Arc, time::Duration};
use uuid::Uuid;
fn config(engine: Engine) -> ConnectionConfig {
    ConnectionConfig {
        id: Uuid::new_v4(),
        engine,
        host: "127.0.0.1".into(),
        tcp_address: None,
        port: std::env::var(if engine == Engine::Postgres {
            "STUDIO_PG_PORT"
        } else {
            "STUDIO_MYSQL_PORT"
        })
        .unwrap_or_else(|_| {
            if engine == Engine::Postgres {
                "55432".into()
            } else {
                "53306".into()
            }
        })
        .parse()
        .unwrap(),
        username: "studio".into(),
        password: std::env::var("STUDIO_TEST_PASSWORD").unwrap_or_else(|_| "studio_test".into()),
        database: "studio_test".into(),
        ssl: SslMode::Disable,
        connect_timeout: Duration::from_secs(10),
    }
}
fn core(limits: RuntimeLimits) -> DatabaseCore {
    DatabaseCore::new(
        vec![
            Arc::new(db_postgres::PostgresAdapter),
            Arc::new(db_mysql::MySqlAdapter),
        ],
        limits,
        vec![ProtectedTarget {
            host: "127.0.0.1".into(),
            port: 55433,
            database: "studio_app".into(),
        }],
    )
    .unwrap()
}
async fn events(stream: &mut QueryStream) -> Vec<QueryEvent> {
    let mut events = Vec::new();
    while let Some(event) = tokio::time::timeout(Duration::from_secs(10), stream.recv())
        .await
        .unwrap()
    {
        events.push(event);
    }
    assert_eq!(events.iter().filter(|e| e.data.terminal()).count(), 1);
    events
}
async fn run(core: &DatabaseCore, id: SessionId, sql: &str) -> Vec<QueryEvent> {
    let mut stream = core
        .execute(QueryRequest {
            session_id: id,
            sql: sql.into(),
            timeout: None,
        })
        .await
        .unwrap();
    events(&mut stream).await
}
fn completed(events: &[QueryEvent]) {
    assert!(
        matches!(
            events.last().unwrap().data,
            QueryEventData::QueryCompleted { .. }
        ),
        "{:?}",
        events.last()
    );
}
fn row_values(events: &[QueryEvent]) -> Vec<ResultRow> {
    events
        .iter()
        .flat_map(|e| match &e.data {
            QueryEventData::Rows { rows, .. } => rows.clone(),
            _ => Vec::new(),
        })
        .collect()
}

async fn workflow(engine: Engine) {
    let core = core(RuntimeLimits::default());
    let config = config(engine);
    core.test_connection(config.clone()).await.unwrap();
    let id = core.connect(config.clone()).await.unwrap();
    let table = format!("fixture_{}", Uuid::new_v4().simple());
    let child = format!("child_{}", Uuid::new_v4().simple());
    let view = format!("view_{}", Uuid::new_v4().simple());
    let sequence = format!("seq_{}", Uuid::new_v4().simple());
    completed(&run(&core,id,&format!("CREATE TABLE {table} (id INT PRIMARY KEY, name VARCHAR(64) NOT NULL DEFAULT 'initial', amount DECIMAL(30,9), UNIQUE(name), CHECK(amount >= 0))")).await);
    completed(&run(&core,id,&format!("CREATE TABLE {child} (id INT PRIMARY KEY, parent_id INT, FOREIGN KEY(parent_id) REFERENCES {table}(id))")).await);
    completed(
        &run(
            &core,
            id,
            &format!("CREATE VIEW {view} AS SELECT id FROM {table}"),
        )
        .await,
    );
    if engine == Engine::Postgres {
        completed(&run(&core, id, &format!("CREATE SEQUENCE {sequence}")).await);
    }
    completed(
        &run(
            &core,
            id,
            &format!(
                "INSERT INTO {table} (id,name,amount) VALUES (1,'one',9007199254740993.123456789)"
            ),
        )
        .await,
    );
    let rows = row_values(
        &run(
            &core,
            id,
            &format!("SELECT id,name,amount,NULL AS absent FROM {table}"),
        )
        .await,
    );
    assert_eq!(rows[0][0], DbValue::Signed(1));
    assert_eq!(rows[0][1], DbValue::Text("one".into()));
    assert_eq!(
        rows[0][2],
        DbValue::Decimal("9007199254740993.123456789".into())
    );
    assert_eq!(rows[0][3], DbValue::Null);
    let empty = run(&core, id, &format!("SELECT id FROM {table} WHERE id=99")).await;
    completed(&empty);
    assert!(
        empty
            .iter()
            .any(|e| matches!(&e.data,QueryEventData::Columns {columns} if columns.len()==1))
    );
    let dbs = core
        .metadata(&config, &MetadataRequest::Databases)
        .await
        .unwrap();
    assert!(dbs.objects.iter().any(|o| o.name == "studio_test"));
    let schema = if engine == Engine::Postgres {
        let s = core
            .metadata(&config, &MetadataRequest::Schemas)
            .await
            .unwrap();
        assert!(s.objects.iter().any(|o| o.name == "public"));
        Some("public".into())
    } else {
        assert!(
            core.metadata(&config, &MetadataRequest::Schemas)
                .await
                .is_err()
        );
        None
    };
    for (kind, name) in [
        (ObjectKind::Table, table.clone()),
        (ObjectKind::View, view.clone()),
    ] {
        let objects = core
            .metadata(
                &config,
                &MetadataRequest::Objects {
                    schema: schema.clone(),
                    kind,
                },
            )
            .await
            .unwrap();
        assert!(objects.objects.iter().any(|o| o.name == name));
    }
    if engine == Engine::Postgres {
        let seqs = core
            .metadata(
                &config,
                &MetadataRequest::Objects {
                    schema: schema.clone(),
                    kind: ObjectKind::Sequence,
                },
            )
            .await
            .unwrap();
        assert!(seqs.objects.iter().any(|o| o.name == sequence));
    }
    let object = ObjectId {
        engine,
        database: config.database.clone(),
        schema: schema.clone(),
        name: table.clone(),
        kind: ObjectKind::Table,
    };
    let inspect = core
        .metadata(&config, &MetadataRequest::Inspect { object })
        .await
        .unwrap();
    assert_eq!(inspect.columns.len(), 3);
    assert!(
        inspect
            .ddl
            .as_ref()
            .is_some_and(|ddl| ddl.contains("CREATE TABLE"))
    );
    assert!(
        inspect.columns[2]
            .data_type
            .to_lowercase()
            .contains("(30,9)")
    );
    assert!(!inspect.columns[1].nullable);
    assert!(inspect.columns[1].default.is_some());
    assert!(inspect.indexes.iter().any(|i| i.unique));
    assert!(inspect.constraints.iter().any(|c| c.kind == "PRIMARY KEY"));
    assert!(inspect.constraints.iter().any(|c| c.kind == "UNIQUE"));
    assert!(
        inspect
            .constraints
            .iter()
            .any(|c| c.kind == "CHECK" && c.definition.contains("amount"))
    );
    let foreign = core
        .metadata(
            &config,
            &MetadataRequest::Inspect {
                object: ObjectId {
                    engine,
                    database: config.database.clone(),
                    schema,
                    name: child.clone(),
                    kind: ObjectKind::Table,
                },
            },
        )
        .await
        .unwrap();
    assert!(
        foreign
            .constraints
            .iter()
            .any(|c| c.kind == "FOREIGN KEY" && c.definition.contains(&table))
    );
    let temp = if engine == Engine::Postgres {
        "CREATE TEMP TABLE session_local (n INT)"
    } else {
        "CREATE TEMPORARY TABLE session_local (n INT)"
    };
    completed(&run(&core, id, temp).await);
    completed(&run(&core, id, "BEGIN").await);
    completed(&run(&core, id, "INSERT INTO session_local VALUES (42)").await);
    assert_eq!(
        row_values(&run(&core, id, "SELECT n FROM session_local").await)[0][0],
        DbValue::Signed(42)
    );
    completed(&run(&core, id, "ROLLBACK").await);
    assert!(row_values(&run(&core, id, "SELECT n FROM session_local").await).is_empty());
    assert_eq!(
        core.transaction_state(id).await.unwrap(),
        TransactionState::Idle
    );
    completed(&run(&core, id, &format!("DROP VIEW {view}")).await);
    completed(&run(&core, id, &format!("DROP TABLE {child}")).await);
    completed(&run(&core, id, &format!("DROP TABLE {table}")).await);
    if engine == Engine::Postgres {
        completed(&run(&core, id, &format!("DROP SEQUENCE {sequence}")).await);
    }
    let next = core.reconnect(id).await.unwrap();
    assert_ne!(next, id);
    let missing = run(&core, next, "SELECT n FROM session_local").await;
    assert!(matches!(
        missing.last().unwrap().data,
        QueryEventData::QueryFailed { .. }
    ));
    core.disconnect(next).await.unwrap();
    assert!(!core.session_valid(next).await);
    core.shutdown().await.unwrap();
}

async fn cancellation(engine: Engine) {
    let core = core(RuntimeLimits::default());
    let config = config(engine);
    let id = core.connect(config.clone()).await.unwrap();
    completed(
        &run(
            &core,
            id,
            if engine == Engine::Postgres {
                "CREATE TEMP TABLE cancel_fixture (n INT)"
            } else {
                "CREATE TEMPORARY TABLE cancel_fixture (n INT)"
            },
        )
        .await,
    );
    completed(&run(&core, id, "INSERT INTO cancel_fixture VALUES (1)").await);
    completed(&run(&core, id, "BEGIN").await);
    let sql = if engine == Engine::Postgres {
        "SELECT pg_sleep(30) FROM cancel_fixture"
    } else {
        "SELECT n FROM cancel_fixture WHERE SLEEP(30)"
    };
    let mut stream = core
        .execute(QueryRequest {
            session_id: id,
            sql: sql.into(),
            timeout: None,
        })
        .await
        .unwrap();
    assert_eq!(
        core.execute(QueryRequest {
            session_id: id,
            sql: "SELECT 1".into(),
            timeout: None
        })
        .await
        .err()
        .unwrap()
        .code,
        "SESSION_BUSY"
    );
    // Independent metadata remains usable while the editor is blocked.
    core.metadata(&config, &MetadataRequest::Databases)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(core.cancel(stream.query_id).await);
    assert!(core.cancel(stream.query_id).await);
    let result = events(&mut stream).await;
    assert!(
        matches!(
            result.last().unwrap().data,
            QueryEventData::QueryCancelled {
                session_valid: true,
                ..
            }
        ),
        "{:?}",
        result.last()
    );
    assert!(!core.cancel(stream.query_id).await);
    assert!(core.session_valid(id).await);
    completed(&run(&core, id, "ROLLBACK").await);
    completed(&run(&core, id, "SELECT n FROM cancel_fixture").await);
    core.shutdown().await.unwrap();
}

async fn bounded_results(engine: Engine) {
    let limits = RuntimeLimits {
        batch_rows: 4,
        batch_bytes: 8192,
        queue_bytes: 16384,
        consumer_timeout: Duration::from_millis(150),
        ..Default::default()
    };
    let core = core(limits);
    let id = core.connect(config(engine)).await.unwrap();
    let sql = if engine == Engine::Postgres {
        "SELECT n,repeat('x',2000),pg_sleep(0.003) FROM generate_series(1,100) n"
    } else {
        "WITH RECURSIVE seq(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM seq WHERE n<100) SELECT n,REPEAT('x',2000),SLEEP(0.003) FROM seq"
    };
    let mut stream = core
        .execute(QueryRequest {
            session_id: id,
            sql: sql.into(),
            timeout: None,
        })
        .await
        .unwrap();
    loop {
        let event = stream.recv().await.unwrap();
        if let QueryEventData::Rows { rows, .. } = event.data {
            assert!(rows.len() <= 4);
            assert!(
                rows.iter()
                    .flatten()
                    .map(DbValue::payload_bytes)
                    .sum::<usize>()
                    <= 8192
            );
            break;
        }
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(10), stream.wait_terminal())
            .await
            .is_err(),
        "First rows must arrive before completion"
    );
    // Stop consuming; byte/count backpressure must terminate rather than grow unbounded.
    let terminal = tokio::time::timeout(Duration::from_secs(5), stream.wait_terminal())
        .await
        .unwrap()
        .unwrap();
    assert!(
        matches!(terminal.data,QueryEventData::QueryFailed {ref error,..} if error.code=="CONSUMER_TIMEOUT"),
        "{:?}",
        terminal
    );
    drop(stream);
    assert!(core.session_valid(id).await);
    let large = run(&core, id, "SELECT REPEAT('x',9000)").await;
    assert!(
        matches!(large.last().unwrap().data,QueryEventData::QueryFailed {ref error,..} if error.code=="RESULT_ROW_TOO_LARGE"),
        "{:?}",
        large.last()
    );
    completed(&run(&core, id, "SELECT 1").await);
    core.shutdown().await.unwrap();
}

async fn limits_and_drop(engine: Engine) {
    let core = core(RuntimeLimits {
        sessions_per_profile: 1,
        sessions_total: 2,
        ..Default::default()
    });
    let config = config(engine);
    let id = core.connect(config.clone()).await.unwrap();
    assert_eq!(
        core.connect(config.clone()).await.unwrap_err().code,
        "CAPACITY_EXCEEDED"
    );
    assert_eq!(
        core.execute(QueryRequest {
            session_id: id,
            sql: "SELECT 1; SELECT 2".into(),
            timeout: None
        })
        .await
        .err()
        .unwrap()
        .code,
        "UNSUPPORTED_STATEMENT"
    );
    assert_eq!(
        core.execute(QueryRequest {
            session_id: id,
            sql: "CALL change_data()".into(),
            timeout: None
        })
        .await
        .err()
        .unwrap()
        .code,
        "UNSUPPORTED_STATEMENT"
    );
    let sql = if engine == Engine::Postgres {
        "SELECT pg_sleep(30)"
    } else {
        "SELECT 1 WHERE SLEEP(30)"
    };
    let mut stream = core
        .execute(QueryRequest {
            session_id: id,
            sql: sql.into(),
            timeout: None,
        })
        .await
        .unwrap();
    assert!(matches!(
        stream.recv().await.unwrap().data,
        QueryEventData::QueryStarted
    ));
    tokio::time::sleep(Duration::from_millis(100)).await;
    drop(stream);
    tokio::time::timeout(Duration::from_secs(6), async {
        loop {
            match core
                .execute(QueryRequest {
                    session_id: id,
                    sql: "SELECT 1".into(),
                    timeout: None,
                })
                .await
            {
                Ok(mut s) => {
                    completed(&events(&mut s).await);
                    break;
                }
                Err(e) if e.code == "SESSION_BUSY" => {
                    tokio::time::sleep(Duration::from_millis(25)).await
                }
                Err(e) => panic!("{e}"),
            }
        }
    })
    .await
    .unwrap();
    let redacted = run(&core, id, "SELECT secret_password_abc").await;
    let encoded = serde_json::to_string(&redacted).unwrap();
    assert!(!encoded.contains("secret_password_abc"));
    assert!(!format!("{config:?}").contains("studio_test"));
    core.disconnect(id).await.unwrap();
    let next = core.connect(config).await.unwrap();
    core.disconnect(next).await.unwrap();
    core.shutdown().await.unwrap();
}

macro_rules! integration {
    ($name:ident,$engine:expr,$body:ident) => {
        #[tokio::test]
        #[ignore = "requires disposable Compose databases"]
        async fn $name() {
            $body($engine).await;
        }
    };
}
integration!(postgres_workflow, Engine::Postgres, workflow);
integration!(mysql_workflow, Engine::MySql, workflow);
integration!(postgres_cancel, Engine::Postgres, cancellation);
integration!(mysql_cancel, Engine::MySql, cancellation);
integration!(postgres_bounded, Engine::Postgres, bounded_results);
integration!(mysql_bounded, Engine::MySql, bounded_results);
integration!(postgres_limits_drop, Engine::Postgres, limits_and_drop);
integration!(mysql_limits_drop, Engine::MySql, limits_and_drop);

#[tokio::test]
async fn protects_control_plane_before_network_access() {
    let core = core(RuntimeLimits::default());
    let mut config = config(Engine::Postgres);
    config.port = 55433;
    config.database = "studio_app".into();
    assert_eq!(
        core.connect(config).await.unwrap_err().code,
        "PROTECTED_TARGET"
    );
}

async fn typed_values(engine: Engine) {
    let core = core(RuntimeLimits::default());
    let id = core.connect(config(engine)).await.unwrap();
    let sql = if engine == Engine::Postgres {
        "SELECT TRUE,(-42)::bigint,1.25::float8,'payload'::text,decode('00ff','hex'),DATE '2026-10-09',TIME '12:34:56',TIMESTAMP '2026-10-09 12:34:56','{\"n\":1}'::jsonb,ARRAY[1,2]"
    } else {
        "SELECT TRUE,CAST(-42 AS SIGNED),CAST(1.25 AS DOUBLE),'payload',UNHEX('00ff'),CAST('2026-10-09' AS DATE),CAST('12:34:56' AS TIME),CAST('2026-10-09 12:34:56' AS DATETIME),CAST('{\"n\":1}' AS JSON),CAST(18446744073709551615 AS UNSIGNED)"
    };
    let result = run(&core, id, sql).await;
    completed(&result);
    let rows = row_values(&result);
    let row = &rows[0];
    if engine == Engine::Postgres {
        assert_eq!(row[0], DbValue::Boolean(true));
    } else {
        assert_eq!(row[0], DbValue::Signed(1));
    }
    assert_eq!(row[1], DbValue::Signed(-42));
    assert_eq!(row[2], DbValue::Float("1.25".into()));
    assert_eq!(row[3], DbValue::Text("payload".into()));
    assert_eq!(row[4], DbValue::Binary(vec![0, 255]));
    assert_eq!(row[5], DbValue::Date("2026-10-09".into()));
    assert!(matches!(row[6], DbValue::Time(_)));
    assert!(matches!(row[7], DbValue::Timestamp(_)));
    assert_eq!(row[8], DbValue::Json(serde_json::json!({"n":1})));
    if engine == Engine::Postgres {
        assert!(matches!(row[9], DbValue::Opaque { .. }));
    } else {
        assert_eq!(row[9], DbValue::Unsigned(u64::MAX));
    }
    let terminal = &result.last().unwrap().data;
    assert!(
        matches!(terminal,QueryEventData::QueryCompleted {summary,..} if summary.affected_rows==0 && summary.rows_returned==1)
    );
    core.shutdown().await.unwrap();
}
integration!(postgres_typed, Engine::Postgres, typed_values);
integration!(mysql_typed, Engine::MySql, typed_values);

async fn blocked_lock(engine: Engine) {
    let core = core(RuntimeLimits::default());
    let config = config(engine);
    let owner = core.connect(config.clone()).await.unwrap();
    let waiter = core.connect(config).await.unwrap();
    let table = format!("lock_{}", Uuid::new_v4().simple());
    completed(
        &run(
            &core,
            owner,
            &format!("CREATE TABLE {table}(id INT PRIMARY KEY,n INT)"),
        )
        .await,
    );
    completed(&run(&core, owner, &format!("INSERT INTO {table} VALUES(1,0)")).await);
    completed(&run(&core, owner, "BEGIN").await);
    completed(&run(&core, owner, &format!("UPDATE {table} SET n=1 WHERE id=1")).await);
    let mut stream = core
        .execute(QueryRequest {
            session_id: waiter,
            sql: format!("UPDATE {table} SET n=2 WHERE id=1"),
            timeout: Some(Duration::from_millis(200)),
        })
        .await
        .unwrap();
    let result = events(&mut stream).await;
    assert!(
        matches!(
            result.last().unwrap().data,
            QueryEventData::QueryCancelled {
                session_valid: true,
                ..
            }
        ),
        "{:?}",
        result.last()
    );
    completed(&run(&core, owner, "ROLLBACK").await);
    let rows = row_values(&run(&core, waiter, &format!("SELECT n FROM {table}")).await);
    assert_eq!(rows[0][0], DbValue::Signed(0));
    completed(&run(&core, owner, &format!("DROP TABLE {table}")).await);
    core.shutdown().await.unwrap();
}
integration!(postgres_lock, Engine::Postgres, blocked_lock);
integration!(mysql_lock, Engine::MySql, blocked_lock);

#[tokio::test]
#[ignore = "requires disposable MySQL"]
async fn mysql_successful_completion_wins_cancel_and_reports_partial_results() {
    let core = core(RuntimeLimits::default());
    let id = core.connect(config(Engine::MySql)).await.unwrap();
    let mut stream = core
        .execute(QueryRequest {
            session_id: id,
            sql: "SELECT SLEEP(30)".into(),
            timeout: None,
        })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    core.cancel(stream.query_id).await;
    let result = events(&mut stream).await;
    assert!(
        matches!(result.last().unwrap().data,QueryEventData::QueryCompleted {ref summary,..} if !summary.results_complete)
    );
    assert!(core.session_valid(id).await);
    core.shutdown().await.unwrap();
}
