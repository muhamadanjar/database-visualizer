use db_types::Engine;
use desktop_runtime::{OsSecretStore, Persistence, Profile, SecretStore};
use std::sync::Arc;
fn main() {
    let store = OsSecretStore::with_service(format!(
        "dev.database-studio.probe.{}",
        uuid::Uuid::new_v4()
    ));
    let value = b"disposable-probe-value";
    match store.set("probe", value) {
        Ok(()) => {
            assert_eq!(store.get("probe").expect("credential read failed"), value);
            store.delete("probe").expect("credential cleanup failed");
            assert!(store.get("probe").is_err());
            println!("OS_STORE_ROUNDTRIP_AND_DELETE_OK");
            let directory = tempfile::tempdir().expect("temporary data directory");
            let store = Arc::new(OsSecretStore::with_service(format!(
                "dev.database-studio.probe.{}",
                uuid::Uuid::new_v4()
            )));
            let persistence = Persistence::open(directory.path(), store.clone()).unwrap();
            let profile = persistence
                .save_profile(
                    Profile {
                        id: uuid::Uuid::new_v4(),
                        revision: 0,
                        name: "Credential probe".into(),
                        engine: Engine::Postgres,
                        host: "localhost".into(),
                        port: 5432,
                        username: "probe".into(),
                        database: "probe".into(),
                        ssl: "disable".into(),
                        connect_timeout_ms: 10000,
                        query_timeout_ms: None,
                        history_enabled: false,
                        restore_sql: false,
                    },
                    Some("disposable-probe-password"),
                    true,
                )
                .expect("OS credential could not be stored");
            drop(persistence);
            let reopened = Persistence::open(directory.path(), store.clone()).unwrap();
            assert_eq!(
                reopened
                    .password(profile.id)
                    .expect("credential after reopen"),
                "disposable-probe-password"
            );
            let reference: String =
                rusqlite::Connection::open(directory.path().join("workspace.sqlite3"))
                    .unwrap()
                    .query_row(
                        "SELECT secret_ref FROM profiles WHERE id=?",
                        [profile.id.to_string()],
                        |row| row.get(0),
                    )
                    .unwrap();
            reopened
                .delete_profile(profile.id)
                .expect("credential cleanup");
            assert_eq!(
                store
                    .get(&reference)
                    .expect_err("deleted secret remains")
                    .code,
                "SECRET_MISSING"
            );
            println!("OS_STORE_PERSISTENCE_REOPEN_AND_DELETE_OK");
        }
        Err(e) => {
            println!("OS_STORE_UNAVAILABLE: {}", e.code);
            std::process::exit(2);
        }
    }
}
