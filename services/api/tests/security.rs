use axum::http::StatusCode;
use query_runtime::Profile;
use std::sync::Arc;
use studio_api::{
    Limiter,
    config::{NetworkPolicy, TargetRule},
    keys::{KeyRing, SecretStore},
};
use uuid::Uuid;

#[test]
fn ciphertext_and_idempotency_are_bound_to_tenant_and_key_version() {
    let old = KeyRing::from_key("old", [3; 32]);
    let context = b"organization/workspace/resource/credential";
    let ciphertext = old.seal(context, b"fixture password").unwrap();
    assert!(
        !ciphertext
            .windows(16)
            .any(|chunk| chunk == b"fixture password")
    );
    assert_eq!(
        &*old.open(context, &ciphertext).unwrap(),
        b"fixture password"
    );
    assert!(old.open(b"different organization", &ciphertext).is_err());
    let mut corrupt = ciphertext.clone();
    let last = corrupt.len() - 1;
    corrupt[last] ^= 1;
    assert!(old.open(context, &corrupt).is_err());
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("keys.json");
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD;
    std::fs::write(&path,serde_json::to_vec(&serde_json::json!({"active":"new","keys":{"old":encoded.encode([3;32]),"new":encoded.encode([5;32])}})).unwrap()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let rotated = KeyRing::load(&path).unwrap();
    assert_eq!(
        &*rotated.open(context, &ciphertext).unwrap(),
        b"fixture password"
    );
    let request = b"canonical execution request";
    let fingerprint = old.fingerprint(context, request).unwrap();
    assert!(
        rotated
            .matches_fingerprint(context, request, &fingerprint)
            .unwrap()
    );
    assert!(
        !rotated
            .matches_fingerprint(context, b"changed request", &fingerprint)
            .unwrap()
    );
    assert!(
        !rotated
            .matches_fingerprint(b"other user", request, &fingerprint)
            .unwrap()
    );
    let new = KeyRing::from_key("new", [5; 32]);
    assert_eq!(
        new.open(context, &ciphertext).unwrap_err().code,
        "SECRET_KEY_MISSING"
    );
    assert!(
        new.matches_fingerprint(context, request, &fingerprint)
            .is_err()
    );
}

#[test]
fn network_policy_denies_unlisted_hosts_control_plane_metadata_and_unverified_tls() {
    let mut profile = Profile {
        id: Uuid::new_v4(),
        revision: 0,
        name: "Test".into(),
        engine: db_types::Engine::Postgres,
        host: "127.0.0.1".into(),
        port: 55432,
        username: "studio".into(),
        database: "studio_test".into(),
        ssl: "verify_full".into(),
        connect_timeout_ms: 1000,
        query_timeout_ms: None,
        history_enabled: false,
        restore_sql: false,
    };
    let policy = NetworkPolicy {
        rules: vec![TargetRule {
            host: "127.0.0.1".into(),
            port: 55432,
            network: "127.0.0.0/8".parse().unwrap(),
        }],
        protected: vec!["127.0.0.1:55433".parse().unwrap()],
        development: false,
    };
    assert!(policy.validate(&profile).is_ok());
    profile.ssl = "disable".into();
    assert_eq!(
        policy.validate(&profile).unwrap_err().code,
        "TLS_VERIFICATION_REQUIRED"
    );
    profile.ssl = "verify_full".into();
    profile.host = "localhost".into();
    assert_eq!(
        policy.validate(&profile).unwrap_err().code,
        "PERMISSION_DENIED"
    );
    for host in [
        "169.254.169.254",
        "100.100.100.200",
        "::ffff:169.254.169.254",
        "fe80::1",
        "fd00:ec2::254",
        "0.0.0.0",
    ] {
        profile.host = host.into();
        assert_eq!(
            policy.validate(&profile).unwrap_err().status,
            StatusCode::FORBIDDEN
        );
    }
    profile.host = "127.0.0.1".into();
    profile.port = 55433;
    assert_eq!(
        policy.validate(&profile).unwrap_err().status,
        StatusCode::FORBIDDEN
    );
}

#[test]
fn rate_limiter_has_per_key_and_global_memory_bounds() {
    let limiter = Arc::new(Limiter::default());
    assert!(limiter.check("same".into(), 1).is_ok());
    assert_eq!(
        limiter.check("same".into(), 1).unwrap_err().status,
        StatusCode::TOO_MANY_REQUESTS
    );
    for id in 0..4095 {
        assert!(limiter.check(format!("user-{id}"), 1).is_ok());
    }
    assert_eq!(
        limiter.check("over-capacity".into(), 1).unwrap_err().status,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert!(limiter.check("same".into(), 3).is_ok());
}

#[test]
fn all_dns_answers_must_match_policy_before_pinning() {
    let mut profile = Profile {
        id: Uuid::new_v4(),
        revision: 0,
        name: "TLS fixture".into(),
        engine: db_types::Engine::Postgres,
        host: "db.example.test".into(),
        port: 5432,
        username: "fixture".into(),
        database: "fixture".into(),
        ssl: "verify_full".into(),
        connect_timeout_ms: 1000,
        query_timeout_ms: None,
        history_enabled: false,
        restore_sql: false,
    };
    let policy = NetworkPolicy {
        rules: vec![TargetRule {
            host: "db.example.test".into(),
            port: 5432,
            network: "192.0.2.0/24".parse().unwrap(),
        }],
        protected: vec!["192.0.2.9:5432".parse().unwrap()],
        development: false,
    };
    assert!(policy.validate(&profile).is_ok());
    let allowed = "192.0.2.1".parse().unwrap();
    assert_eq!(
        policy.validate_addresses(&profile, &[allowed]).unwrap(),
        allowed
    );
    for rejected in [
        "198.51.100.1",
        "192.0.2.9",
        "169.254.169.254",
        "::ffff:169.254.169.254",
    ] {
        assert!(
            policy
                .validate_addresses(&profile, &[allowed, rejected.parse().unwrap()])
                .is_err()
        );
    }
    assert!(policy.validate_addresses(&profile, &[]).is_err());
    profile.host = "other.example.test".into();
    assert!(policy.validate(&profile).is_err());
}
