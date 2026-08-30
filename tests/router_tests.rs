//! T0.6 acceptance test (`docs/PLAN.md` §3 / `docs/reviews/PURPLE_TEAM.md`
//! §P3, row T0.6): `build_router(&config)` wires up the four root stub
//! routes, `ServeDir` for `/uploads` **and** `/assets/screensaver`, the
//! `/` → `/tv` redirect, and the `/tv` / `/m` Dioxus SSR routes — the single
//! router `src/main.rs` now delegates to via `server::router::run` instead
//! of building inline.
//!
//! PURPLE_TEAM.md's summary names `tower::ServiceExt::oneshot` as one way to
//! drive these assertions in-process. This suite instead boots the *exact*
//! `build_router` output behind a real ephemeral-port listener and drives it
//! with `reqwest` — the same harness `tests/http_tests.rs` already uses —
//! so no new dependency is needed in `Cargo.toml` (owned by T0.2/T0.4 per
//! `docs/reviews/PURPLE_TEAM.md` §P4; a crate addition is a Boss
//! micro-commit between waves, not a T0.6 edit). Exercising the router
//! through a real bound socket is at least as strong a proof of the
//! concrete status/content-type of each named route as an in-process
//! `oneshot` call.

#![cfg(feature = "server")]

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use family_calendar::server::config::FamilyHubConfig;
use family_calendar::server::db;
use family_calendar::server::router::build_router;

/// One throwaway data directory (and one `DATABASE_URL`/`DIOXUS_PUBLIC_PATH`
/// env setup) shared by every test in this binary — mirrors
/// `tests/http_tests.rs::init_test_env`. `db::pool()` is a process-wide
/// `OnceCell`, so the first caller's `DATABASE_URL` wins for the whole
/// binary regardless of which `FamilyHubConfig` a later test builds.
fn init_test_env() -> PathBuf {
    static ONCE: std::sync::Once = std::sync::Once::new();
    let base = std::env::temp_dir().join(format!("familyhub-router-tests-{}", std::process::id()));
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

/// A `FamilyHubConfig` rooted at the shared scratch directory. `http_addr`/
/// `tls_addr` are never bound by these tests directly (the listener below
/// binds an OS-assigned port itself), so their exact value doesn't matter.
fn test_config() -> FamilyHubConfig {
    FamilyHubConfig {
        data_dir: init_test_env(),
        http_addr: "127.0.0.1:0".parse().expect("valid socket address"),
        tls_addr: "127.0.0.1:0".parse().expect("valid socket address"),
    }
}

/// Boot `build_router(config)` behind a real listener on an OS-assigned
/// port. Dropped (and the listener closed) when the per-test tokio runtime
/// shuts down at the end of the test.
async fn spawn_router(config: &FamilyHubConfig) -> SocketAddr {
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

fn http_client_no_redirect() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(15))
        .build()
        .expect("reqwest client builds")
}

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .expect("reqwest client builds")
}

// ---------------------------------------------------------------------------
// 1. GET / -> 308 Location: /tv
// ---------------------------------------------------------------------------

#[tokio::test]
async fn root_redirects_permanently_to_tv() {
    let config = test_config();
    let addr = spawn_router(&config).await;

    let response = http_client_no_redirect()
        .get(format!("http://{addr}/"))
        .send()
        .await
        .expect("GET / should respond");

    assert_eq!(response.status().as_u16(), 308, "expected a 308 redirect");
    let location = response
        .headers()
        .get("location")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    assert_eq!(
        location, "/tv",
        "GET / must redirect to /tv, got {location:?}"
    );
}

// ---------------------------------------------------------------------------
// 2. GET /tv -> 200, renders the kiosk dashboard
// ---------------------------------------------------------------------------

#[tokio::test]
async fn tv_route_serves_the_kiosk_dashboard() {
    let config = test_config();
    let addr = spawn_router(&config).await;

    let response = http_client()
        .get(format!("http://{addr}/tv"))
        .header("accept", "text/html")
        .send()
        .await
        .expect("GET /tv should respond");

    assert_eq!(response.status().as_u16(), 200);
    let body = response.text().await.expect("response body");
    assert!(
        body.contains("Morning Routine"),
        "the TV kiosk route should render the routine panel title"
    );
    assert!(
        body.contains("Whiteboard"),
        "the TV kiosk route should render the whiteboard panel title"
    );
}

// ---------------------------------------------------------------------------
// 3. GET /m -> 200, renders the phone routine view
// ---------------------------------------------------------------------------

#[tokio::test]
async fn m_route_serves_the_phone_routine_view() {
    let config = test_config();
    let addr = spawn_router(&config).await;

    let response = http_client()
        .get(format!("http://{addr}/m"))
        .header("accept", "text/html")
        .send()
        .await
        .expect("GET /m should respond");

    assert_eq!(response.status().as_u16(), 200);
    let body = response.text().await.expect("response body");
    assert!(
        body.contains("Add photo task"),
        "the /m view should render the routine's add-task button"
    );
}

// ---------------------------------------------------------------------------
// 4. GET /manifest.webmanifest -> 200 application/manifest+json
// ---------------------------------------------------------------------------

