//! D4.1 acceptance — bundle the poster faces (`docs/design/DESIGN_DIRECTION.md`
//! §4 D4.1, §5).
//!
//! The contract, verbatim: *"(a) router `oneshot` GET
//! `/fonts/nunito-600-latin.woff2`, `/fonts/nunito-800-latin.woff2`,
//! `/fonts/baloo2-800-latin.woff2` → 200, `content-type` `font/woff2`, body
//! starts `wOF2`; (b) `input.css` contains exactly 3 `@font-face` and no
//! `http` substring; (c) compiled `assets/tailwind.css` contains `Baloo 2`
//! in a `.font-poster` rule; (d) each woff2 ≤ 120 KB and OFL files
//! present."*
//!
//! Plus the Boss addition: the service worker's app-shell precache list
//! includes the three font URLs, so a phone that installed the PWA offline
//! still renders in the poster faces without a network round trip.
//!
//! (a) is driven the same way `tests/router_tests.rs` drives every other
//! `ServeDir` route: `build_router` behind a real ephemeral-port listener,
//! `reqwest` on top — `PURPLE_TEAM.md`'s "`oneshot` is one way to drive
//! these assertions" is a suggestion, not a specific API; exercising the
//! router through a real bound socket is at least as strong a proof of the
//! concrete status/content-type of each named route.

#![cfg(feature = "server")]

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use family_calendar::server::config::FamilyHubConfig;
use family_calendar::server::db;
use family_calendar::server::router::build_router;

const MAX_WOFF2_BYTES: u64 = 120 * 1024;

const FONT_FILES: [&str; 3] = [
    "nunito-600-latin.woff2",
    "nunito-800-latin.woff2",
    "baloo2-800-latin.woff2",
];

/// One throwaway data directory (and one `DATABASE_URL`/`DIOXUS_PUBLIC_PATH`
/// env setup) shared by every test in this binary — mirrors
/// `tests/router_tests.rs::init_test_env`. `db::pool()` is a process-wide
/// `OnceCell`, so the first caller's `DATABASE_URL` wins for the whole
/// binary regardless of which `FamilyHubConfig` a later test builds.
fn init_test_env() -> PathBuf {
    static ONCE: std::sync::Once = std::sync::Once::new();
    let base = std::env::temp_dir().join(format!("familyhub-font-tests-{}", std::process::id()));
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
        screensaver_schedule_hour: None,
        log_level: None,
    }
}

/// Boot `build_router(config)` behind a real listener on an OS-assigned
/// port, mirroring `tests/router_tests.rs::spawn_router` exactly.
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

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .expect("reqwest client builds")
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

// ---------------------------------------------------------------------------
// (a) GET /fonts/<file> -> 200, content-type font/woff2, body starts wOF2
// ---------------------------------------------------------------------------

#[tokio::test]
async fn fonts_route_serves_each_woff2_with_the_right_content_type_and_magic_bytes() {
    let config = test_config();
    let addr = spawn_router(&config).await;
    let client = http_client();

    for file in FONT_FILES {
        let response = client
            .get(format!("http://{addr}/fonts/{file}"))
            .send()
            .await
            .unwrap_or_else(|err| panic!("GET /fonts/{file} should respond: {err}"));

        assert_eq!(
            response.status().as_u16(),
            200,
            "GET /fonts/{file} should be 200"
        );

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        assert!(
            content_type.contains("font/woff2"),
            "GET /fonts/{file}: expected content-type font/woff2, got {content_type:?}"
        );

        let body = response
            .bytes()
            .await
            .unwrap_or_else(|err| panic!("GET /fonts/{file} body should read: {err}"));
        assert!(
            body.starts_with(b"wOF2"),
            "GET /fonts/{file}: body must start with the wOF2 magic bytes"
        );
    }
}

// ---------------------------------------------------------------------------
// (b) input.css contains exactly 3 @font-face and no `http` substring
// ---------------------------------------------------------------------------

#[test]
fn input_css_declares_exactly_three_font_faces_and_never_reaches_the_network() {
    let input_css =
        std::fs::read_to_string(repo_root().join("input.css")).expect("input.css is readable");

    let font_face_count = input_css.matches("@font-face").count();
    assert_eq!(
        font_face_count, 3,
        "input.css must declare exactly 3 @font-face blocks (Nunito 600, Nunito 800, \
         Baloo 2 800), found {font_face_count}"
    );

    assert!(
        !input_css.contains("http"),
        "input.css must contain no `http` substring — every font URL must be the \
         same-origin absolute path /fonts/... (§5: 'No external font requests — ever.')"
    );
}

