use desktop_runtime::{OsSecretStore, SecretStore};
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
        }
        Err(e) => {
            println!("OS_STORE_UNAVAILABLE: {}", e.code);
            std::process::exit(2);
        }
    }
}
