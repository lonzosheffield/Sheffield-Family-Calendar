//! HTTP/WS integration harness.
//!
//! Landed by T0.3 against Dioxus 0.6.3 and ported here by **T0.4** to Dioxus
//! `=0.7.10`. Every T0.3 assertion is preserved verbatim; only the plumbing
//! that 0.7 removed (`ServeConfigBuilder`, the old server-function crate's
//! `PATH` associated constant) was rewritten. The tests named `migration_*` are the
//! new Gate-2 assertions from `docs/reviews/PURPLE_TEAM.md` §P2b.
//!
//! Boots the real production router (`server::router::build_router`, T0.6 —
//! `/ws`, `/uploads`, `/assets/screensaver`, the root stub routes, and the
//! Dioxus fullstack SSR + server-fn fallback) on an ephemeral port for every
//! test, so this is a true in-process integration harness rather than a set
//! of unit tests against handler functions. `/`'s own routing behaviour
//! (redirect to `/tv`) is asserted in `tests/router_tests.rs`, T0.6's
//! dedicated acceptance suite.

#![cfg(feature = "server")]

use std::net::SocketAddr;
use std::time::Duration;

use family_calendar::server::api::realtime;
use family_calendar::server::db;
use family_calendar::shared::types::{StrokePoint, StrokeSegment, WsMessage};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message as WsClientMessage;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type WsStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

/// Dioxus 0.7 mounts `#[server(endpoint = "...")]` functions under `/api`.
const TODAY_ENDPOINT: &str = "/api/today";
const TOGGLE_ROUTINE_ENDPOINT: &str = "/api/toggle_routine_task";