// ---------------------------------------------------------------------------
// (c) compiled assets/tailwind.css contains `Baloo 2` in a `.font-poster` rule
// ---------------------------------------------------------------------------

#[test]
fn compiled_tailwind_css_declares_the_font_poster_family_with_baloo_2() {
    let tailwind_css = std::fs::read_to_string(repo_root().join("assets/tailwind.css"))
        .expect("assets/tailwind.css is readable — did you forget to rebuild it?");

    let rule_start = tailwind_css
        .find(".font-poster")
        .expect("assets/tailwind.css must contain a .font-poster rule");
    // The rule body is whatever comes between the next `{` and its matching
    // `}` — minified Tailwind output puts everything on effectively one
    // line, so a bounded forward scan for the closing brace is enough.
    let body_start = tailwind_css[rule_start..]
        .find('{')
        .map(|offset| rule_start + offset)
        .expect(".font-poster rule must have an opening brace");
    let body_end = tailwind_css[body_start..]
        .find('}')
        .map(|offset| body_start + offset)
        .expect(".font-poster rule must have a closing brace");
    let rule_body = &tailwind_css[body_start..body_end];
    // `--minify` runs the whole sheet through cssnano, which rewrites a
    // quoted multi-word font-family value (`'Baloo 2'`/`"Baloo 2"`) into the
    // shorter, semantically-identical unquoted-identifier-with-escaped-space
    // form (`Baloo\ 2`) — valid CSS, same font, same cascade, just fewer
    // bytes. Undo that one escape before the substring check so the
    // assertion is about the family name, not this build's whitespace
    // encoding of it.
    let normalised = rule_body.replace("\\ ", " ");

    assert!(
        normalised.contains("Baloo 2"),
        ".font-poster rule must reference the Baloo 2 family, got: {rule_body}"
    );
}

// ---------------------------------------------------------------------------
// (d) each woff2 <= 120 KB, and both OFL license files are present
// ---------------------------------------------------------------------------

#[test]
fn woff2_files_are_within_the_size_budget() {
    for file in FONT_FILES {
        let path = repo_root().join("assets/fonts").join(file);
        let metadata = std::fs::metadata(&path)
            .unwrap_or_else(|err| panic!("{} must exist: {err}", path.display()));
        assert!(
            metadata.len() <= MAX_WOFF2_BYTES,
            "{} is {} bytes, over the {} byte budget",
            path.display(),
            metadata.len(),
            MAX_WOFF2_BYTES
        );
        assert!(
            metadata.len() > 0,
            "{} must not be an empty file",
            path.display()
        );
    }
}

#[test]
fn both_ofl_license_texts_are_committed() {
    for file in ["OFL-nunito.txt", "OFL-baloo2.txt"] {
        let path = repo_root().join("assets/fonts").join(file);
        let contents = std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("{} must exist and be readable: {err}", path.display()));
        assert!(
            contents.contains("SIL Open Font License"),
            "{} does not look like an OFL license text",
            path.display()
        );
    }
}

// ---------------------------------------------------------------------------
// Boss addition: the service worker's app-shell precache list carries the
// three font URLs, so an offline-installed phone PWA still renders in the
// poster faces.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn service_worker_precaches_the_nunito_600_font() {
    let config = test_config();
    let addr = spawn_router(&config).await;

    let response = http_client()
        .get(format!("http://{addr}/sw.js"))
        .send()
        .await
        .expect("GET /sw.js should respond");

    assert_eq!(response.status().as_u16(), 200);
    let body = response.text().await.expect("response body");
    assert!(
        body.contains("/fonts/nunito-600-latin.woff2"),
        "sw.js must precache /fonts/nunito-600-latin.woff2 in its app shell"
    );
}

#[test]
fn service_worker_source_stays_within_the_six_kilobyte_budget() {
    let sw_js = std::fs::read_to_string(repo_root().join("src/client/components/mobile/sw.js"))
        .expect("sw.js is readable");

    assert!(
        sw_js.len() <= 6 * 1024,
        "sw.js must stay <= 6 KB (docs/NON_RUST.md budget), got {} bytes",
        sw_js.len()
    );
    for file in FONT_FILES {
        assert!(
            sw_js.contains(&format!("/fonts/{file}")),
            "sw.js must precache /fonts/{file}"
        );
    }
}
