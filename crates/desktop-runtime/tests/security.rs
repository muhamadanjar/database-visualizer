use db_types::*;
use desktop_runtime::*;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
use uuid::Uuid;

#[derive(Default)]
struct Secrets {
    values: Mutex<HashMap<String, Vec<u8>>>,
    locked: AtomicBool,
}
impl SecretStore for Secrets {
    fn get(&self, key: &str) -> DbResult<Vec<u8>> {
        if self.locked.load(Ordering::Acquire) {
            return Err(DbError::new("SECRET_STORE_UNAVAILABLE", "secret", "locked"));
        }
        self.values
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .ok_or_else(|| DbError::new("SECRET_MISSING", "secret", "missing"))
    }
    fn set(&self, key: &str, value: &[u8]) -> DbResult<()> {
        if self.locked.load(Ordering::Acquire) {
            return Err(DbError::new("SECRET_STORE_UNAVAILABLE", "secret", "locked"));
        }
        self.values.lock().unwrap().insert(key.into(), value.into());
        Ok(())
    }
    fn delete(&self, key: &str) -> DbResult<()> {
        if self.locked.load(Ordering::Acquire) {
            return Err(DbError::new("SECRET_STORE_UNAVAILABLE", "secret", "locked"));
        }
        self.values.lock().unwrap().remove(key);
        Ok(())
    }
}
fn profile() -> Profile {
    Profile {
        id: Uuid::new_v4(),
        revision: 0,
        name: "Test".into(),
        engine: Engine::Postgres,
        host: "127.0.0.1".into(),
        port: 55432,
        username: "studio".into(),
        database: "studio_test".into(),
        ssl: "disable".into(),
        connect_timeout_ms: 10000,
        query_timeout_ms: None,
        history_enabled: false,
        restore_sql: false,
    }
}
#[test]
fn secrets_and_sql_never_enter_sqlite_plaintext() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(Secrets::default());
    let store = Persistence::open(dir.path(), secrets.clone()).unwrap();
    let mut p = profile();
    p.history_enabled = true;
    let p = store
        .save_profile(p, Some("unique-secret-password"), true)
        .unwrap();
    assert_eq!(store.password(p.id).unwrap(), "unique-secret-password");
    let document = Document {
        id: Uuid::new_v4(),
        connection_id: p.id,
        kind: "saved".into(),
        name: "Saved".into(),
        content: serde_json::json!({"sql":"SELECT 'unique-secret-SQL'"}),
        created_at_ms: 1,
    };
    store.save_document(&document).unwrap();
    assert_eq!(
        store.documents(p.id, "saved").unwrap()[0].content,
        document.content
    );
    let bytes = std::fs::read(dir.path().join("workspace.sqlite3")).unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert!(!text.contains("unique-secret-password"));
    assert!(!text.contains("unique-secret-SQL"));
    secrets.locked.store(true, Ordering::Release);
    assert!(store.documents(p.id, "saved").is_err());
    assert_eq!(store.profiles().unwrap().len(), 1);
}
#[test]
fn failed_secret_rotation_keeps_old_credential_and_revision() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(Secrets::default());
    let store = Persistence::open(dir.path(), secrets.clone()).unwrap();
    let p = store.save_profile(profile(), Some("old"), true).unwrap();
    secrets.locked.store(true, Ordering::Release);
    assert!(store.save_profile(p.clone(), Some("new"), true).is_err());
    assert_eq!(store.profile(p.id).unwrap().revision, p.revision);
    secrets.locked.store(false, Ordering::Release);
    store.cleanup_secrets().unwrap();
    assert_eq!(store.password(p.id).unwrap(), "old");
    let updated = store.save_profile(p, None, false).unwrap();
    assert_eq!(store.password(updated.id).unwrap(), "old");
}
#[test]
fn history_opt_in_layout_allowlist_and_schema_downgrade_guard() {
    let dir = tempfile::tempdir().unwrap();
    let store = Persistence::open(dir.path(), Arc::new(Secrets::default())).unwrap();
    let p = store.save_profile(profile(), None, false).unwrap();
    let document = Document {
        id: Uuid::new_v4(),
        connection_id: p.id,
        kind: "history".into(),
        name: "History".into(),
        content: serde_json::json!({"sql":"SELECT 1"}),
        created_at_ms: 1,
    };
    assert!(store.save_document(&document).is_err());
    store
        .save_layout(&serde_json::json!({"sidebar_width":300,"sql":"SECRET","password":"SECRET"}))
        .unwrap();
    assert_eq!(
        store.layout().unwrap(),
        serde_json::json!({"sidebar_width":300})
    );
    drop(store);
    rusqlite::Connection::open(dir.path().join("workspace.sqlite3"))
        .unwrap()
        .execute_batch("PRAGMA user_version=99")
        .unwrap();
    assert!(
        matches!(Persistence::open(dir.path(),Arc::new(Secrets::default())),Err(e) if e.code=="SCHEMA_TOO_NEW")
    );
}
#[test]
fn spool_is_encrypted_bounded_lossless_and_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let id = Uuid::new_v4();
    let total = Arc::new(AtomicU64::new(0));
    let mut spool = Spool::with_limits(dir.path(), id, total.clone(), 1000, 2000).unwrap();
    spool
        .append(&[vec![
            DbValue::Signed(9007199254740993),
            DbValue::Text("secret-result".into()),
            DbValue::Null,
        ]])
        .unwrap();
    let page = spool.page(0, 256).unwrap();
    assert_eq!(page[0][0].value.as_deref(), Some("9007199254740993"));
    assert_eq!(page[0][2].kind, "null");
    assert!(
        !String::from_utf8_lossy(&std::fs::read(dir.path().join(format!("{id}.spool"))).unwrap())
            .contains("secret-result")
    );
    assert!(
        spool
            .append(&[vec![DbValue::Text("x".repeat(2000))]])
            .is_err()
    );
    assert_eq!(spool.rows(), 1);
    assert!(spool.page(0, 257).is_err());
    drop(spool);
    assert_eq!(total.load(Ordering::Acquire), 0);
    assert!(!dir.path().join(format!("{id}.spool")).exists());
}
#[test]
fn missing_sql_key_does_not_overwrite_existing_ciphertext() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(Secrets::default());
    let store = Persistence::open(dir.path(), secrets.clone()).unwrap();
    let p = store.save_profile(profile(), None, false).unwrap();
    store
        .save_document(&Document {
            id: Uuid::new_v4(),
            connection_id: p.id,
            kind: "saved".into(),
            name: "saved".into(),
            content: serde_json::json!({"sql":"SELECT 1"}),
            created_at_ms: 1,
        })
        .unwrap();
    secrets.values.lock().unwrap().remove("sql-key/v1");
    assert!(matches!(store.documents(p.id,"saved"),Err(e) if e.code=="ENCRYPTION_KEY_MISSING"));
    assert!(!secrets.values.lock().unwrap().contains_key("sql-key/v1"));
}