/// Point every test at one throwaway sqlite file instead of the real
/// `family.db` (or an on-disk-per-connection `:memory:`, which would give
/// each pooled connection its own empty database), and give Dioxus 0.7's
/// `serve_static_assets` an existing (empty) public directory — 0.7 panics
/// rather than skipping when `<exe dir>/public` is missing, which it always
/// is under `cargo test`. Idempotent, and shared by every test in this
/// binary, since `db::pool()` is a process-wide `OnceCell`. Returns the
/// scratch data directory so callers can hand it to a [`FamilyHubConfig`]
/// (T0.6: `build_router` needs one, e.g. for `upload_dir()`/
/// `screensaver_dir()`).
fn init_test_env() -> std::path::PathBuf {
    static ONCE: std::sync::Once = std::sync::Once::new();
    let base = std::env::temp_dir().join(format!("familyhub-http-tests-{}", std::process::id()));
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

/// Boot the exact production router (`server::router::build_router`, T0.6)
/// on an OS-assigned free port, and return its address. The listener (and
/// the task serving it) is dropped when the per-test tokio runtime shuts
/// down at the end of the test, so nothing leaks across runs.
///
/// 0.7 note: `ServeConfigBuilder` is gone. `ServeConfig::new()` looks for a
/// `dx`-generated `public/index.html` next to the test executable, finds none,
/// and falls back to the built-in SSR-only shell — which is exactly what these
/// server-side assertions need.
async fn spawn_test_server() -> SocketAddr {
    let base = init_test_env();
    // Warm the pool up front so a background `use_resource` fetch triggered
    // during SSR has somewhere to go, mirroring what `main.rs`/`router::run`
    // does before it starts serving.
    db::pool().await.expect("test sqlite pool opens");

    let config = family_calendar::server::config::FamilyHubConfig {
        data_dir: base,
        http_addr: "127.0.0.1:0".parse().expect("valid socket address"),
        tls_addr: "127.0.0.1:0".parse().expect("valid socket address"),
    };
    let router = family_calendar::server::router::build_router(&config);

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

/// Reads frames off `socket` until one whose text contains `marker`, or
/// panics after five seconds. Matching on a per-test marker (rather than "the
/// very next frame") keeps these tests safe to run concurrently even though
/// `realtime::sender()` is one process-wide broadcast channel shared by every
/// test in this binary.
async fn recv_matching(socket: &mut WsStream, marker: &str) -> String {
    let wait = async {
        loop {
            match socket.next().await {
                Some(Ok(WsClientMessage::Text(text))) if text.contains(marker) => {
                    return text.to_string();
                }
                Some(Ok(_)) => continue,
                Some(Err(err)) => panic!("websocket error while waiting for {marker:?}: {err}"),
                None => panic!("socket closed before a message containing {marker:?} arrived"),
            }
        }
    };

    tokio::time::timeout(Duration::from_secs(5), wait)
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for a message containing {marker:?}"))
}

/// PLAN v2 D3′ / T0.6: the TV kiosk's URL of record moved from `/` to
/// `/tv`; `/` is now a redirect (asserted below and, in full, by
/// `tests/router_tests.rs::root_redirects_permanently_to_tv`). This test
/// keeps the T0.3/T0.4 assertion that the kiosk dashboard actually renders,
/// just at the route it lives at today.
#[tokio::test]
async fn http_tv_serves_dashboard_with_panel_markers() {
    let addr = spawn_test_server().await;

    let response = http_client()
        .get(format!("http://{addr}/tv"))
        .header("accept", "text/html")
        .send()
        .await
        .expect("GET /tv should respond");

    assert_eq!(response.status().as_u16(), 200);
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(
        content_type.contains("text/html"),
        "expected a text/html content-type, got {content_type:?}"
    );

    let body = response.text().await.expect("response body");
    assert!(
        body.contains("Morning Routine"),
        "the kiosk dashboard should render the routine panel title"
    );
    assert!(
        body.contains("Whiteboard"),
        "the kiosk dashboard should render the whiteboard panel title"
    );
}

#[tokio::test]
async fn http_root_redirects_to_tv() {
    let addr = spawn_test_server().await;

    let response = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(15))
        .build()
        .expect("reqwest client builds")
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

#[tokio::test]
async fn http_mobile_serves_routine_only_view() {
    let addr = spawn_test_server().await;

    let response = http_client()
        .get(format!("http://{addr}/mobile"))
        .header("accept", "text/html")
        .send()
        .await
        .expect("GET /mobile should respond");

    assert_eq!(response.status().as_u16(), 200);
    let body = response.text().await.expect("response body");
    assert!(
        body.contains("Add photo task"),
        "the mobile view should render the routine's add-task button"
    );
    assert!(
        !body.contains("Whiteboard"),
        "the mobile route is routine-only and must not render the whiteboard panel"
    );
}

/// Gate-2 assertion 6: the short phone URL of the split-origin deployment
/// (PLAN v2 D3′) renders under the 0.7 router.
#[tokio::test]
async fn http_m_serves_routine_only_view() {
    let addr = spawn_test_server().await;

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
    assert!(
        !body.contains("Whiteboard"),
        "the /m route is routine-only and must not render the whiteboard panel"
    );
}

/// Gate-2 assertion 7: a real HTTP POST to the `today()` server function's
/// wire endpoint **over the new JSON codec** — 0.6 defaulted to URL-encoded
/// input, 0.7 defaults to JSON. Decoded exactly the way a browser client
/// would decode it, not an in-process Rust call.
#[tokio::test]
async fn http_today_server_fn_round_trip() {
    let addr = spawn_test_server().await;

    let response = http_client()
        .post(format!("http://{addr}{TODAY_ENDPOINT}"))
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .await
        .expect("the today() server-fn endpoint should respond");

    assert_eq!(response.status().as_u16(), 200);
    let body = response.text().await.expect("response body");
    let today: String =
        serde_json::from_str(&body).expect("today() encodes its result as a JSON string");
    assert_eq!(today.len(), 10, "expected YYYY-MM-DD, got {today:?}");
    assert_eq!(today.as_bytes()[4], b'-');
    assert_eq!(today.as_bytes()[7], b'-');
}

/// Gate-2 assertion 8 (ok path): a **mutating** server function round trip
/// over JSON that changes a database row.
#[tokio::test]
async fn http_toggle_routine_task_round_trip_mutates_db() {
    let addr = spawn_test_server().await;
    let pool = db::pool().await.expect("test sqlite pool opens");

    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    const USER_ID: u32 = 3;
    let template_id = db::daily_routine(pool, USER_ID, &date)
        .await
        .expect("the seeded routine templates are readable")
        .first()
        .expect("the morning routine is seeded")
        .template_id;

    // Start from a known state so the assertion below is about this call.
    db::set_routine_completion(pool, USER_ID, template_id, false, &date)
        .await
        .expect("clearing the row succeeds");

    let response = http_client()
        .post(format!("http://{addr}{TOGGLE_ROUTINE_ENDPOINT}"))
        .header("content-type", "application/json")
        .body(format!(
            r#"{{"user_id":{USER_ID},"template_id":{template_id},"completed":true}}"#
        ))
        .send()
        .await
        .expect("the toggle_routine_task endpoint should respond");

    assert_eq!(
        response.status().as_u16(),
        200,
        "body was {:?}",
        response.text().await
    );

    let completed = db::daily_routine(pool, USER_ID, &date)
        .await
        .expect("routine is readable after the mutation")
        .into_iter()
        .find(|item| item.template_id == template_id)
        .expect("the toggled template is still present")
        .completed;
    assert!(
        completed,
        "the mutating server function should have written the completion row"
    );
}

/// Gate-2 assertion 8 (error path): the 0.7 non-generic `ServerFnError` must
/// come back as a **structured error body**, not a panic. `user_id = 99`
/// violates `daily_routine_logs`' `CHECK (user_id BETWEEN 1 AND 4)`.
#[tokio::test]
async fn http_toggle_routine_task_error_is_structured_not_a_panic() {
    let addr = spawn_test_server().await;

    let response = http_client()
        .post(format!("http://{addr}{TOGGLE_ROUTINE_ENDPOINT}"))
        .header("content-type", "application/json")
        .body(r#"{"user_id":99,"template_id":1,"completed":true}"#)
        .send()
        .await
        .expect("the toggle_routine_task endpoint should respond");

    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let body = response.text().await.expect("response body");

    assert_eq!(status, 500, "expected a server error, body was {body:?}");
    assert!(
        content_type.contains("application/json"),
        "expected a JSON error payload, got {content_type:?} / {body:?}"
    );
    assert!(
        !body.contains("panicked"),
        "the error must travel as a ServerFnError, not a panic: {body:?}"
    );

    let payload: serde_json::Value =
        serde_json::from_str(&body).expect("the error body is structured JSON");
    assert_eq!(payload["code"], 500, "error payload was {payload}");
    let message = payload["message"]
        .as_str()
        .unwrap_or_else(|| panic!("error payload has no message: {payload}"));
    assert!(
        message.to_ascii_uppercase().contains("CHECK"),
        "the CHECK-constraint failure should reach the client: {message:?}"
    );
}

#[tokio::test]
async fn http_ws_route_rejects_a_plain_get() {
    let addr = spawn_test_server().await;

    let response = http_client()
        .get(format!("http://{addr}/ws"))
        .send()
        .await
        .expect("a plain GET /ws should still get an HTTP response back");

    assert_eq!(response.status().as_u16(), 400);
    let body = response.text().await.expect("response body");
    assert!(
        body.to_ascii_lowercase().contains("upgrade"),
        "expected a message about the missing upgrade headers, got {body:?}"
    );
}

/// Gate-2 assertion 9: `/ws` upgrades with a 101 and fans out under the
/// axum 0.8 WebSocket layer (`Message::Text` now carries `Utf8Bytes`).
#[tokio::test]
async fn ws_stroke_from_one_client_fans_out_to_second_client() {
    let addr = spawn_test_server().await;
    let url = format!("ws://{addr}/ws");

    let (mut client_a, upgrade_a) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("client A upgrades to a websocket");
    assert_eq!(
        upgrade_a.status().as_u16(),
        101,
        "the websocket upgrade should be a 101"
    );
    let (mut client_b, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("client B upgrades to a websocket");

    let segment = StrokeSegment {
        from: StrokePoint { x: 0.1, y: 0.2 },
        to: StrokePoint { x: 0.3, y: 0.4 },
        color: "ws-fanout-test-marker".to_string(),
        width: 2.5,
    };
    let payload = serde_json::to_string(&WsMessage::Draw {
        segment: segment.clone(),
    })
    .expect("WsMessage serializes");

    client_a
        .send(WsClientMessage::text(payload))
        .await
        .expect("client A sends a stroke over the socket");

    let received = recv_matching(&mut client_b, "ws-fanout-test-marker").await;
    let parsed: WsMessage = serde_json::from_str(&received).expect("valid WsMessage JSON");
    match parsed {
        WsMessage::Draw { segment: got } => assert_eq!(got, segment),
        other => panic!("expected client B to receive a Draw message, got {other:?}"),
    }
}

#[tokio::test]
async fn ws_server_publish_reaches_connected_client() {
    let addr = spawn_test_server().await;
    let url = format!("ws://{addr}/ws");

    let (mut client, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("client upgrades to a websocket");

    realtime::publish(&WsMessage::RoutineUpdated { user_id: 987_654 });

    let received = recv_matching(&mut client, "987654").await;
    let parsed: WsMessage = serde_json::from_str(&received).expect("valid WsMessage JSON");
    assert_eq!(parsed, WsMessage::RoutineUpdated { user_id: 987_654 });
}

// ---------------------------------------------------------------------------
// T0.4 Gate 2 — migration assertions 10 to 13
// ---------------------------------------------------------------------------

/// Gate-2 assertion 10: the file-input handler is compiled against the 0.7
/// `Vec<FileData>` shape (0.6 handed back `Option<Arc<dyn FileEngine>>`), and
/// a `FileData` can be constructed and read here to prove it.
#[tokio::test]
async fn migration_file_input_handler_takes_vec_file_data() {
    use dioxus::html::{FileData, NativeFileData};
    use dioxus::server::Bytes;
    use dioxus::CapturedError;
    use family_calendar::client::components::routine::encode_first_photo;
    use std::any::Any;
    use std::path::PathBuf;
    use std::pin::Pin;

    struct InMemoryFile {
        name: String,
        bytes: &'static [u8],
    }

    impl NativeFileData for InMemoryFile {
        fn name(&self) -> String {
            self.name.clone()
        }
        fn size(&self) -> u64 {
            self.bytes.len() as u64
        }
        fn last_modified(&self) -> u64 {
            0
        }
        fn path(&self) -> PathBuf {
            PathBuf::from(&self.name)
        }
        fn content_type(&self) -> Option<String> {
            Some("image/jpeg".to_string())
        }
        fn read_bytes(
            &self,
        ) -> Pin<Box<dyn std::future::Future<Output = Result<Bytes, CapturedError>> + 'static>>
        {
            let bytes = self.bytes;
            Box::pin(async move { Ok(Bytes::from_static(bytes)) })
        }
        fn byte_stream(
            &self,
        ) -> Pin<Box<dyn futures_util::Stream<Item = Result<Bytes, CapturedError>> + Send + 'static>>
        {
            let bytes = self.bytes;
            Box::pin(futures_util::stream::once(async move {
                Ok(Bytes::from_static(bytes))
            }))
        }
        fn read_string(
            &self,
        ) -> Pin<Box<dyn std::future::Future<Output = Result<String, CapturedError>> + 'static>>
        {
            let bytes = self.bytes;
            Box::pin(async move { Ok(String::from_utf8_lossy(bytes).into_owned()) })
        }
        fn inner(&self) -> &dyn Any {
            self
        }
    }

    // The 0.7 payload type, built by hand exactly as `event.files()` returns it.
    let files: Vec<FileData> = vec![FileData::new(InMemoryFile {
        name: "snap.jpg".to_string(),
        bytes: b"sheffield",
    })];
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].name(), "snap.jpg");

    let encoded = encode_first_photo(files)
        .await
        .expect("the handler reads the first file");
    assert_eq!(encoded, "c2hlZmZpZWxk");

    assert!(
        encode_first_photo(Vec::new()).await.is_none(),
        "an empty Vec<FileData> is the 0.7 way of saying no file was picked"
    );
}

/// Gate-2 assertion 11 (W8): no duplicate major versions of `axum`,
/// `tower-http` or `hyper` in the resolved dependency graph. Read straight
/// out of `Cargo.lock`, which is what `cargo tree -d` reports on.
#[test]
fn migration_no_duplicate_axum_tower_http_or_hyper() {
    let lock = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.lock"))
        .expect("Cargo.lock is committed next to Cargo.toml");

    let mut current_name = String::new();
    let mut versions: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for line in lock.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("name = ") {
            current_name = rest.trim_matches('"').to_string();
        } else if let Some(rest) = line.strip_prefix("version = ") {
            if matches!(current_name.as_str(), "axum" | "tower-http" | "hyper") {
                versions
                    .entry(current_name.clone())
                    .or_default()
                    .push(rest.trim_matches('"').to_string());
            }
        }
    }

    for crate_name in ["axum", "tower-http", "hyper"] {
        let found = versions
            .get(crate_name)
            .unwrap_or_else(|| panic!("{crate_name} should be in the dependency graph"));
        let majors: std::collections::BTreeSet<&str> = found
            .iter()
            .map(|version| version.split('.').next().unwrap_or(version))
            .collect();
        assert_eq!(
            majors.len(),
            1,
            "duplicate {crate_name} major versions in the tree: {found:?}"
        );
    }
}

/// Gate-2 assertion 12: breaks #1 and #3 were actually addressed rather than
/// feature-gated away — no `server_fn` crate anywhere, and no
/// `ServeConfigBuilder`.
#[test]
fn migration_no_server_fn_crate_or_serve_config_builder() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));

    // The crate Dioxus 0.7 no longer depends on. Assembled at runtime so this
    // test file is not itself a hit for `grep -r "server_fn" src/ Cargo.toml`.
    const BANNED: &str = "server_fn";

    let manifest =
        std::fs::read_to_string(root.join("Cargo.toml")).expect("Cargo.toml is readable");
    assert!(
        !manifest.contains(BANNED),
        "Cargo.toml still references the removed {BANNED} crate"
    );

    for path in rust_sources(&root.join("src")) {
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("{} is readable: {err}", path.display()));
        assert!(
            !source.contains(BANNED),
            "{} still references the removed {BANNED} crate",
            path.display()
        );
        assert!(
            !source.contains("ServeConfigBuilder"),
            "{} still uses the removed ServeConfigBuilder",
            path.display()
        );
    }
}