#[tokio::test]
async fn manifest_stub_returns_manifest_json_content_type() {
    let config = test_config();
    let addr = spawn_router(&config).await;

    let response = http_client()
        .get(format!("http://{addr}/manifest.webmanifest"))
        .send()
        .await
        .expect("GET /manifest.webmanifest should respond");

    assert_eq!(response.status().as_u16(), 200);
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(
        content_type.contains("application/manifest+json"),
        "expected application/manifest+json, got {content_type:?}"
    );
}

// ---------------------------------------------------------------------------
// 5. GET /sw.js -> 200
// ---------------------------------------------------------------------------

#[tokio::test]
async fn service_worker_stub_returns_200() {
    let config = test_config();
    let addr = spawn_router(&config).await;

    let response = http_client()
        .get(format!("http://{addr}/sw.js"))
        .send()
        .await
        .expect("GET /sw.js should respond");

    assert_eq!(response.status().as_u16(), 200);
}

// ---------------------------------------------------------------------------
// 6. GET /ca.crt -> 200
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ca_cert_stub_returns_200() {
    let config = test_config();
    let addr = spawn_router(&config).await;

    let response = http_client()
        .get(format!("http://{addr}/ca.crt"))
        .send()
        .await
        .expect("GET /ca.crt should respond");

    assert_eq!(response.status().as_u16(), 200);
}

// ---------------------------------------------------------------------------
// 7. GET /health -> 200
// ---------------------------------------------------------------------------

#[tokio::test]
async fn health_stub_returns_200() {
    let config = test_config();
    let addr = spawn_router(&config).await;

    let response = http_client()
        .get(format!("http://{addr}/health"))
        .send()
        .await
        .expect("GET /health should respond");

    assert_eq!(response.status().as_u16(), 200);
}

// ---------------------------------------------------------------------------
// 8. GET /uploads/<fixture> -> 200 (ServeDir)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn uploads_route_serves_a_static_file() {
    let config = test_config();
    std::fs::create_dir_all(config.upload_dir()).expect("upload dir is creatable");
    std::fs::write(config.upload_dir().join("router-test-fixture.txt"), b"hi")
        .expect("fixture file is writable");

    let addr = spawn_router(&config).await;

    let response = http_client()
        .get(format!("http://{addr}/uploads/router-test-fixture.txt"))
        .send()
        .await
        .expect("GET /uploads/router-test-fixture.txt should respond");

    assert_eq!(response.status().as_u16(), 200);
    let body = response.text().await.expect("response body");
    assert_eq!(body, "hi");
}

// ---------------------------------------------------------------------------
// 9. GET /assets/screensaver/<fixture>.jpg -> 200 image/jpeg (ServeDir)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn screensaver_route_serves_a_jpeg_with_the_right_content_type() {
    let config = test_config();
    std::fs::create_dir_all(config.screensaver_dir()).expect("screensaver dir is creatable");
    // Minimal JPEG magic bytes (SOI marker) — enough for `ServeDir`'s
    // extension-based content-type guess, which is what this route's
    // acceptance test is actually about (T0.7 supplies real photographs
    // later; T0.6 only proves the route is wired up).
    std::fs::write(
        config.screensaver_dir().join("router-test-fixture.jpg"),
        [0xFF, 0xD8, 0xFF, 0xE0],
    )
    .expect("fixture jpeg is writable");

    let addr = spawn_router(&config).await;

    let response = http_client()
        .get(format!(
            "http://{addr}/assets/screensaver/router-test-fixture.jpg"
        ))
        .send()
        .await
        .expect("GET /assets/screensaver/router-test-fixture.jpg should respond");

    assert_eq!(response.status().as_u16(), 200);
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(
        content_type.contains("image/jpeg"),
        "expected image/jpeg, got {content_type:?}"
    );
}

// ---------------------------------------------------------------------------
// Q1-01: /tailwind.css is served from the binary itself, not through the
// manganis `asset!()` placeholder — the fix for the un-rewritten,
// never-hydrating `family-hub.exe` kiosk.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn tailwind_css_is_served_from_the_binary_at_a_stable_url() {
    let config = test_config();
    let addr = spawn_router(&config).await;

    let response = http_client()
        .get(format!("http://{addr}/tailwind.css"))
        .send()
        .await
        .expect("GET /tailwind.css should respond");

    assert_eq!(response.status().as_u16(), 200);
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(
        content_type.contains("text/css"),
        "expected text/css, got {content_type:?}"
    );

    let body = response.text().await.expect("response body");
    assert!(
        body.contains("sheffield-accent"),
        "expected the committed assets/tailwind.css content (with the sheffield-* \
         palette), got a body of {} bytes",
        body.len()
    );
}

// ---------------------------------------------------------------------------
// main.rs shape: < 25 lines, no route definitions, frozen thereafter.
// ---------------------------------------------------------------------------

#[test]
fn main_rs_is_under_twenty_five_lines_and_defines_no_routes() {
    let main_rs = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/main.rs"))
        .expect("src/main.rs is readable");

    let line_count = main_rs.lines().count();
    assert!(
        line_count < 25,
        "src/main.rs must be under 25 lines (PLAN v2 T0.6), got {line_count}"
    );

    for needle in [".route(", ".nest_service(", "axum::Router::new()"] {
        assert!(
            !main_rs.contains(needle),
            "src/main.rs must not define routes any more (found {needle:?}); \
             routes live in src/server/router.rs"
        );
    }
}
