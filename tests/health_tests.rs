//! T1.7 acceptance suite — `/health` JSON (`docs/reviews/PURPLE_TEAM.md`
//! §P3, row T1.7).
//!
//! | # | Assertion |
//! | - | --- |
//! | a | `GET /health` -> 200 JSON with all 8 keys, correct types |
//! | b | `days_to_expiry` matches the leaf's actual `not_after` |
//!
//! The third lettered assertion — "with the DB pool closed -> 503 and
//! `"db": false`" — lives in its own test binary,
//! `tests/health_pool_closed_tests.rs`: `db::pools()` is a process-wide
//! `OnceCell` (`docs/HANDOFF.md` H-9), so closing it must not be able to
//! break any other test sharing this binary (`tests/router_tests.rs`'s own
//! `health_stub_returns_200` — now exercising this same handler — included).
//! The staleness badge state machine's unit tests live directly in
//! `src/server/health.rs` (pure, no server boot needed).

#![cfg(feature = "server")]

use std::path::PathBuf;

use family_calendar::server::config::FamilyHubConfig;
use family_calendar::server::db;
use family_calendar::server::pki::{CertProvider, SelfSignedCa};
use family_calendar::server::router::build_router;
use serde_json::Value;

/// One throwaway data directory shared by every test in this binary, mirroring
/// `tests/router_tests.rs::init_test_env`.
fn init_test_env() -> PathBuf {
    static ONCE: std::sync::Once = std::sync::Once::new();
    let base = std::env::temp_dir().join(format!("familyhub-health-tests-{}", std::process::id()));
    ONCE.call_once(|| {
        std::fs::create_dir_all(&base).expect("test scratch directory is creatable");

        let db_path = base.join("family.db");
        let url = format!(
            "sqlite://{}",
            db_path.display().to_string().replace('\\', "/")
        );
        std::env::set_var("DATABASE_URL", url);

        let public = base.join("public");
        std::fs::create_dir_all(&public).expect("test public directory is creatable");
        std::env::set_var("DIOXUS_PUBLIC_PATH", &public);
    });
    base
}

fn test_config() -> FamilyHubConfig {
    FamilyHubConfig {
        data_dir: init_test_env(),
        http_addr: "127.0.0.1:0".parse().expect("valid socket address"),
        tls_addr: "127.0.0.1:0".parse().expect("valid socket address"),
    }
}

async fn spawn_router(config: &FamilyHubConfig) -> std::net::SocketAddr {
    db::pool().await.expect("test sqlite pool opens");

    let router = build_router(config);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind an ephemeral port");
    let addr = listener.local_addr().expect("listener has a local address");

    tokio::spawn(async move {
        let _ = axum::serve(listener, router.into_make_service()).await;
    });
    addr
}

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .expect("reqwest client builds")
}

// ---------------------------------------------------------------------------
// (a) GET /health -> 200 JSON with all 8 keys, correct types
// ---------------------------------------------------------------------------

#[tokio::test]
async fn health_returns_200_with_all_eight_keys_correctly_typed() {
    let config = test_config();
    let addr = spawn_router(&config).await;

    let response = http_client()
        .get(format!("http://{addr}/health"))
        .send()
        .await
        .expect("GET /health should respond");

    assert_eq!(response.status().as_u16(), 200);
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(
        content_type.contains("application/json"),
        "expected application/json, got {content_type:?}"
    );

    let body: Value = response.json().await.expect("valid JSON body");
    let object = body.as_object().expect("/health returns a JSON object");
    assert_eq!(
        object.len(),
        8,
        "/health must carry exactly 8 keys, got {object:?}"
    );

    assert!(
        body["db"].is_boolean(),
        "db must be a bool, got {:?}",
        body["db"]
    );
    assert_eq!(body["db"], Value::Bool(true), "the test database is up");

    assert!(
        body["last_google_poll"].is_null() || body["last_google_poll"].is_string(),
        "last_google_poll must be string-or-null, got {:?}",
        body["last_google_poll"]
    );

    assert!(
        body["cert_not_after"].is_string(),
        "cert_not_after must be a string once the local CA has issued a leaf, got {:?}",
        body["cert_not_after"]
    );
    chrono::DateTime::parse_from_rfc3339(body["cert_not_after"].as_str().unwrap())
        .expect("cert_not_after must be RFC3339");

    assert!(
        body["days_to_expiry"].is_i64() || body["days_to_expiry"].is_u64(),
        "days_to_expiry must be an integer, got {:?}",
        body["days_to_expiry"]
    );

    assert!(
        body["disk_free_bytes"].is_u64(),
        "disk_free_bytes must be an unsigned integer, got {:?}",
        body["disk_free_bytes"]
    );
    assert!(
        body["disk_free_bytes"].as_u64().unwrap() > 0,
        "the test machine must report nonzero free disk space"
    );

    assert!(
        body["ws_clients"].is_u64(),
        "ws_clients must be an unsigned integer, got {:?}",
        body["ws_clients"]
    );
    assert_eq!(
        body["ws_clients"],
        Value::from(0u64),
        "no WebSocket clients are connected in this test"
    );

    assert!(
        body["uptime_seconds"].is_u64(),
        "uptime_seconds must be an unsigned integer, got {:?}",
        body["uptime_seconds"]
    );

    assert!(
        body["migration_version"].is_i64() || body["migration_version"].is_u64(),
        "migration_version must be an integer once the database has migrated, got {:?}",
        body["migration_version"]
    );
    // T1.1 lands 0001/0002 and T1.4 (same wave as T1.7) lands 0003; this was
    // `Value::from(2)` on T1.7's branch, bumped by Boss at the wave 1-b merge
    // exactly as T1.4 bumped `tests/storage_tests.rs`' own constants.
    assert_eq!(
        body["migration_version"],
        Value::from(3),
        "T1.1 lands migrations 0001 and 0002, T1.4 lands 0003"
    );
}

// ---------------------------------------------------------------------------
// (b) days_to_expiry / cert_not_after match the leaf actually being served
// ---------------------------------------------------------------------------

#[tokio::test]
async fn health_cert_fields_match_the_leaf_certificate_on_disk() {
    let config = test_config();
    let addr = spawn_router(&config).await;

    let response = http_client()
        .get(format!("http://{addr}/health"))
        .send()
        .await
        .expect("GET /health should respond");
    let body: Value = response.json().await.expect("valid JSON body");

    // The same directory /health resolved its certificate from, read
    // independently and parsed straight out of the certificate bytes
    // (`pki::parse_certificate_validity`, via `SelfSignedCa::open`) — not
    // trusting the handler's own arithmetic.
    let pki = SelfSignedCa::open(config.pki_dir()).expect("open the local CA /health just used");
    let leaf = pki.current();

    let reported_not_after =
        chrono::DateTime::parse_from_rfc3339(body["cert_not_after"].as_str().unwrap())
            .expect("cert_not_after is RFC3339")
            .timestamp();
    assert_eq!(
        reported_not_after,
        leaf.not_after.unix_timestamp(),
        "/health's cert_not_after must be the leaf's real not_after"
    );

    let reported_days = body["days_to_expiry"].as_i64().expect("integer");
    assert_eq!(
        reported_days,
        leaf.days_remaining(),
        "/health's days_to_expiry must match IssuedLeaf::days_remaining()"
    );
    // Sanity: a fresh 397-day leaf, not some unrelated near-expiry fixture.
    assert!(
        (390..=397).contains(&reported_days),
        "expected a freshly issued leaf's days_to_expiry, got {reported_days}"
    );
}
