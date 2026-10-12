use db_types::DbValue;
use query_runtime::Spool;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use uuid::Uuid;

#[test]
fn tenant_quota_failure_rolls_back_all_reservations_and_drop_releases_storage() {
    let directory = tempfile::tempdir().unwrap();
    let total = Arc::new(AtomicU64::new(0));
    let user = Arc::new(AtomicU64::new(0));
    let workspace = Arc::new(AtomicU64::new(0));
    let mut rejected =
        Spool::with_limits(directory.path(), Uuid::new_v4(), total.clone(), 4096, 16384)
            .unwrap()
            .with_budgets(vec![(user.clone(), 4096), (workspace.clone(), 1)]);
    assert!(
        rejected
            .append(&[vec![DbValue::Text("customer row".into())]])
            .is_err()
    );
    assert_eq!(rejected.rows(), 0);
    for counter in [&total, &user, &workspace] {
        assert_eq!(counter.load(Ordering::Acquire), 0);
    }
    drop(rejected);
    let mut source =
        Spool::with_limits(directory.path(), Uuid::new_v4(), total.clone(), 4096, 16384)
            .unwrap()
            .with_budgets(vec![(user.clone(), 4096), (workspace.clone(), 4096)]);
    source
        .append(&[vec![DbValue::Signed(9007199254740993)], vec![DbValue::Null]])
        .unwrap();
    let original = source.bytes();
    assert!(original > 0);
    let view = source.transform(Some(0), false, "").unwrap();
    assert!(total.load(Ordering::Acquire) > original);
    assert_eq!(total.load(Ordering::Acquire), user.load(Ordering::Acquire));
    assert_eq!(
        total.load(Ordering::Acquire),
        workspace.load(Ordering::Acquire)
    );
    drop(view);
    assert_eq!(total.load(Ordering::Acquire), original);
    drop(source);
    for counter in [&total, &user, &workspace] {
        assert_eq!(counter.load(Ordering::Acquire), 0);
    }
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}
