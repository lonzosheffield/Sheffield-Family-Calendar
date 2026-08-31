//! **T2.6 acceptance** — the cross-surface loop test (E4):
//! `docs/reviews/PURPLE_TEAM.md` §P3 T2.6, `docs/PLAN.md` §3.
//!
//! Everything else in this repo tests one surface at a time: T2.1 is the TV,
//! T2.2 is the phone, T2.3 is the whiteboard, T1.2 is the hub protocol in the
//! abstract. This file is the one place that plays both ends of `/ws` at
//! once, tagged the way the acceptance test names them — `phone` and `tv` —
//! and walks the whole control path a real evening in this house exercises:
//! a parent's phone drives the TV, an unauthenticated phone cannot, a child's
//! scribble on the phone shows up on the TV attributed to the phone, and a
//! power-cycle of the server (the box under the stairs, not either surface)
//! is invisible to both within 30 s.
//!
//! | Acceptance assertion (`PURPLE_TEAM.md` §P3 T2.6) | Proven by |
//! | --- | --- |
//! | authed `phone` `SetView` reaches `tv` within 1 s | step 2 |
//! | unauthed `phone` `SetView` is not delivered | step 1 |
//! | `phone`'s stroke arrives at `tv` with `origin == phone` | step 3 |
//! | kill + restart the server ⇒ both resync within 30 s | step 4 |
//!
//! The harness (`connect`/`send`/`next_message`/`wait_for`/`expect_hello`) is
//! deliberately the same shape as `tests/realtime_tests.rs` — integration
//! test binaries in Rust cannot share code except through a `mod` included by
//! `#[path]`, and this file owns no production module, so it is
//! self-contained rather than reaching into another task's test file.

#![cfg(feature = "server")]

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use axum::routing::get;
use axum::Router;
use family_calendar::client::realtime::backoff;
use family_calendar::server::api::realtime;
use family_calendar::shared::types::{
    ClientId, ClientMessage, ServerMessage, Stroke, StrokePoint, View, DEFAULT_BOARD_ID,
};
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message as WsFrame;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type WsStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

// ---------------------------------------------------------------------------
// Harness (mirrors tests/realtime_tests.rs; see the module doc comment)
// ---------------------------------------------------------------------------

/// Point this test binary at its own throwaway sqlite file, isolated from
/// every other test binary's `DATABASE_URL` (mirrors
/// `tests/realtime_tests.rs::init_test_env`).
fn init_test_env() {
    static ONCE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    ONCE.get_or_init(|| {
        let base =
            std::env::temp_dir().join(format!("familyhub-loop-tests-{}", std::process::id()));
        // Windows reuses PIDs: wipe any leftover scratch dir from an earlier run first.
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("test scratch directory is creatable");
        let db_path = base.join("family.db");
        let url = format!(
            "sqlite://{}",
            db_path.display().to_string().replace('\\', "/")
        );
        std::env::set_var("DATABASE_URL", url);
        std::env::set_var("FAMILY_HUB_DATA_DIR", &base);
    });
}

/// The realtime hub on its own router — just `/ws`, the same surface both
/// the TV and the phone PWA actually speak (`docs/PROTOCOL.md` §9).
fn hub_router() -> Router {
    Router::new().route("/ws", get(realtime::ws_handler))
}

async fn spawn_hub() -> (SocketAddr, JoinHandle<()>) {
    init_test_env();
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind an ephemeral port");
    let addr = listener.local_addr().expect("listener has a local address");
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, hub_router()).await;
    });
    (addr, handle)
}

/// Serve on a *specific* address, retrying while the previous listener's port
/// is still being released — the restart step rebinds the address the
/// killed server just vacated.
async fn serve_on(addr: SocketAddr) -> JoinHandle<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match TcpListener::bind(addr).await {
            Ok(listener) => {
                return tokio::spawn(async move {
                    let _ = axum::serve(listener, hub_router()).await;
                });
            }
            Err(err) if Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(50)).await;
                let _ = err;
            }
            Err(err) => panic!("could not rebind {addr}: {err}"),
        }
    }
}

async fn connect(addr: SocketAddr) -> WsStream {
    let (socket, response) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .expect("websocket upgrade");
    assert_eq!(response.status().as_u16(), 101);
    socket
}

async fn send(socket: &mut WsStream, message: &ClientMessage) {
    let payload = serde_json::to_string(message).expect("ClientMessage serializes");
    socket
        .send(WsFrame::text(payload))
        .await
        .expect("send over the websocket");
}

fn parse(text: &str) -> ServerMessage {
    serde_json::from_str(text).unwrap_or_else(|err| panic!("invalid ServerMessage {text:?}: {err}"))
}

