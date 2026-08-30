//! **T2.3 acceptance suite** — whiteboard v2, `docs/reviews/PURPLE_TEAM.md`
//! §P3:
//!
//! | # | Assertion | Test |
//! | --- | --- | --- |
//! | a | 500 strokes persist; a fresh WS connection's `Snapshot` replays them in `seq` order | [`t2_3_a_five_hundred_strokes_persist_and_replay_in_seq_order`] |
//! | b | `ClearBoard` moves the `cleared_at` watermark; the next `Snapshot` is empty; compaction removes the rows | [`t2_3_b_clear_moves_the_watermark_then_compaction_removes_the_rows`] |
//! | c | Undo removes exactly the calling client's own last stroke, never another's | [`t2_3_c_undo_removes_only_the_callers_own_last_stroke`] |
//!
//! (d) "50 queued `Draw`s between two render ticks are all drawn" and (e)
//! "a resize triggers a repaint-from-log" are client-side unit tests that
//! live inline in `src/client/components/whiteboard.rs`'s own `#[cfg(test)]`
//! module instead — they exercise that file's private `canvas` stub and
//! construct a bare `RealtimeBus`, neither of which an integration test can
//! reach.
//!
//! Follows the harness pattern `tests/realtime_tests.rs` (T1.2) and
//! `tests/profiles_tests.rs` (T1.4) already established: an in-process `/ws`
//! router on an ephemeral port, driven by real `tokio-tungstenite` clients,
//! and every `#[server]` fn (here, `api::whiteboard::undo_last_stroke`)
//! called directly rather than over HTTP — the real server-side
//! implementation running in-process, per `tests/profiles_tests.rs`'s own
//! doc comment.

#![cfg(feature = "server")]

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use axum::routing::get;
use axum::Router;
use family_calendar::server::api::{realtime, whiteboard};
use family_calendar::server::db;
use family_calendar::shared::types::{
    ClientId, ClientMessage, ServerMessage, Stroke, StrokePoint, DEFAULT_BOARD_ID,
};
use futures_util::{SinkExt, StreamExt};
use sqlx::Row;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message as WsFrame;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type WsStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// Serialises every test in this binary: `realtime::sender()`, the in-process
/// `seq` counter and the database are all process-wide, exactly the reason
/// `tests/realtime_tests.rs::hub_lock` exists.
async fn hub_lock() -> tokio::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await
}

/// One throwaway sqlite file for this test binary's process, isolated from
/// every other test binary's `DATABASE_URL` (mirrors
/// `tests/profiles_tests.rs::init_test_env`).
fn init_test_env() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let base =
            std::env::temp_dir().join(format!("familyhub-whiteboard-tests-{}", std::process::id()));
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

async fn expect_hello(socket: &mut WsStream) -> ClientId {
    match next_message(socket, Duration::from_secs(5)).await {
        Some(ServerMessage::Hello { client_id, .. }) => client_id,
        other => panic!("expected Hello as the first frame, got {other:?}"),
    }
}

/// A stroke tagged so a `Snapshot` can be checked for identity and order.
///
/// QA round 1 **Q1-10** made `Stroke::color` a validated field on the server
/// (`realtime::valid_stroke`: `#` plus ASCII hex, at most 32 bytes), so the
/// marker rides in the colour as a 24-bit value and [`marker_of`] reads it
/// back off the wire.
fn stroke_marked(marker: u32) -> Stroke {
    Stroke {
        points: vec![
            StrokePoint { x: 0.1, y: 0.1 },
            StrokePoint { x: 0.2, y: 0.2 },
        ],
        color: format!("#{:06x}", marker & 0xff_ffff),
        width: 3.0,
    }
}

/// The marker [`stroke_marked`] encoded.
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

/// Row count straight from the table, bypassing the server API — proves
/// persistence (a) and compaction (b) against the database itself, not just
/// what one connection's `Snapshot` happens to report.
async fn stroke_row_count(board_id: i64, only_live: bool) -> i64 {
    let pool = db::pool().await.expect("test db pool");
    let sql = if only_live {
        "SELECT COUNT(*) FROM whiteboard_strokes WHERE board_id = ?1 AND cleared_at IS NULL"
    } else {
        "SELECT COUNT(*) FROM whiteboard_strokes WHERE board_id = ?1"
    };
    let row = sqlx::query(sql)
        .bind(board_id)
        .fetch_one(pool)
        .await
        .expect("count query");
    row.try_get::<i64, _>(0).expect("count column")
}

