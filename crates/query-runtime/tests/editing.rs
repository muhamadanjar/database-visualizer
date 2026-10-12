use db_core::{DatabaseCore, RuntimeLimits};
use db_types::*;
use query_runtime::{
    Profile,
    edit::{EditBatch, EditChange, EditInput, EditOutcome, EditOwner, EditRuntime, collect},
};
use std::{collections::BTreeMap, sync::Arc};
use uuid::Uuid;
fn values(name: &str, value: Option<&str>) -> BTreeMap<String, EditInput> {
    [(
        name.into(),
        EditInput::Value {
            value: value.map(String::from),
        },
    )]
    .into()
}
async fn editing(engine: Engine) {
    let core = DatabaseCore::new(
        vec![
            Arc::new(db_postgres::PostgresAdapter),
            Arc::new(db_mysql::MySqlAdapter),
        ],
        RuntimeLimits::default(),
        vec![],
    )
    .unwrap();
    let profile = Profile {
        id: Uuid::new_v4(),
        revision: 1,
        name: "Disposable editing test".into(),
        engine,
        host: "127.0.0.1".into(),
        port: if engine == Engine::Postgres {
            55432
        } else {
            53306
        },
        username: "studio".into(),
        database: "studio_test".into(),
        ssl: "disable".into(),
        connect_timeout_ms: 1000,
        query_timeout_ms: None,
        history_enabled: false,
        restore_sql: false,
    };
    let config = profile.config("studio_test".into()).unwrap();
    let session = core.connect(config.clone()).await.unwrap();
    let name = format!("edit_{}", Uuid::new_v4().simple());
    let qualified = if engine == Engine::Postgres {
        format!("public.{name}")
    } else {
        format!("studio_test.{name}")
    };
    collect(&core,session,format!("CREATE TABLE {qualified} (part INTEGER NOT NULL,id BIGINT NOT NULL,label VARCHAR(100),amount DECIMAL(30,6) DEFAULT 7,PRIMARY KEY(part,id))"),vec![]).await.unwrap();
    collect(
        &core,
        session,
        format!("INSERT INTO {qualified} (part,id,label) VALUES(1,9007199254740993,'original')"),
        vec![],
    )
    .await
    .unwrap();
    let object = ObjectId {
        engine,
        database: "studio_test".into(),
        schema: if engine == Engine::Postgres {
            Some("public".into())
        } else {
            None
        },
        name,
        kind: ObjectKind::Table,
    };
    let metadata = core
        .metadata(
            &config,
            &MetadataRequest::Inspect {
                object: object.clone(),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        metadata.editing.as_ref().unwrap().primary_key,
        vec!["part", "id"]
    );
    let runtime = EditRuntime::default();
    let owner = EditOwner {
        user: "fixture".into(),
        tab: Uuid::new_v4().to_string(),
        scope: "local".into(),
    };
    let snapshot = runtime
        .snapshot(
            &core,
            session,
            &profile,
            owner.clone(),
            object.clone(),
            metadata.clone(),
        )
        .await
        .unwrap();
    assert_eq!(
        snapshot.rows[0][1].value.as_deref(),
        Some("9007199254740993")
    );
    let batch = EditBatch {
        tab_id: Uuid::new_v4(),
        token: snapshot.token,
        changes: vec![EditChange::Update {
            row: 0,
            values: values("label", Some("changed")),
        }],
    };
    let other = EditOwner {
        tab: "other".into(),
        ..owner.clone()
    };
    assert!(runtime.preview(&other, profile.id, &batch).is_err());
    assert!(
        runtime
            .preview(
                &owner,
                profile.id,
                &EditBatch {
                    changes: vec![EditChange::Update {
                        row: 0,
                        values: values("id", Some("2"))
                    }],
                    ..batch.clone()
                }
            )
            .is_err()
    );
    runtime.preview(&owner, profile.id, &batch).unwrap();
    assert_eq!(
        runtime
            .apply(&core, session, &profile, &owner, &batch, &metadata)
            .await
            .unwrap(),
        EditOutcome::Applied
    );
    assert!(
        runtime
            .apply(&core, session, &profile, &owner, &batch, &metadata)
            .await
            .is_err()
    );
    let snapshot = runtime
        .snapshot(
            &core,
            session,
            &profile,
            owner.clone(),
            object.clone(),
            metadata.clone(),
        )
        .await
        .unwrap();
    collect(
        &core,
        session,
        format!("UPDATE {qualified} SET label='concurrent'"),
        vec![],
    )
    .await
    .unwrap();
    let conflict = EditBatch {
        token: snapshot.token,
        ..batch.clone()
    };
    assert_eq!(
        runtime
            .apply(&core, session, &profile, &owner, &conflict, &metadata)
            .await
            .unwrap(),
        EditOutcome::Conflict
    );
    let snapshot = runtime
        .snapshot(
            &core,
            session,
            &profile,
            owner.clone(),
            object.clone(),
            metadata.clone(),
        )
        .await
        .unwrap();
    let mut insert = values("part", Some("1"));
    insert.extend(values("id", Some("9007199254740993")));
    let failed = EditBatch {
        token: snapshot.token,
        changes: vec![
            EditChange::Update {
                row: 0,
                values: values("label", Some("must rollback")),
            },
            EditChange::Insert { values: insert },
        ],
        ..batch.clone()
    };
    assert_eq!(
        runtime
            .apply(&core, session, &profile, &owner, &failed, &metadata)
            .await
            .unwrap(),
        EditOutcome::Failed
    );
    let (rows, _) = collect(
        &core,
        session,
        format!("SELECT label FROM {qualified}"),
        vec![],
    )
    .await
    .unwrap();
    assert_eq!(rows[0][0], DbValue::Text("concurrent".into()));
    let snapshot = runtime
        .snapshot(
            &core,
            session,
            &profile,
            owner.clone(),
            object.clone(),
            metadata.clone(),
        )
        .await
        .unwrap();
    let deleted = EditBatch {
        token: snapshot.token,
        changes: vec![EditChange::Delete { row: 0 }],
        ..batch
    };
    assert_eq!(
        runtime
            .apply(&core, session, &profile, &owner, &deleted, &metadata)
            .await
            .unwrap(),
        EditOutcome::Applied
    );
    let (rows, _) = collect(&core, session, format!("SELECT * FROM {qualified}"), vec![])
        .await
        .unwrap();
    assert!(rows.is_empty());
    let snapshot = runtime
        .snapshot(
            &core,
            session,
            &profile,
            owner.clone(),
            object.clone(),
            metadata.clone(),
        )
        .await
        .unwrap();
    let mut insert = values("part", Some("2"));
    insert.extend(values("id", Some("9007199254740994")));
    insert.extend(values("label", Some("")));
    insert.extend(values("amount", Some("123456789012345.123456")));
    let inserted = EditBatch {
        token: snapshot.token,
        changes: vec![EditChange::Insert { values: insert }],
        ..deleted.clone()
    };
    assert_eq!(
        runtime
            .apply(&core, session, &profile, &owner, &inserted, &metadata)
            .await
            .unwrap(),
        EditOutcome::Applied
    );
    let snapshot = runtime
        .snapshot(
            &core,
            session,
            &profile,
            owner.clone(),
            object.clone(),
            metadata.clone(),
        )
        .await
        .unwrap();
    let mut changed = values("label", None);
    changed.insert("amount".into(), EditInput::Default);
    let defaults = EditBatch {
        token: snapshot.token,
        changes: vec![EditChange::Update {
            row: 0,
            values: changed,
        }],
        ..inserted
    };
    assert_eq!(
        runtime
            .apply(&core, session, &profile, &owner, &defaults, &metadata)
            .await
            .unwrap(),
        EditOutcome::Applied
    );
    let (rows, _) = collect(
        &core,
        session,
        format!("SELECT label,amount FROM {qualified}"),
        vec![],
    )
    .await
    .unwrap();
    assert_eq!(rows[0][0], DbValue::Null);
    assert!(
        matches!(&rows[0][1],DbValue::Decimal(v) if &v.parse::<bigdecimal::BigDecimal>().unwrap() == 7)
    );
    collect(&core, session, format!("DROP TABLE {qualified}"), vec![])
        .await
        .unwrap();
    let generated = if engine == Engine::Postgres {
        "BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY"
    } else {
        "BIGINT AUTO_INCREMENT PRIMARY KEY"
    };
    collect(&core, session, format!("CREATE TABLE {qualified} (id {generated},label VARCHAR(100) DEFAULT 'generated-default')"),vec![]).await.unwrap();
    let metadata = core
        .metadata(
            &config,
            &MetadataRequest::Inspect {
                object: object.clone(),
            },
        )
        .await
        .unwrap();
    assert!(
        metadata
            .editing
            .as_ref()
            .unwrap()
            .generated
            .contains(&"id".into())
    );
    let snapshot = runtime
        .snapshot(
            &core,
            session,
            &profile,
            owner.clone(),
            object.clone(),
            metadata.clone(),
        )
        .await
        .unwrap();
    collect(
        &core,
        session,
        format!(
            "INSERT INTO {qualified} (label) VALUES {}",
            std::iter::repeat_n("('seed')", 101)
                .collect::<Vec<_>>()
                .join(",")
        ),
        vec![],
    )
    .await
    .unwrap();
    let insert = EditBatch {
        tab_id: Uuid::new_v4(),
        token: snapshot.token,
        changes: vec![EditChange::Insert {
            values: BTreeMap::new(),
        }],
    };
    assert_eq!(
        runtime
            .apply(&core, session, &profile, &owner, &insert, &metadata)
            .await
            .unwrap(),
        EditOutcome::Applied
    );
    let receipt = runtime.readback(&owner, profile.id, insert.token).unwrap();
    assert_eq!(receipt.len(), 1);
    assert!(
        receipt[0][0]
            .value
            .as_ref()
            .unwrap()
            .parse::<u64>()
            .unwrap()
            > 100
    );
    assert_eq!(receipt[0][1].value.as_deref(), Some("generated-default"));
    let fresh = runtime
        .snapshot(
            &core,
            session,
            &profile,
            owner.clone(),
            object.clone(),
            metadata.clone(),
        )
        .await
        .unwrap();
    assert_eq!(fresh.rows[0][1].value.as_deref(), Some("seed"));
    // A real blocked row lock is interrupted by disconnect; uncertain rollback is never reported Applied.
    collect(&core, session, "BEGIN".into(), vec![])
        .await
        .unwrap();
    collect(
        &core,
        session,
        format!("UPDATE {qualified} SET label=label"),
        vec![],
    )
    .await
    .unwrap();
    let mutation_session = core.connect(config).await.unwrap();
    let cancelled = EditBatch {
        tab_id: Uuid::new_v4(),
        token: fresh.token,
        changes: vec![EditChange::Update {
            row: 0,
            values: values("label", Some("must not persist")),
        }],
    };
    let (outcome, ()) = tokio::join!(
        runtime.apply(
            &core,
            mutation_session,
            &profile,
            &owner,
            &cancelled,
            &metadata
        ),
        async {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            core.disconnect(mutation_session).await.unwrap();
        }
    );
    assert_ne!(outcome.unwrap(), EditOutcome::Applied);
    collect(&core, session, "ROLLBACK".into(), vec![])
        .await
        .unwrap();
    let (rows, _) = collect(
        &core,
        session,
        format!("SELECT label FROM {qualified} WHERE id=1"),
        vec![],
    )
    .await
    .unwrap();
    assert_eq!(rows[0][0], DbValue::Text("seed".into()));
    collect(&core, session, format!("DROP TABLE {qualified}"), vec![])
        .await
        .unwrap();
    core.shutdown().await.unwrap();
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn postgres_atomic_edits_and_conflicts() {
    editing(Engine::Postgres).await
}
#[tokio::test]
#[ignore = "requires disposable MySQL"]
async fn mysql_atomic_edits_and_conflicts() {
    editing(Engine::MySql).await
}
