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

use family_calendar::client::components::tv::style::{TV_RENDER_WIDTH_PX, TV_VIEWPORT_META};
use family_calendar::server::auth;
use family_calendar::server::config::FamilyHubConfig;
use family_calendar::server::db;
use family_calendar::server::router::build_router;

/// The head shape of the `dx build` bundle's own `public/index.html`
/// (`target/dx/family-calendar/release/web/public/index.html`), reduced to the
/// parts that decide viewport policy: a `<title>`, the template's own
/// `width=device-width, initial-scale=1` meta, and the `id="main"` mount point
/// `dioxus_server::IndexHtml` splits on. The wasm `<script>` is deliberately
/// left out — this binary asserts SSR output, never hydration.
///
/// QT-01 / B-1: production always serves this template
/// (`build_router` → `ServeConfig::new()` → `<public>/index.html`), so the page
/// the television loads carries **two** viewport metas. `dioxus-server`'s
/// `ssr::render_head` writes the template head first and the collected
/// `ServerDocument` head elements after it, before `</head>`, so ours is last —
/// and Chromium applies viewport metas in document order, each replacing the
/// previous description. *That ordering is the whole fix.* Without this file the
/// harness falls back to `IndexHtml::ssr_only()` (empty head) and the ordering
/// is never exercised at all.
const DX_TEMPLATE_VIEWPORT_META: &str = "width=device-width, initial-scale=1";

const DX_TEMPLATE_INDEX_HTML: &str = r#"<!DOCTYPE html>
<html>
    <head>
        <title>Sheffield Family Hub</title>
        <meta content="text/html;charset=utf-8" http-equiv="Content-Type">
        <meta name="viewport" content="width=device-width, initial-scale=1">
        <meta charset="UTF-8">
    </head>
    <body>
        <div id="main"></div>
    </body>
</html>"#;