#[test]
fn document_pages_omit_sql_and_document_ids_cannot_change_kind() {
    let dir = tempfile::tempdir().unwrap();
    let store = Persistence::open(dir.path(), Arc::new(Secrets::default())).unwrap();
    let mut p = profile();
    p.restore_sql = true;
    let p = store.save_profile(p, None, false).unwrap();
    let mut last = None;
    for n in 0..101 {
        let document = Document {
            id: Uuid::new_v4(),
            connection_id: p.id,
            kind: "saved".into(),
            name: format!("Query {n}"),
            content: serde_json::json!({"sql":"SELECT 'private-history-text'"}),
            created_at_ms: n,
        };
        store.save_document(&document).unwrap();
        last = Some(document);
    }
    let headers = store.document_headers(p.id, "saved", 0, 100).unwrap();
    assert_eq!(headers.len(), 100);
    assert_eq!(
        store
            .document_headers(p.id, "saved", 100, 100)
            .unwrap()
            .len(),
        1
    );
    assert!(
        !serde_json::to_string(&headers)
            .unwrap()
            .contains("private-history-text")
    );
    let mut document = last.unwrap();
    assert_eq!(
        store.document(p.id, "saved", document.id).unwrap().content,
        document.content
    );
    store
        .rename_document(p.id, "saved", document.id, "Renamed")
        .unwrap();
    assert_eq!(
        store.document(p.id, "saved", document.id).unwrap().name,
        "Renamed"
    );
    document.kind = "editor".into();
    assert_eq!(
        store.save_document(&document).unwrap_err().code,
        "DOCUMENT_ID_CONFLICT"
    );
    assert_eq!(
        store
            .document_headers(p.id, "editor", 0, 100)
            .unwrap()
            .len(),
        0
    );
    assert!(
        store
            .document(Uuid::new_v4(), "saved", document.id)
            .is_err()
    );
}