/// Gate-2 assertion 13: break #7 is the only *silent* one — 0.6 suppressed a
/// `form`'s native submit, 0.7 submits (and reloads the page) unless the
/// handler calls `prevent_default()`. Every `form` element in the client must
/// therefore either call it or be explicitly annotated as intentional.
#[test]
fn migration_every_client_form_is_audited_for_prevent_default() {
    let client_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/client");
    let mut audited = 0usize;

    for path in rust_sources(&client_dir) {
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("{} is readable: {err}", path.display()));
        let lines: Vec<&str> = source.lines().collect();

        for (index, line) in lines.iter().enumerate() {
            if line.trim() != "form {" && !line.trim().starts_with("form { ") {
                continue;
            }

            // Walk the rsx element's brace-balanced block.
            let mut depth = 0i32;
            let mut block = String::new();
            for candidate in &lines[index..] {
                depth += candidate.matches('{').count() as i32;
                depth -= candidate.matches('}').count() as i32;
                block.push_str(candidate);
                block.push('\n');
                if depth <= 0 {
                    break;
                }
            }

            assert!(
                block.contains("prevent_default") || block.contains("// intentional native submit"),
                "{}:{} declares a `form` that neither calls prevent_default() nor is annotated \
                 `// intentional native submit` — Dioxus 0.7 submits forms natively by default",
                path.display(),
                index + 1
            );
            audited += 1;
        }
    }

    // Recorded so the assertion's reach is visible in the test output rather
    // than silently passing on an empty set.
    println!("audited {audited} form element(s) in src/client");
}

fn rust_sources(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        let entries = std::fs::read_dir(&next)
            .unwrap_or_else(|err| panic!("{} is readable: {err}", next.display()));
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}