/// Next `ServerMessage`, or `None` if `within` elapses or the socket closes.
async fn next_message(socket: &mut WsStream, within: Duration) -> Option<ServerMessage> {
    let wait = async {
        loop {
            match socket.next().await {
                Some(Ok(WsFrame::Text(text))) => return Some(parse(&text)),
                Some(Ok(WsFrame::Close(_))) | None => return None,
                Some(Ok(_)) => continue,
                Some(Err(_)) => return None,
            }
        }
    };
    tokio::time::timeout(within, wait).await.ok().flatten()
}

/// Read until `predicate` matches, or give up after `within`.
async fn wait_for(
    socket: &mut WsStream,
    within: Duration,
    predicate: impl Fn(&ServerMessage) -> bool,
) -> Option<ServerMessage> {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match next_message(socket, remaining).await {
            Some(message) if predicate(&message) => return Some(message),
            Some(_) => continue,
            None => return None,
        }
    }
    None
}

/// Protocol v2: the server's first frame is always `Hello` with the id it
/// minted for this connection.
async fn expect_hello(socket: &mut WsStream) -> ClientId {
    match next_message(socket, Duration::from_secs(5)).await {
        Some(ServerMessage::Hello { client_id, .. }) => {
            assert!(!client_id.as_str().is_empty(), "the server mints an id");
            client_id
        }
        other => panic!("expected Hello as the first frame, got {other:?}"),
    }
}

/// A stroke tagged so the TV can be shown to have received *this* stroke.
///
/// QA round 1 **Q1-10** made `Stroke::color` a validated field on the server
/// (`realtime::valid_stroke`: `#` plus ASCII hex, at most 32 bytes), so the
/// marker rides in the colour as a 24-bit value and [`marker_of`] reads it
/// back off the wire.
fn stroke_with(marker: u32, points: usize) -> Stroke {
    Stroke {
        points: (0..points)
            .map(|i| StrokePoint {
                x: i as f64 / points as f64,
                y: (i as f64 / points as f64) * 0.5,
            })
            .collect(),
        color: format!("#{:06x}", marker & 0xff_ffff),
        width: 3.0,
    }
}

/// The marker [`stroke_with`] encoded.
fn marker_of(stroke: &Stroke) -> u32 {
    u32::from_str_radix(
        stroke
            .color
            .strip_prefix('#')
            .unwrap_or_else(|| panic!("marker colour {:?} is not #rrggbb", stroke.color)),
        16,
    )
    .unwrap_or_else(|err| panic!("marker colour {:?} is not hex: {err}", stroke.color))
}

/// The phone's stroke in `t2_6_*`.
const PHONE_STROKE: u32 = 0x0_9403;

// ---------------------------------------------------------------------------
// The loop
// ---------------------------------------------------------------------------

/// `realtime::sender()` is one process-wide broadcast channel and
/// `realtime::session` a process-wide token set, so — exactly as
/// `tests/realtime_tests.rs::hub_lock` does — a lock across the whole test
/// body keeps this test binary's other tests (if any are ever added) from
/// interleaving with it.
async fn hub_lock() -> tokio::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await
}