/// One throwaway data directory (and one `DATABASE_URL`/`DIOXUS_PUBLIC_PATH`
/// env setup) shared by every test in this binary — mirrors
/// `tests/http_tests.rs::init_test_env`. `db::pool()` is a process-wide
/// `OnceCell`, so the first caller's `DATABASE_URL` wins for the whole
/// binary regardless of which `FamilyHubConfig` a later test builds.
fn init_test_env() -> PathBuf {
    static ONCE: std::sync::Once = std::sync::Once::new();
    let base = std::env::temp_dir().join(format!("familyhub-router-tests-{}", std::process::id()));
    ONCE.call_once(|| {
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
        // QT-01: serve the same document shape production serves — template
        // head first, Dioxus's collected head elements after it. See
        // `DX_TEMPLATE_INDEX_HTML` above.
        std::fs::write(public.join("index.html"), DX_TEMPLATE_INDEX_HTML)
            .expect("test index.html is writable");
        std::env::set_var("DIOXUS_PUBLIC_PATH", &public);

        // HS9 (`docs/BACKLOG.md` B-3): this harness — never the shell — pins
        // the data directory, so nothing in this binary can resolve config to
        // the family's live `%ProgramData%\FamilyHub`.
        std::env::set_var("FAMILY_HUB_DATA_DIR", &base);
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
        screensaver_schedule_hour: None,
        log_level: None,
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
// B-1 / TV1: each surface declares its own viewport meta
// (`docs/design/PLAN_TV_VIEWPORT.md` §1.1–§1.3)
//
// One global `width=device-width` meta used to serve every route, so on the
// Insignia — `wm size` override 1920x1080 at density 320, i.e. 2 dppx and a
// 960 CSS px `device-width` — the kiosk laid out at half its design width and
// painted at 2×: one rail card, the routine rows cut at the card's edge. `/tv`
// now pins `TV_VIEWPORT_META`; `/m` and `/mobile` keep the phone's string,
// byte for byte.
//
// These tests boot `build_router` against a `DIOXUS_PUBLIC_PATH` that
// **does** hold an `index.html` — `DX_TEMPLATE_INDEX_HTML`, written by
// `init_test_env()` on purpose, with the same head shape the `dx build`
// bundle ships. So the document under test has production's *two*-viewport-meta
// structure, and the assertions below are about **order**, not count: the
// template's `width=device-width, initial-scale=1` first, the route's own meta
// last. The last meta in document order is the one Chromium applies, and that
// is the entirety of B-1's fix (plan §1.2, §5 risk 2; QT-01).
//
// (Before QT-01 the harness left that directory empty, `ServeConfig` fell back
// to `IndexHtml::ssr_only()` — head a single space — and every route carried
// exactly one meta. The suite then asserted `!body.contains("width=device-width")`
// for `/tv`, which was false of the page production actually serves.)
// ---------------------------------------------------------------------------

/// Every `content` on a `<meta name="viewport">` in `body`, in document order.
///
/// Parsed rather than substring-matched, because nothing guarantees the
/// renderer emits `name` before `content` — the acceptance criterion is about
/// the *tag*, not about a byte sequence.
fn viewport_meta_contents(body: &str) -> Vec<String> {
    /// The value of `attr` in one `<meta …>` tag's attribute text, for either
    /// quoting style. `None` when the attribute is absent.
    fn attr_value(tag: &str, attr: &str) -> Option<String> {
        let mut rest = tag;
        loop {
            let at = rest.find(attr)?;
            // Only a real attribute start: preceded by whitespace or the
            // very beginning, and followed by `=` (so `name` never matches
            // inside e.g. `data-name`).
            let boundary_ok = at == 0
                || rest[..at]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_whitespace());
            let after = rest[at + attr.len()..].trim_start();
            if boundary_ok && after.starts_with('=') {
                let value = after[1..].trim_start();
                let quote = value.chars().next()?;
                if quote == '"' || quote == '\'' {
                    let end = value[1..].find(quote)?;
                    return Some(value[1..1 + end].to_string());
                }
                // Unquoted: up to the next whitespace or the tag's end.
                let end = value
                    .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
                    .unwrap_or(value.len());
                return Some(value[..end].to_string());
            }
            rest = &rest[at + attr.len()..];
        }
    }

    let mut contents = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find("<meta") {
        rest = &rest[start + "<meta".len()..];
        let end = match rest.find('>') {
            Some(end) => end,
            None => break,
        };
        let tag = &rest[..end];
        rest = &rest[end..];
        if attr_value(tag, "name").as_deref() == Some("viewport") {
            contents.push(attr_value(tag, "content").unwrap_or_default());
        }
    }
    contents
}

/// (a) `/tv`'s viewport metas are, in document order, exactly the `dx`
/// template's and then `TV_VIEWPORT_META` — so the television lays the kiosk
/// out at the 1920 × 1080 the rail budget, the type scale and every design-QA
/// measurement are computed for.
#[tokio::test]
async fn tv_route_pins_the_kiosk_viewport_at_the_render_width() {
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

    let metas = viewport_meta_contents(&body);
    assert_eq!(
        metas,
        vec![
            DX_TEMPLATE_VIEWPORT_META.to_string(),
            TV_VIEWPORT_META.to_string(),
        ],
        "/tv must serve the dx template's viewport meta first and the kiosk's \
         **last**: Chromium applies viewport metas in document order and the last \
         one wins, which is the only reason B-1's fix takes on the television \
         (plan §1.2, §5 risk 2). Got {metas:?}"
    );

    // ...and the winning tag is the kiosk's own: `device-width` is 960 on the
    // Insignia, and `initial-scale` beside `width=1920` would turn the kiosk
    // into a horizontally scrolling page (plan §1.3).
    let winning = metas.last().expect("at least one viewport meta on /tv");
    assert!(
        !winning.contains("width=device-width"),
        "the last viewport meta on /tv must not be a device-width one (B-1), got {winning:?}"
    );
    assert!(
        !winning.contains("initial-scale"),
        "the last viewport meta on /tv must not set initial-scale (plan §1.3), got {winning:?}"
    );
}

/// B-1's load-bearing property, stated once, plainly. Plan §5's risk table
/// rates "the `dx` template's `device-width` meta wins over ours" *Very low /
/// High — the fix would be inert*, and offers exactly one proof: the on-device
/// readout of §4.3. This is the other one, and it runs on every commit.
#[tokio::test]
async fn the_kiosk_viewport_meta_is_the_last_one_in_the_document() {
    let config = test_config();
    let addr = spawn_router(&config).await;

    let body = http_client()
        .get(format!("http://{addr}/tv"))
        .header("accept", "text/html")
        .send()
        .await
        .expect("GET /tv should respond")
        .text()
        .await
        .expect("response body");

    let ours = body
        .rfind(TV_VIEWPORT_META)
        .expect("/tv must serve TV_VIEWPORT_META");
    let template = body
        .find(DX_TEMPLATE_VIEWPORT_META)
        .expect("/tv must still serve the dx template's own viewport meta");
    assert!(
        template < ours,
        "the kiosk's viewport meta must come **after** the dx template's — the last \
         one wins in Chromium, and B-1's fix is inert if that order ever inverts \
         (template at {template}, kiosk at {ours})"
    );
    let close_head = body.find("</head>").expect("/tv must have a </head>");
    assert!(
        ours < close_head,
        "the kiosk's viewport meta must be inside <head> (Dioxus writes collected \
         head elements before </head>; a meta emitted during streaming would land \
         after it — see dioxus-server ssr::render_head)"
    );
}

/// (b) The phone surface is untouched: `/m` and `/mobile` both keep the
/// original string, byte for byte, and never see the kiosk's width.
#[tokio::test]
async fn the_phone_routes_keep_the_device_width_viewport() {
    let config = test_config();
    let addr = spawn_router(&config).await;

    for path in ["/m", "/mobile"] {
        let response = http_client()
            .get(format!("http://{addr}{path}"))
            .header("accept", "text/html")
            .send()
            .await
            .unwrap_or_else(|_| panic!("GET {path} should respond"));

        assert_eq!(response.status().as_u16(), 200, "{path}");
        let body = response.text().await.expect("response body");

        let metas = viewport_meta_contents(&body);
        assert_eq!(
            metas.len(),
            2,
            "{path} must carry the dx template's viewport meta and then its own, got {metas:?}"
        );
        assert_eq!(
            metas[0], DX_TEMPLATE_VIEWPORT_META,
            "{path}: the template's meta is expected first"
        );
        let content = metas.last().expect("a viewport meta on the phone route");
        for needle in [
            "width=device-width",
            "initial-scale=1",
            "viewport-fit=cover",
        ] {
            assert!(
                content.contains(needle),
                "{path}'s viewport meta must keep {needle:?} \
                 (the PWA tab bar's safe-area inset depends on it), got {content:?}"
            );
        }
        assert!(
            !body.contains(&format!("width={TV_RENDER_WIDTH_PX}")),
            "{path} must never serve the kiosk's viewport width"
        );
    }
}

/// (c) The manifest `Link` stayed global in `App` — only the viewport moved.
/// A hashed `asset!()` URL here would put `start_url: "/m"` outside the
/// manifest's own scope and the install prompt would never appear (T2.2 / G6
/// / R-16), which is what `tests/pwa_tests.rs` guards; assert it from this
/// side too, since TV1 edits the component that renders it.
#[tokio::test]
async fn the_phone_route_still_links_the_manifest_at_its_root_url() {
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
        body.contains(r#"href="/manifest.webmanifest""#),
        "/m must still link the manifest at its root URL"
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
// 10. POST /api/login sets an HttpOnly/Secure/SameSite=Lax session cookie
//     (QA round 1, Q1-11)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn login_sets_a_well_formed_session_cookie() {
    let config = test_config();
    let addr = spawn_router(&config).await;
    let pool = db::pool().await.expect("test sqlite pool opens");

    // This test binary's only caller of `set_initial_pin` (directly or via
    // /api/setup), so it owns the PIN it sets without racing any other test
    // in this file.
    let code = auth::ensure_setup_code(pool, &config.data_dir)
        .await
        .expect("ensure a setup code exists")
        .expect("no PIN has been set yet in this fresh test binary");

    // Q2-01: before any PIN exists, /api/session must say so distinctly
    // (404), not "not signed in" (401) — a client needs to tell "run
    // first-run setup" apart from "log in" without another round trip.
    let before_pin = http_client()
        .get(format!("http://{addr}/api/session"))
        .send()
        .await
        .expect("GET /api/session should respond");
    assert_eq!(before_pin.status().as_u16(), 404);

    // Q2-01: POST /api/setup is the HTTP route a browser actually drives —
    // `set_initial_parent_pin` is a `#[server]` fn nothing in `src/client/`
    // calls. A wrong setup code is rejected without setting a PIN.
    let wrong_code = http_client()
        .post(format!("http://{addr}/api/setup"))
        .json(&serde_json::json!({ "setup_code": "000000", "pin": "482913" }))
        .send()
        .await
        .expect("POST /api/setup should respond");
    assert_eq!(wrong_code.status().as_u16(), 401);
    assert!(wrong_code.headers().get("set-cookie").is_none());

    let setup = http_client()
        .post(format!("http://{addr}/api/setup"))
        .json(&serde_json::json!({ "setup_code": code.clone(), "pin": "482913" }))
        .send()
        .await
        .expect("POST /api/setup should respond");
    assert_eq!(setup.status().as_u16(), 200);
    let setup_cookie = setup
        .headers()
        .get("set-cookie")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(
        setup_cookie.starts_with("fh_session="),
        "cookie: {setup_cookie:?}"
    );
    assert!(
        setup_cookie.contains("HttpOnly"),
        "cookie: {setup_cookie:?}"
    );
    assert!(setup_cookie.contains("Secure"), "cookie: {setup_cookie:?}");
    assert!(
        setup_cookie.contains("SameSite=Lax"),
        "cookie: {setup_cookie:?}"
    );
    assert!(setup_cookie.contains("Path=/"), "cookie: {setup_cookie:?}");

    // A second call, now that a PIN exists, is refused as a conflict rather
    // than silently re-running first-run setup.
    let second_setup = http_client()
        .post(format!("http://{addr}/api/setup"))
        .json(&serde_json::json!({ "setup_code": code, "pin": "999999" }))
        .send()
        .await
        .expect("POST /api/setup should respond");
    assert_eq!(second_setup.status().as_u16(), 409);

    let response = http_client()
        .post(format!("http://{addr}/api/login"))
        .json(&serde_json::json!({ "pin": "482913" }))
        .send()
        .await
        .expect("POST /api/login should respond");
    assert_eq!(response.status().as_u16(), 200);

    let cookie = response
        .headers()
        .get("set-cookie")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(cookie.starts_with("fh_session="), "cookie: {cookie:?}");
    assert!(cookie.contains("HttpOnly"), "cookie: {cookie:?}");
    assert!(cookie.contains("Secure"), "cookie: {cookie:?}");
    assert!(cookie.contains("SameSite=Lax"), "cookie: {cookie:?}");
    assert!(cookie.contains("Path=/"), "cookie: {cookie:?}");
    assert!(cookie.contains("Max-Age=2592000"), "cookie: {cookie:?}");

    // A wrong PIN is rejected and never sets a cookie.
    let wrong = http_client()
        .post(format!("http://{addr}/api/login"))
        .json(&serde_json::json!({ "pin": "000000" }))
        .send()
        .await
        .expect("POST /api/login should respond");
    assert_eq!(wrong.status().as_u16(), 401);
    assert!(wrong.headers().get("set-cookie").is_none());

    // Cross-origin login requests are refused outright.
    let cross_origin = http_client()
        .post(format!("http://{addr}/api/login"))
        .header("origin", "http://evil.example")
        .json(&serde_json::json!({ "pin": "482913" }))
        .send()
        .await
        .expect("POST /api/login should respond");
    assert_eq!(cross_origin.status().as_u16(), 403);

    // GET /api/session: 401 with no cookie, 204 with the one just minted —
    // the probe `mobile/session.rs::is_parent()` polls, since JS can never
    // read an HttpOnly cookie's value itself.
    let no_cookie = http_client()
        .get(format!("http://{addr}/api/session"))
        .send()
        .await
        .expect("GET /api/session should respond");
    assert_eq!(no_cookie.status().as_u16(), 401);

    let cookie_pair = cookie.split(';').next().unwrap_or_default().to_string();
    let with_cookie = http_client()
        .get(format!("http://{addr}/api/session"))
        .header("cookie", cookie_pair)
        .send()
        .await
        .expect("GET /api/session should respond");
    assert_eq!(with_cookie.status().as_u16(), 204);
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