#[test]
fn bounded_search_favorites_cursors_and_mutation_restart_are_private() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(Secrets::default());
    let store = Persistence::open(dir.path(), secrets.clone()).unwrap();
    let p = store.save_profile(profile(), None, false).unwrap();
    let mut ids = Vec::new();
    for n in 0..105 {
        let id = Uuid::new_v4();
        ids.push(id);
        store
            .save_document(&Document {
                id,
                connection_id: p.id,
                kind: "saved".into(),
                name: format!("Query {n}"),
                content: serde_json::json!({"sql":"SELECT 'private needle'"}),
                created_at_ms: n + 1,
            })
            .unwrap();
    }
    store.favorite(p.id, ids[0], true).unwrap();
    let search = query_runtime::search::SearchRequest {
        kind: "saved".into(),
        query: "NEEDLE".into(),
        from_ms: None,
        to_ms: None,
        status: None,
        favorites_only: false,
        cursor: None,
    };
    let first = store.search_documents(p.id, search.clone()).unwrap();
    assert_eq!(first["items"].as_array().unwrap().len(), 100);
    assert_eq!(first["complete"], false);
    let mut continuation = search.clone();
    continuation.cursor = Some(first["next_cursor"].as_str().unwrap().into());
    let second = store.search_documents(p.id, continuation.clone()).unwrap();
    assert_eq!(second["items"].as_array().unwrap().len(), 5);
    assert_eq!(second["complete"], true);
    continuation.query = "changed".into();
    assert!(store.search_documents(p.id, continuation).is_err());
    let favorites = store
        .search_documents(
            p.id,
            query_runtime::search::SearchRequest {
                favorites_only: true,
                ..search
            },
        )
        .unwrap();
    assert_eq!(favorites["items"].as_array().unwrap().len(), 1);
    let id = Uuid::new_v4();
    let tab = Uuid::new_v4();
    assert_eq!(
        store
            .admit_mutation(id, p.id, tab, b"private row request")
            .unwrap(),
        None
    );
    assert_eq!(
        store
            .admit_mutation(id, p.id, tab, b"private row request")
            .unwrap(),
        Some("Applying".into())
    );
    assert!(store.admit_mutation(id, p.id, tab, b"different").is_err());
    drop(store);
    let store = Persistence::open(dir.path(), secrets).unwrap();
    assert_eq!(
        store
            .admit_mutation(id, p.id, tab, b"private row request")
            .unwrap(),
        Some("Unknown".into())
    );
    let bytes = std::fs::read(dir.path().join("workspace.sqlite3")).unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert!(!text.contains("private needle"));
    assert!(!text.contains("private row request"));
}

#[test]
fn sqlite_v1_upgrade_backup_can_restore_encrypted_documents() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(Secrets::default());
    let store = Persistence::open(dir.path(), secrets.clone()).unwrap();
    let p = store
        .save_profile(profile(), Some("fixture-password"), true)
        .unwrap();
    let doc = Document {
        id: Uuid::new_v4(),
        connection_id: p.id,
        kind: "saved".into(),
        name: "Backup fixture".into(),
        content: serde_json::json!({"sql":"SELECT encrypted_backup_marker"}),
        created_at_ms: 1,
    };
    store.save_document(&doc).unwrap();
    drop(store);
    for file in std::fs::read_dir(dir.path()).unwrap().flatten() {
        if file
            .file_name()
            .to_string_lossy()
            .starts_with("workspace.before-v2-")
        {
            std::fs::remove_file(file.path()).unwrap();
        }
    }
    let database = dir.path().join("workspace.sqlite3");
    let db = rusqlite::Connection::open(&database).unwrap();
    db.execute_batch("DROP TABLE favorites; DROP TABLE mutations; PRAGMA user_version=1")
        .unwrap();
    drop(db);
    let upgraded = Persistence::open(dir.path(), secrets.clone()).unwrap();
    assert_eq!(
        upgraded.document(p.id, "saved", doc.id).unwrap().content,
        doc.content
    );
    let backup = std::fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .find(|f| {
            f.file_name()
                .to_string_lossy()
                .starts_with("workspace.before-v2-")
        })
        .unwrap()
        .path();
    let version: i32 = rusqlite::Connection::open(&backup)
        .unwrap()
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 1);
    assert!(
        !String::from_utf8_lossy(&std::fs::read(&backup).unwrap())
            .contains("encrypted_backup_marker")
    );
    drop(upgraded);
    std::fs::copy(backup, &database).unwrap();
    let restored = Persistence::open(dir.path(), secrets).unwrap();
    assert_eq!(
        restored.document(p.id, "saved", doc.id).unwrap().content,
        doc.content
    );
}