/// **T2.6** — the phone→TV control path end-to-end, in Rust
/// (`PURPLE_TEAM.md` §P3 T2.6): boot the server; open two Rust WS clients
/// tagged `phone` and `tv`; the phone sends `SetView{Calendar}` with a valid
/// parent session and the TV must receive it within 1 s; an unauthenticated
/// phone's `SetView` must not be delivered; the phone draws a stroke and the
/// TV receives it stamped `origin == phone`; killing and restarting the
/// server must leave both clients resynced within 30 s.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn t2_6_phone_drives_the_tv_across_a_server_restart() {
    let _guard = hub_lock().await;
    realtime::session::revoke_all();
    realtime::reset_board().await;
    let (addr, server) = spawn_hub().await;

    // Two Rust WS clients, tagged exactly the way the acceptance test names
    // them: `phone` is the parents' controller surface (D1), `tv` is the
    // kiosk display.
    let mut phone = connect(addr).await;
    let mut tv = connect(addr).await;
    let phone_id = expect_hello(&mut phone).await;
    let tv_id = expect_hello(&mut tv).await;
    assert_ne!(
        phone_id, tv_id,
        "phone and tv are distinct, server-minted connections"
    );

    // ------------------------------------------------------------------
    // 1. An unauthenticated phone's SetView is not delivered.
    // ------------------------------------------------------------------
    send(
        &mut phone,
        &ClientMessage::SetView {
            view: View::Calendar,
            auth: None,
        },
    )
    .await;
    assert!(
        next_message(&mut tv, Duration::from_secs(2))
            .await
            .is_none(),
        "an unauthenticated phone's SetView must not reach the TV"
    );

    // ------------------------------------------------------------------
    // 2. An authed SetView reaches the TV within 1 s.
    // ------------------------------------------------------------------
    let parent_session = realtime::session::issue();
    let sent_at = Instant::now();
    send(
        &mut phone,
        &ClientMessage::SetView {
            view: View::Calendar,
            auth: Some(parent_session.clone()),
        },
    )
    .await;
    let delivered = wait_for(&mut tv, Duration::from_secs(5), |message| {
        matches!(message, ServerMessage::SetView { .. })
    })
    .await;
    let elapsed = sent_at.elapsed();
    assert_eq!(
        delivered,
        Some(ServerMessage::SetView {
            view: View::Calendar
        }),
        "the parent's SetView{{Calendar}} must reach the TV"
    );
    assert!(
        elapsed < Duration::from_secs(1),
        "SetView took {elapsed:?} to reach the TV, over the 1 s budget"
    );

    // ------------------------------------------------------------------
    // 3. The phone's stroke arrives at the TV stamped origin == phone.
    // ------------------------------------------------------------------
    let stroke = stroke_with(PHONE_STROKE, 6);
    send(
        &mut phone,
        &ClientMessage::Draw {
            board_id: DEFAULT_BOARD_ID,
            stroke: stroke.clone(),
        },
    )
    .await;
    let received = wait_for(&mut tv, Duration::from_secs(5), |message| {
        matches!(message, ServerMessage::Draw { .. })
    })
    .await
    .expect("the TV receives the phone's stroke");
    match received {
        ServerMessage::Draw {
            origin, stroke: s, ..
        } => {
            assert_eq!(
                origin, phone_id,
                "the stroke the TV receives must be stamped with the phone's ClientId"
            );
            assert_eq!(
                marker_of(&s),
                PHONE_STROKE,
                "the stroke content round-trips"
            );
        }
        other => panic!("expected Draw, got {other:?}"),
    }

    // ------------------------------------------------------------------
    // 4. Kill and restart the server; both surfaces resync within 30 s.
    // ------------------------------------------------------------------
    server.abort();
    let _ = server.await;
    // Drain whatever the dying socket still has queued, then let it go —
    // both real clients would see their sockets die the same way.
    while next_message(&mut phone, Duration::from_secs(2))
        .await
        .is_some()
    {}
    while next_message(&mut tv, Duration::from_secs(2))
        .await
        .is_some()
    {}
    drop(phone);
    drop(tv);

    let restart_at = Instant::now();
    let restarted = serve_on(addr).await;

    // Both surfaces run the same reconnect supervisor
    // (`client::realtime::pump`, driven by `backoff`); reproduce it for each
    // tagged client independently, exactly as a phone and a TV recovering
    // from the same power cut would — neither waits on the other.
    async fn reconnect_and_snapshot(addr: SocketAddr, deadline: Instant) -> ServerMessage {
        let mut attempt = 0u32;
        loop {
            assert!(
                Instant::now() < deadline,
                "reconnect + Snapshot exceeded the 30 s budget"
            );
            match tokio_tungstenite::connect_async(format!("ws://{addr}/ws")).await {
                Ok((mut socket, _)) => {
                    expect_hello(&mut socket).await;
                    send(
                        &mut socket,
                        &ClientMessage::RequestSnapshot {
                            board_id: DEFAULT_BOARD_ID,
                            since_seq: 0,
                        },
                    )
                    .await;
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if let Some(snapshot) = wait_for(&mut socket, remaining, |m| {
                        matches!(m, ServerMessage::Snapshot { .. })
                    })
                    .await
                    {
                        return snapshot;
                    }
                }
                Err(_) => {
                    tokio::time::sleep(backoff(attempt)).await;
                    attempt = attempt.saturating_add(1);
                }
            }
        }
    }

    let deadline = restart_at + Duration::from_secs(30);
    let (phone_snapshot, tv_snapshot) = tokio::join!(
        reconnect_and_snapshot(addr, deadline),
        reconnect_and_snapshot(addr, deadline),
    );

    let elapsed = restart_at.elapsed();
    assert!(
        elapsed < Duration::from_secs(30),
        "both surfaces took {elapsed:?} to resync after the restart, over the 30 s budget"
    );

    for (tag, snapshot) in [("phone", phone_snapshot), ("tv", tv_snapshot)] {
        match snapshot {
            ServerMessage::Snapshot {
                board_id, strokes, ..
            } => {
                assert_eq!(board_id, DEFAULT_BOARD_ID);
                assert!(
                    strokes.iter().any(|s| marker_of(s) == PHONE_STROKE),
                    "{tag}'s post-restart Snapshot must still carry the stroke drawn before the restart"
                );
            }
            other => panic!("{tag}: expected Snapshot, got {other:?}"),
        }
    }

    realtime::session::revoke(&parent_session);
    realtime::session::revoke_all();
    restarted.abort();
}