/// Poll until `stroke_row_count` reaches `expected` or `within` elapses.
///
/// `record_stroke`'s write-behind design (`server::api::realtime`'s module
/// doc comment) hands a `Draw`'s row to the single ordered persistence task
/// so publish never waits on the write connection — a client that has
/// already seen the broadcast echo may briefly be ahead of what has actually
/// committed (QA round 1, Q1-09: the batch still commits in `seq` order).
async fn wait_for_row_count(board_id: i64, expected: i64, within: Duration) -> i64 {
    let deadline = Instant::now() + within;
    loop {
        let count = stroke_row_count(board_id, false).await;
        if count >= expected || Instant::now() >= deadline {
            return count;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

// ---------------------------------------------------------------------------
// (a) persistence + seq-ordered replay
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn t2_3_a_five_hundred_strokes_persist_and_replay_in_seq_order() {
    let _guard = hub_lock().await;
    realtime::reset_board().await;
    let (addr, server) = spawn_hub().await;

    let writer = connect(addr).await;
    let (mut sink, mut stream) = writer.split();
    match stream.next().await {
        Some(Ok(WsFrame::Text(text))) => assert!(
            matches!(parse(&text), ServerMessage::Hello { .. }),
            "first frame must be Hello"
        ),
        other => panic!("expected Hello, got {other:?}"),
    }

    const TOTAL: usize = 500;

    // Read concurrently with sending: draining this connection's own 500
    // echoes proves the server minted a `seq` (and queued the persistence
    // write) for each.
    let reader = tokio::spawn(async move {
        let mut seen = 0usize;
        while seen < TOTAL {
            match tokio::time::timeout(Duration::from_secs(30), stream.next()).await {
                Ok(Some(Ok(WsFrame::Text(text)))) => {
                    if matches!(parse(&text), ServerMessage::Draw { .. }) {
                        seen += 1;
                    }
                }
                Ok(Some(Ok(_))) => continue,
                _ => break,
            }
        }
        seen
    });

    // Client::realtime's own `StrokeBatcher` caps a real client at one flush
    // per 34 ms (≤ 30 msg/s), comfortably inside the server's 40 msg/s
    // token bucket (`realtime::RATE_LIMIT_PER_SECOND`) — sending all 500 back
    // to back instead would trip that limiter and get *this* connection
    // resynced-and-closed after ~3 over-budget seconds (§P2c assertion 9),
    // which is a correctness feature being exercised, not a bug to work
    // around by disabling it. Pace to match the real client instead.
    let mut ticker = tokio::time::interval(Duration::from_millis(34));
    for i in 0..TOTAL {
        ticker.tick().await;
        let payload = serde_json::to_string(&ClientMessage::Draw {
            board_id: DEFAULT_BOARD_ID,
            stroke: stroke_marked(i as u32),
        })
        .expect("ClientMessage serializes");
        sink.send(WsFrame::text(payload))
            .await
            .expect("send over the websocket");
    }

    let seen = tokio::time::timeout(Duration::from_secs(35), reader)
        .await
        .expect("reader task did not hang")
        .expect("reader task did not panic");
    assert_eq!(seen, TOTAL, "every Draw sent must echo back");

    let on_disk = wait_for_row_count(DEFAULT_BOARD_ID, TOTAL as i64, Duration::from_secs(10)).await;
    assert_eq!(
        on_disk, TOTAL as i64,
        "every stroke must be persisted, not merely broadcast"
    );

    let mut fresh = connect(addr).await;
    expect_hello(&mut fresh).await;
    send(
        &mut fresh,
        &ClientMessage::RequestSnapshot {
            board_id: DEFAULT_BOARD_ID,
            since_seq: 0,
        },
    )
    .await;
    let snapshot = wait_for(&mut fresh, Duration::from_secs(10), |m| {
        matches!(m, ServerMessage::Snapshot { .. })
    })
    .await
    .expect("a fresh connection receives a Snapshot");

    match snapshot {
        ServerMessage::Snapshot {
            board_id, strokes, ..
        } => {
            assert_eq!(board_id, DEFAULT_BOARD_ID);
            assert_eq!(
                strokes.len(),
                TOTAL,
                "the snapshot must replay all 500 persisted strokes"
            );
            let order: Vec<u32> = strokes.iter().map(marker_of).collect();
            let expected: Vec<u32> = (0..TOTAL as u32).collect();
            assert_eq!(order, expected, "strokes must replay in seq order");
        }
        other => panic!("expected Snapshot, got {other:?}"),
    }

    server.abort();
}

// ---------------------------------------------------------------------------
// (b) clear watermark + compaction
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn t2_3_b_clear_moves_the_watermark_then_compaction_removes_the_rows() {
    let _guard = hub_lock().await;
    realtime::reset_board().await;
    let (addr, server) = spawn_hub().await;

    let mut writer = connect(addr).await;
    expect_hello(&mut writer).await;

    const DRAWN: usize = 5;
    for i in 0..DRAWN {
        send(
            &mut writer,
            &ClientMessage::Draw {
                board_id: DEFAULT_BOARD_ID,
                stroke: stroke_marked(i as u32),
            },
        )
        .await;
    }
    let mut seen = 0usize;
    while seen < DRAWN {
        if let Some(ServerMessage::Draw { .. }) =
            next_message(&mut writer, Duration::from_secs(10)).await
        {
            seen += 1;
        }
    }
    wait_for_row_count(DEFAULT_BOARD_ID, DRAWN as i64, Duration::from_secs(5)).await;

    send(
        &mut writer,
        &ClientMessage::ClearBoard {
            board_id: DEFAULT_BOARD_ID,
        },
    )
    .await;
    wait_for(&mut writer, Duration::from_secs(5), |m| {
        matches!(m, ServerMessage::BoardCleared { .. })
    })
    .await
    .expect("ClearBoard is acknowledged");

    // The very next Snapshot — even on the same connection — is empty.
    send(
        &mut writer,
        &ClientMessage::RequestSnapshot {
            board_id: DEFAULT_BOARD_ID,
            since_seq: 0,
        },
    )
    .await;
    let snapshot = wait_for(&mut writer, Duration::from_secs(5), |m| {
        matches!(m, ServerMessage::Snapshot { .. })
    })
    .await
    .expect("snapshot after clear");
    match snapshot {
        ServerMessage::Snapshot { strokes, .. } => {
            assert!(strokes.is_empty(), "a cleared board must replay empty")
        }
        other => panic!("expected Snapshot, got {other:?}"),
    }

    // The rows are soft-cleared, not gone yet.
    assert_eq!(
        stroke_row_count(DEFAULT_BOARD_ID, false).await,
        DRAWN as i64,
        "cleared rows must survive on disk until compaction"
    );
    assert_eq!(
        stroke_row_count(DEFAULT_BOARD_ID, true).await,
        0,
        "no *live* row may remain after a clear"
    );

    let removed = realtime::compact_board(DEFAULT_BOARD_ID)
        .await
        .expect("compaction");
    assert_eq!(removed, DRAWN as u64);
    assert_eq!(
        stroke_row_count(DEFAULT_BOARD_ID, false).await,
        0,
        "compaction must hard-delete the cleared rows"
    );

    server.abort();
}

// ---------------------------------------------------------------------------
// (c) undo-own-last-stroke
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn t2_3_c_undo_removes_only_the_callers_own_last_stroke() {
    let _guard = hub_lock().await;
    realtime::reset_board().await;
    let (addr, server) = spawn_hub().await;

    let mut a = connect(addr).await;
    let mut b = connect(addr).await;
    let a_id = expect_hello(&mut a).await;
    let b_id = expect_hello(&mut b).await;

    send(
        &mut a,
        &ClientMessage::Draw {
            board_id: DEFAULT_BOARD_ID,
            stroke: stroke_marked(0xa1),
        },
    )
    .await;
    wait_for(&mut a, Duration::from_secs(5), |m| {
        matches!(m, ServerMessage::Draw { .. })
    })
    .await
    .expect("A's stroke echoes to A");
    wait_for(&mut b, Duration::from_secs(5), |m| {
        matches!(m, ServerMessage::Draw { .. })
    })
    .await
    .expect("A's stroke reaches B too");

    send(
        &mut b,
        &ClientMessage::Draw {
            board_id: DEFAULT_BOARD_ID,
            stroke: stroke_marked(0xb1),
        },
    )
    .await;
    wait_for(&mut a, Duration::from_secs(5), |m| {
        matches!(m, ServerMessage::Draw { .. })
    })
    .await
    .expect("B's stroke reaches A");
    wait_for(&mut b, Duration::from_secs(5), |m| {
        matches!(m, ServerMessage::Draw { .. })
    })
    .await
    .expect("B's stroke echoes to B");

    wait_for_row_count(DEFAULT_BOARD_ID, 2, Duration::from_secs(5)).await;

    // A client with nothing on the board has nothing to undo — not an error.
    let nothing = whiteboard::undo_last_stroke("nobody-drew-anything".to_string())
        .await
        .expect("undo call for an unknown client succeeds");
    assert_eq!(nothing, None);

    let removed = whiteboard::undo_last_stroke(a_id.as_str().to_string())
        .await
        .expect("undo call");
    assert!(removed.is_some(), "A has exactly one stroke to undo");

    let mut fresh = connect(addr).await;
    expect_hello(&mut fresh).await;
    send(
        &mut fresh,
        &ClientMessage::RequestSnapshot {
            board_id: DEFAULT_BOARD_ID,
            since_seq: 0,
        },
    )
    .await;
    let snapshot = wait_for(&mut fresh, Duration::from_secs(5), |m| {
        matches!(m, ServerMessage::Snapshot { .. })
    })
    .await
    .expect("snapshot after undo");
    match snapshot {
        ServerMessage::Snapshot { strokes, .. } => {
            assert_eq!(strokes.len(), 1, "only B's stroke may remain");
            assert_eq!(
                marker_of(&strokes[0]),
                0xb1,
                "undo must remove A's own stroke, never B's"
            );
        }
        other => panic!("expected Snapshot, got {other:?}"),
    }

    // A has nothing left; undoing again is a no-op, not an error, and B's
    // stroke is still untouched.
    let again = whiteboard::undo_last_stroke(a_id.as_str().to_string())
        .await
        .expect("undo call");
    assert_eq!(again, None, "A has nothing left to undo");
    assert_eq!(stroke_row_count(DEFAULT_BOARD_ID, true).await, 1);

    let _ = b_id;
    server.abort();
}
