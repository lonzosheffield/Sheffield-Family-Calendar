//! T1.7 acceptance suite — `/health` with the database pool closed
//! (`docs/reviews/PURPLE_TEAM.md` §P3, row T1.7, third lettered assertion:
//! "with the DB pool closed -> 503 and `"db": false`").
//!
//! Kept in its own test binary, deliberately: `db::pools()` is a
//! process-wide `OnceCell` (`docs/HANDOFF.md` H-9), so once it is closed
//! every other test in the same binary that touches the database would break
//! for reasons that have nothing to do with what they test. Isolating the
//! close to a binary that runs nothing else keeps `tests/health_tests.rs`'s
//! and `tests/router_tests.rs`'s (`health_stub_returns_200`, a T0.6-protected
//! assertion this handler must keep answering 200) tests honest.

#![cfg(feature = "server")]

use std::path::PathBuf;

use family_calendar::server::config::FamilyHubConfig;
use family_calendar::server::db;
use family_calendar::server::router::build_router;
use serde_json::Value;

fn init_test_env() -> PathBuf {
    let base = std::env::temp_dir().join(format!(
        "familyhub-health-pool-closed-tests-{}",
        std::process::id()
    ));
    // Windows reuses PIDs: wipe any leftover scratch dir from an earlier run first.
    let _ = std::fs::remove_dir_all(&base);
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
    base
}

fn test_config() -> FamilyHubConfig {
    FamilyHubConfig {
        data_dir: init_test_env(),
        http_addr: "127.0.0.1:0".parse().expect("valid socket address"),
        tls_addr: "127.0.0.1:0".parse().expect("valid socket address"),
        screensaver_schedule_hour: None,
        log_level: None,
    }
}

#[tokio::test]
async fn health_returns_503_and_db_false_once_the_pool_is_closed() {
    let config = test_config();

    // Boot exactly like the production `run()` and every other router test:
    // open the process-wide pools, migrate, then serve.
    db::pool().await.expect("test sqlite pool opens");

    let router = build_router(&config);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind an ephemeral port");
    let addr = listener.local_addr().expect("listener has a local address");
    tokio::spawn(async move {
        let _ = axum::serve(listener, router.into_make_service()).await;
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .expect("reqwest client builds");

    // Sanity: healthy before the close, matching
    // `tests/router_tests.rs::health_stub_returns_200` and
    // `tests/health_tests.rs`'s 200 case.
    let before = client
        .get(format!("http://{addr}/health"))
        .send()
        .await
        .expect("GET /health should respond before the pool closes");
    assert_eq!(before.status().as_u16(), 200);

    // Close the exact pools the handler's `db::read_pool()` resolves.
    db::pools()
        .await
        .expect("pools were already open")
        .close()
        .await;

    let after = client
        .get(format!("http://{addr}/health"))
        .send()
        .await
        .expect("GET /health should still respond, just unhealthy");

    assert_eq!(
        after.status().as_u16(),
        503,
        "a closed database pool must answer 503"
    );

    let body: Value = after
        .json()
        .await
        .expect("valid JSON body even when unhealthy");
    assert_eq!(
        body["db"],
        Value::Bool(false),
        "db must be false once the pool is closed, got {body:?}"
    );

    // Every other key must still be present and typed — a dead database must
    // not blank out the rest of the report.
    let object = body.as_object().expect("/health returns a JSON object");
    assert_eq!(object.len(), 8, "still all 8 keys, got {object:?}");
    assert!(body["ws_clients"].is_u64());
    assert!(body["uptime_seconds"].is_u64());
    assert!(body["disk_free_bytes"].is_u64());
    assert!(
        body["migration_version"].is_null(),
        "migration_version cannot be read once the pool is closed, got {:?}",
        body["migration_version"]
    );
}
