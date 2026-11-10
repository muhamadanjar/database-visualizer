use db_types::*;
use desktop_runtime::*;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use uuid::Uuid;
#[derive(Default)]
struct Secrets(Mutex<HashMap<String, Vec<u8>>>);
impl SecretStore for Secrets {
    fn get(&self, key: &str) -> DbResult<Vec<u8>> {
        self.0
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .ok_or_else(|| DbError::new("SECRET_MISSING", "secret", "missing"))
    }
    fn set(&self, key: &str, value: &[u8]) -> DbResult<()> {
        self.0.lock().unwrap().insert(key.into(), value.into());
        Ok(())
    }
    fn delete(&self, key: &str) -> DbResult<()> {
        self.0.lock().unwrap().remove(key);
        Ok(())
    }
}
async fn wait(host: &DesktopHost, id: Uuid) -> ExecutionInfo {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let info = host.snapshot(id).unwrap();
            if info.status.terminal() {
                return info;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}
#[tokio::test]
#[ignore = "requires disposable Docker databases"]
async fn desktop_host_scripts_sessions_history_and_cancellation() {
    for engine in [Engine::Postgres, Engine::MySql] {
        let directory = tempfile::tempdir().unwrap();
        let host =
            DesktopHost::open(directory.path().into(), Arc::new(Secrets::default())).unwrap();
        let mut profile = host
            .save_profile(
                Profile {
                    id: Uuid::new_v4(),
                    revision: 0,
                    name: "Desktop fixture".into(),
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
                    connect_timeout_ms: 10000,
                    query_timeout_ms: None,
                    history_enabled: true,
                    restore_sql: false,
                },
                Some("studio_test".into()),
                false,
            )
            .unwrap();
        let session = host.connect(profile.id, None).await.unwrap();
        let script = host
            .start(
                session.id,
                "SELECT 9007199254740993 AS precise; SELECT 42;".into(),
            )
            .await
            .unwrap();
        let result = wait(&host, script).await;
        assert_eq!(result.status, QueryStatus::Completed);
        assert_eq!(result.results.len(), 2);
        assert_eq!(
            host.page(script, result.results[0].id, 0, 100).unwrap()[0][0]
                .value
                .as_deref(),
            Some("9007199254740993")
        );
        let result_id = result.results[0].id;
        let filtered = host
            .transform_result(script, result_id, None, false, "missing")
            .unwrap();
        assert_eq!(filtered.results[0].rows, 0);
        let reset = host
            .transform_result(script, result_id, None, false, "")
            .unwrap();
        assert_eq!(reset.results[0].rows, 1);
        assert_eq!(
            host.page(script, result_id, 0, 1).unwrap()[0][0]
                .value
                .as_deref(),
            Some("9007199254740993")
        );
        host.close_execution(script).unwrap();
        let table = format!("desktop_page_{}", Uuid::new_v4().simple());
        let setup=host.start(session.id,format!("CREATE TABLE {table}(id INTEGER,label TEXT); INSERT INTO {table} VALUES (1,'percent%literal'),(2,'normal'),(3,'x_underscore');")).await.unwrap();
        assert_eq!(wait(&host, setup).await.status, QueryStatus::Completed);
        host.close_execution(setup).unwrap();
        let result=host.request(serde_json::json!({"operation":"table","args":{"session_id":session.id,"object":{"engine":engine,"database":"studio_test","schema":if engine==Engine::Postgres{Some("public")}else{None},"name":table,"kind":"Table"},"filter_column":"label","filter":"%","sort_column":"id","descending":true,"limit":100}})).await.unwrap();
        let id = Uuid::parse_str(result["execution_id"].as_str().unwrap()).unwrap();
        let info = wait(&host, id).await;
        assert_eq!(info.status, QueryStatus::Completed);
        assert_eq!(info.results[0].rows, 1);
        assert_eq!(
            host.page(id, info.results[0].id, 0, 1).unwrap()[0][1]
                .value
                .as_deref(),
            Some("percent%literal")
        );
        host.close_execution(id).unwrap();
        let cleanup = host
            .start(session.id, format!("DROP TABLE {table}"))
            .await
            .unwrap();
        assert_eq!(wait(&host, cleanup).await.status, QueryStatus::Completed);
        host.close_execution(cleanup).unwrap();
        let script = host
            .start(
                session.id,
                "SELECT 1; SELECT nonexistent_column; SELECT 999;".into(),
            )
            .await
            .unwrap();
        let result = wait(&host, script).await;
        assert_eq!(result.status, QueryStatus::Failed);
        assert_eq!(result.results.len(), 2);
        host.close_execution(script).unwrap();
        host.auto_commit(session.id, false).await.unwrap();
        let script = host.start(session.id, "SELECT 1;".into()).await.unwrap();
        wait(&host, script).await;
        assert_eq!(
            host.session(session.id).await.unwrap().transaction,
            TransactionState::Active
        );
        assert!(host.auto_commit(session.id, true).await.is_err());
        host.close_execution(script).unwrap();
        let script = host.start(session.id, "ROLLBACK;".into()).await.unwrap();
        wait(&host, script).await;
        host.close_execution(script).unwrap();
        host.auto_commit(session.id, true).await.unwrap();
        let sql = if engine == Engine::Postgres {
            "SELECT pg_sleep(20); SELECT 999;"
        } else {
            "SELECT SLEEP(20); SELECT 999;"
        };
        let script = host.start(session.id, sql.into()).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(host.start(session.id, "SELECT 2".into()).await.is_err());
        host.cancel(script).await.unwrap();
        let result = wait(&host, script).await;
        assert_eq!(result.results.len(), 1);
        host.close_execution(script).unwrap();
        for _ in 0..100 {
            if host
                .persistence
                .documents(profile.id, "history")
                .unwrap()
                .len()
                >= 5
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(
            host.persistence
                .documents(profile.id, "history")
                .unwrap()
                .len()
                >= 5
        );
        let table = format!("desktop_edit_{}", Uuid::new_v4().simple());
        let collation_column = if engine == Engine::Postgres {
            ", collated TEXT COLLATE \"C\""
        } else {
            ""
        };
        let execution = host
            .start(
                session.id,
                format!(
                    "CREATE TABLE {table} (id INTEGER PRIMARY KEY,label VARCHAR(80){collation_column}); INSERT INTO {table} (id,label) VALUES(1,'original')"
                ),
            )
            .await
            .unwrap();
        assert_eq!(wait(&host, execution).await.status, QueryStatus::Completed);
        host.close_execution(execution).unwrap();
        if engine == Engine::MySql {
            let protected_table = format!("desktop_auto_{}", Uuid::new_v4().simple());
            let create = host
                .start(
                    session.id,
                    format!(
                        "CREATE TABLE {protected_table} (id INT NOT NULL AUTO_INCREMENT PRIMARY KEY, note VARCHAR(20) COMMENT 'keep me', code VARCHAR(20) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin) DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci"
                    ),
                )
                .await
                .unwrap();
            assert_eq!(wait(&host, create).await.status, QueryStatus::Completed);
            host.close_execution(create).unwrap();
            let protected_object = ObjectId {
                engine,
                database: "studio_test".into(),
                schema: None,
                name: protected_table.clone(),
                kind: ObjectKind::Table,
            };
            let alter = serde_json::json!({
                "table": protected_object,
                "operation": {
                    "type": "alter_column",
                    "column": {
                        "name": "id",
                        "data_type": "BIGINT",
                        "nullable": false,
                        "default": null
                    }
                }
            });
            let inspected = host
                .request(serde_json::json!({
                    "operation": "metadata",
                    "args": {
                        "connection_id": profile.id,
                        "request": { "Inspect": { "object": protected_object } }
                    }
                }))
                .await
                .unwrap();
            assert!(
                inspected["columns"][0]["extra"]
                    .as_str()
                    .unwrap()
                    .contains("auto_increment")
            );
            assert_eq!(inspected["columns"][1]["comment"], "keep me");
            assert_eq!(inspected["table_collation"], "utf8mb4_general_ci");
            assert_eq!(inspected["columns"][2]["collation"], "utf8mb4_bin");
            let collation_error = host
                .request(serde_json::json!({
                    "operation": "schema_preview",
                    "args": {
                        "connection_id": profile.id,
                        "request": {
                            "table": protected_object,
                            "operation": {
                                "type": "alter_column",
                                "column": {
                                    "name": "code",
                                    "data_type": "VARCHAR(30)",
                                    "nullable": true,
                                    "default": null
                                }
                            }
                        }
                    }
                }))
                .await
                .unwrap_err();
            assert_eq!(collation_error.code, "UNSUPPORTED_SCHEMA_CHANGE");
            let comment_error = host
                .request(serde_json::json!({
                    "operation": "schema_preview",
                    "args": {
                        "connection_id": profile.id,
                        "request": {
                            "table": protected_object,
                            "operation": {
                                "type": "alter_column",
                                "column": {
                                    "name": "note",
                                    "data_type": "VARCHAR(30)",
                                    "nullable": true,
                                    "default": null
                                }
                            }
                        }
                    }
                }))
                .await
                .unwrap_err();
            assert_eq!(comment_error.code, "UNSUPPORTED_SCHEMA_CHANGE");
            let preview_error = host
                .request(serde_json::json!({
                    "operation": "schema_preview",
                    "args": { "connection_id": profile.id, "request": alter.clone() }
                }))
                .await
                .unwrap_err();
            assert_eq!(preview_error.code, "UNSUPPORTED_SCHEMA_CHANGE");
            let apply_error = host
                .request(serde_json::json!({
                    "operation": "schema_apply",
                    "args": {
                        "connection_id": profile.id,
                        "expected_profile_revision": profile.revision,
                        "request": {
                            "table": protected_object,
                            "operation": alter["operation"],
                            "confirmation": { "confirmed": true, "typed_name": "id" }
                        }
                    }
                }))
                .await
                .unwrap_err();
            assert_eq!(apply_error.code, "UNSUPPORTED_SCHEMA_CHANGE");
            let cleanup = host
                .start(session.id, format!("DROP TABLE {protected_table}"))
                .await
                .unwrap();
            assert_eq!(wait(&host, cleanup).await.status, QueryStatus::Completed);
            host.close_execution(cleanup).unwrap();
        }
        let history_before_schema = host
            .persistence
            .documents(profile.id, "history")
            .unwrap()
            .len();

        let schema_object = ObjectId {
            engine,
            database: "studio_test".into(),
            schema: if engine == Engine::Postgres {
                Some("public".into())
            } else {
                None
            },
            name: table.clone(),
            kind: ObjectKind::Table,
        };
        if engine == Engine::Postgres {
            let metadata = host
                .request(serde_json::json!({
                    "operation": "metadata",
                    "args": {
                        "connection_id": profile.id,
                        "request": {
                            "Inspect": { "object": schema_object.clone() }
                        }
                    }
                }))
                .await
                .unwrap();
            let collated = metadata["columns"]
                .as_array()
                .unwrap()
                .iter()
                .find(|column| column["name"] == "collated")
                .unwrap();
            assert!(collated["collation"].as_str().is_some());
            let error = host
                .request(serde_json::json!({
                    "operation": "schema_preview",
                    "args": {
                        "connection_id": profile.id,
                        "request": {
                            "table": schema_object.clone(),
                            "operation": {
                                "type": "alter_column",
                                "column": {
                                    "name": "collated",
                                    "data_type": "TEXT",
                                    "nullable": false,
                                    "default": null
                                }
                            }
                        }
                    }
                }))
                .await
                .unwrap_err();
            assert_eq!(error.code, "UNSUPPORTED_SCHEMA_CHANGE");
        }
        let default_value = format!(r"\'; DROP TABLE {table}; -- ignored");
        let ddl = host
            .request(serde_json::json!({
                "operation": "schema_apply",
                "args": {
                    "connection_id": profile.id,
                    "expected_profile_revision": profile.revision,
                    "request": {
                        "table": schema_object,
                        "operation": {
                            "type": "add_column",
                            "column": {
                                "name": "note",
                                "data_type": "VARCHAR(255)",
                                "nullable": false,
                                "default": { "type": "text", "value": default_value }
                            }
                        }
                    }
                }
            }))
            .await
            .unwrap();
        let ddl_id = Uuid::parse_str(ddl["execution_id"].as_str().unwrap()).unwrap();
        assert_eq!(wait(&host, ddl_id).await.status, QueryStatus::Completed);
        host.close_execution(ddl_id).unwrap();
        assert_eq!(
            host.persistence
                .documents(profile.id, "history")
                .unwrap()
                .len(),
            history_before_schema
        );
        let verify = host
            .start(session.id, format!("SELECT note FROM {table}"))
            .await
            .unwrap();
        let result = wait(&host, verify).await;
        assert_eq!(result.status, QueryStatus::Completed);
        assert_eq!(
            host.page(verify, result.results[0].id, 0, 1).unwrap()[0][0]
                .value
                .as_deref(),
            Some(default_value.as_str())
        );
        host.close_execution(verify).unwrap();
        if engine == Engine::Postgres {
            host.auto_commit(session.id, false).await.unwrap();
            let lock = host
                .start(
                    session.id,
                    format!("LOCK TABLE {table} IN ACCESS EXCLUSIVE MODE"),
                )
                .await
                .unwrap();
            assert_eq!(wait(&host, lock).await.status, QueryStatus::Completed);
            host.close_execution(lock).unwrap();
            let ddl = host
                .request(serde_json::json!({
                    "operation": "schema_apply",
                    "args": {
                        "connection_id": profile.id,
                        "expected_profile_revision": profile.revision,
                        "request": {
                            "table": {
                                "engine": "Postgres",
                                "database": "studio_test",
                                "schema": "public",
                                "name": table,
                                "kind": "Table"
                            },
                            "operation": {
                                "type": "add_column",
                                "column": {
                                    "name": "blocked",
                                    "data_type": "INTEGER",
                                    "nullable": true,
                                    "default": null
                                }
                            }
                        }
                    }
                }))
                .await
                .unwrap();
            let ddl_id = Uuid::parse_str(ddl["execution_id"].as_str().unwrap()).unwrap();
            tokio::time::sleep(Duration::from_secs(11)).await;
            let pending = host.snapshot(ddl_id).unwrap();
            assert!(!pending.status.terminal());
            assert!(host.session(pending.session_id).await.is_ok());
            let rollback = host.start(session.id, "ROLLBACK".into()).await.unwrap();
            assert_eq!(wait(&host, rollback).await.status, QueryStatus::Completed);
            host.close_execution(rollback).unwrap();
            host.auto_commit(session.id, true).await.unwrap();
            assert_eq!(wait(&host, ddl_id).await.status, QueryStatus::Completed);
            for _ in 0..100 {
                if host.session(pending.session_id).await.is_err() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            assert!(host.session(pending.session_id).await.is_err());
            host.close_execution(ddl_id).unwrap();
        }
        let tab = Uuid::new_v4();
        let key = Uuid::new_v4();
        let object = ObjectId {
            engine,
            database: "studio_test".into(),
            schema: if engine == Engine::Postgres {
                Some("public".into())
            } else {
                None
            },
            name: table.clone(),
            kind: ObjectKind::Table,
        };
        let snapshot = host.request(serde_json::json!({"operation":"edit_snapshot","args":{"connection_id":profile.id,"tab_id":tab,"object":object}})).await.unwrap();
        let request = serde_json::json!({"operation":"edit_apply","args":{"connection_id":profile.id,"tab_id":tab,"expected_profile_revision":snapshot["profile_revision"],"idempotency_key":key,"batch":{"tab_id":tab,"token":snapshot["token"],"changes":[{"type":"update","row":0,"values":{"label":{"type":"value","value":"changed"}}}]}}});
        assert_eq!(
            host.request(request.clone()).await.unwrap()["state"],
            "Applying"
        );
        let state = tokio::time::timeout(Duration::from_secs(10),async {loop {
            let state = host.request(serde_json::json!({"operation":"edit_status","args":{"connection_id":profile.id,"tab_id":tab,"idempotency_key":key}})).await.unwrap();
            if state["state"]!="Applying" {break state;}
            tokio::time::sleep(Duration::from_millis(20)).await;
        }}).await.unwrap();
        assert_eq!(state["state"], "Applied");
        assert_eq!(host.request(request).await.unwrap(), state);
        let receipt = host.request(serde_json::json!({"operation":"edit_readback","args":{"connection_id":profile.id,"tab_id":tab,"token":snapshot["token"]}})).await.unwrap();
        assert_eq!(receipt[0][1]["value"], "changed");
        let stale_tab = Uuid::new_v4();
        let stale_key = Uuid::new_v4();
        let stale_snapshot = host
            .request(serde_json::json!({
                "operation": "edit_snapshot",
                "args": {
                    "connection_id": profile.id,
                    "tab_id": stale_tab,
                    "object": object
                }
            }))
            .await
            .unwrap();
        profile = host
            .save_profile(profile.clone(), Some("studio_test".into()), false)
            .unwrap();
        let stale_apply = host
            .request(serde_json::json!({
                "operation": "edit_apply",
                "args": {
                    "connection_id": profile.id,
                    "tab_id": stale_tab,
                    "expected_profile_revision": stale_snapshot["profile_revision"],
                    "idempotency_key": stale_key,
                    "batch": {
                        "tab_id": stale_tab,
                        "token": stale_snapshot["token"],
                        "changes": [{
                            "type": "update",
                            "row": 0,
                            "values": { "label": { "type": "value", "value": "stale" } }
                        }]
                    }
                }
            }))
            .await
            .unwrap_err();
        assert_eq!(stale_apply.code, "SCHEMA_PROFILE_CHANGED");

        let execution = host
            .start(session.id, format!("SELECT label FROM {table}"))
            .await
            .unwrap();
        let result = wait(&host, execution).await;
        assert_eq!(
            host.page(execution, result.results[0].id, 0, 1).unwrap()[0][0]
                .value
                .as_deref(),
            Some("changed")
        );
        host.close_execution(execution).unwrap();
        let approved = host
            .request(serde_json::json!({
                "operation": "schema_preview",
                "args": {
                    "connection_id": profile.id,
                    "request": {
                        "table": schema_object,
                        "operation": { "type": "drop_table" }
                    }
                }
            }))
            .await
            .unwrap();
        assert_eq!(approved["profile_revision"], profile.revision);
        let changed_profile = host
            .save_profile(profile.clone(), Some("studio_test".into()), false)
            .unwrap();
        assert_ne!(changed_profile.revision, profile.revision);
        let stale_apply = host
            .request(serde_json::json!({
                "operation": "schema_apply",
                "args": {
                    "connection_id": profile.id,
                    "expected_profile_revision": approved["profile_revision"],
                    "request": {
                        "table": schema_object,
                        "operation": { "type": "drop_table" },
                        "confirmation": {
                            "confirmed": true,
                            "typed_name": table
                        }
                    }
                }
            }))
            .await
            .unwrap_err();
        assert_eq!(stale_apply.code, "SCHEMA_PROFILE_CHANGED");
        let execution = host
            .start(session.id, format!("DROP TABLE {table}"))
            .await
            .unwrap();
        assert_eq!(wait(&host, execution).await.status, QueryStatus::Completed);
        host.close_execution(execution).unwrap();
        host.disconnect(session.id, true).await.unwrap();
        host.delete_profile(profile.id, false).await.unwrap();
        host.shutdown().await.unwrap();
    }
}
