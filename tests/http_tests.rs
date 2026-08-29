//! T0.3 acceptance test — HTTP/WS integration harness on the current Dioxus
//! 0.6.3 code (`docs/reviews/PURPLE_TEAM.md` §P3, row T0.3).
//!
//! Boots the real production router (the exact routes `main.rs` registers:
//! `/ws`, `/uploads`, and the Dioxus fullstack SSR + server-fn fallback) on an
//! ephemeral port for every test, so this is a true in-process integration
//! harness rather than a set of unit tests against handler functions.

#![cfg(feature = "server")]

use std::net::SocketAddr;
use std::time::Duration;

use dioxus::prelude::*;
use family_calendar::client::app::App;
use family_calendar::server::api::realtime;
use family_calendar::server::api::Today;
use family_calendar::server::db;
use family_calendar::shared::types::{StrokePoint, StrokeSegment, WsMessage};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message as WsClientMessage;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type WsStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

const TEST_INDEX_HTML: &str =
    "<!DOCTYPE html><html><head><title>Family Hub</title></head><body><div id=\"main\"></div></body></html>";

/// Point every test at one throwaway sqlite file instead of the real
/// `family.db` (or an on-disk-per-connection `:memory:`, which would give
/// each pooled connection its own empty database). Idempotent, and shared by
/// every test in this binary, since `db::pool()` is a process-wide `OnceCell`.
fn init_test_database_url() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let path =
            std::env::temp_dir().join(format!("familyhub-http-tests-{}.db", std::process::id()));
        let url = format!("sqlite://{}", path.display().to_string().replace('\\', "/"));
        std::env::set_var("DATABASE_URL", url);
    });
}

/// Boot the same router `main.rs` serves, on an OS-assigned free port, and
/// return its address. The listener (and the task serving it) is dropped
/// when the per-test tokio runtime shuts down at the end of the test, so nothing
/// leaks across runs.
async fn spawn_test_server() -> SocketAddr {
    init_test_database_url();
    // Warm the pool up front so a background `use_resource` fetch triggered
    // during SSR has somewhere to go, mirroring what `main.rs` does before it
    // starts serving.
    db::pool().await.expect("test sqlite pool opens");

    let cfg = ServeConfigBuilder::new()
        .index_html(TEST_INDEX_HTML.to_string())
        .build()
        .expect("in-memory index.html builds into a ServeConfig");

    let router = axum::Router::new()
        .route("/ws", axum::routing::get(realtime::ws_handler))
        .nest_service(
            "/uploads",
            tower_http::services::ServeDir::new(db::UPLOAD_DIR),
        )
        .serve_dioxus_application(cfg, App);

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
        .timeout(Duration::from_secs(5))
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

#[tokio::test]
async fn http_root_serves_dashboard_with_panel_markers() {
    let addr = spawn_test_server().await;

    let response = http_client()
        .get(format!("http://{addr}/"))
        .header("accept", "text/html")
        .send()
        .await
        .expect("GET / should respond");

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

/// The one required "server-fn round trip": a real HTTP POST to the
/// `today()` server function's wire endpoint, decoded exactly the way a
/// browser client would decode it — not an in-process Rust call, which would
/// just run the function body directly.
#[tokio::test]
async fn http_today_server_fn_round_trip() {
    let addr = spawn_test_server().await;
    let path = <Today as server_fn::ServerFn>::PATH;

    let response = http_client()
        .post(format!("http://{addr}{path}"))
        .header("content-type", "application/x-www-form-urlencoded")
        .body("")
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

#[tokio::test]
async fn ws_stroke_from_one_client_fans_out_to_second_client() {
    let addr = spawn_test_server().await;
    let url = format!("ws://{addr}/ws");

    let (mut client_a, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("client A upgrades to a websocket");
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
