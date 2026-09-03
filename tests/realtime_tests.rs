//! **T1.2 acceptance suite** — the nine points of
//! `docs/reviews/PURPLE_TEAM.md` §P2c "T1.2 acceptance", plus the
//! `docs/PROTOCOL.md` completeness assertion.
//!
//! | # | Assertion | Test |
//! | --- | --- | --- |
//! | 1 | `backoff(0..10)` matches the schedule; jitter within ±20 % | `t1_2_1_*` |
//! | 2 | Lag ⇒ `Resync`, socket stays open, fast client gets all 5,000 | `t1_2_2_*` |
//! | 3 | 8 × 30 msg/s × 30 s: zero closes, p99 < 250 ms, RSS +< 50 MB | `t1_2_3_*` |
//! | 4 | Echo carries `origin`; the sender skips its own | `t1_2_4_*` |
//! | 5 | Spoofed `CalendarUpdated` reaches nobody | `t1_2_5_*` |
//! | 6 | `SetView` needs a parent session | `t1_2_6_*` |
//! | 7 | Kill + restart ⇒ reconnect with `Snapshot` in < 30 s | `t1_2_7_*` |
//! | 8 | Midnight tick on both DST dates for NY and London | `t1_2_8_*` |
//! | 9 | A 200 msg/s client is throttled, resynced and closed — alone | `t1_2_9_*` |
//! | doc | `docs/PROTOCOL.md` names every protocol variant | `t1_2_protocol_doc_*` |
//!
//! QA round 1 added two more, from `docs/qa/QA_ROUND_1.md`:
//!
//! | # | Assertion | Test |
//! | --- | --- | --- |
//! | Q1-10 | A 1 MiB frame closes only the sender; an invalid stroke is dropped | `qa1_10_*` |
//! | Q1-13 | A connected client receives `Health` within 2× the interval | `qa1_13_*` |
//!
//! `realtime::sender()` is one process-wide broadcast channel, so every test
//! that counts frames takes [`hub_lock`] first.

#![cfg(feature = "server")]

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use axum::routing::get;
use axum::Router;
use family_calendar::client::realtime::{backoff, backoff_base, is_own_echo, BACKOFF_JITTER};
use family_calendar::server::api::realtime;
use family_calendar::shared::types::{
    ClientId, ClientMessage, ResyncReason, ServerMessage, Stroke, StrokePoint, View,
    DEFAULT_BOARD_ID, PROTOCOL_VERSION,
};
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message as WsFrame;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type WsStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// Serialises the tests that count frames on the shared broadcast channel.
///
/// An async-aware mutex, because the guard is deliberately held across the
/// whole test body: `realtime::sender()` is one process-wide channel and two
/// concurrent tests would see each other's frames.
async fn hub_lock() -> tokio::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await
}

/// The realtime hub on its own router — just `/ws`, no SSR.
///
/// **T2.3** swapped `record_stroke`/`clear_board`/`snapshot` for rows in
/// `whiteboard_strokes` (`docs/HANDOFF.md` H-10, the seam this task's own
/// doc comment reserved), so the hub now touches a real SQLite database —
/// [`init_test_env`] points this test binary's process at its own throwaway
/// file, the same `DATABASE_URL` convention every other integration test
/// binary already uses (`tests/profiles_tests.rs::init_test_env`,
/// `tests/http_tests.rs::init_test_env`), so it can never collide with
/// another test binary's data.
fn hub_router() -> Router {
    Router::new().route("/ws", get(realtime::ws_handler))
}

/// Point every test in this binary at one throwaway sqlite file, isolated
/// from every other test binary's `DATABASE_URL` (mirrors
/// `tests/profiles_tests.rs::init_test_env`).
fn init_test_env() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        let base =
            std::env::temp_dir().join(format!("familyhub-realtime-tests-{}", std::process::id()));
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
/// is still being released (the reconnect test rebinds the same port).
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

async fn send_raw(socket: &mut WsStream, payload: &str) {
    socket
        .send(WsFrame::text(payload.to_string()))
        .await
        .expect("send raw text over the websocket");
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
        Some(ServerMessage::Hello {
            client_id,
            protocol,
            ..
        }) => {
            assert_eq!(protocol, PROTOCOL_VERSION);
            assert!(!client_id.as_str().is_empty(), "the server mints an id");
            client_id
        }
        other => panic!("expected Hello as the first frame, got {other:?}"),
    }
}

/// A stroke tagged so the test can recognise it again on the far side of the
/// hub.
///
/// QA round 1 **Q1-10** made `Stroke::color` a validated field — `#` plus
/// ASCII hex, at most 32 bytes — so the marker can no longer be an arbitrary
/// English string. It is carried as a 24-bit value in the colour instead, and
/// [`marker_of`] decodes it back.
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

/// The load test's per-message stroke, carrying its correlation `key`.
///
/// The key used to ride in `points[0].x` as a bare `f64` counter. QA round 1
/// **Q1-10** made the coordinates a validated `0.0..=1.0` — a stroke that far
/// off the canvas is exactly what the hub now refuses — so the key moved into
/// the colour, where `#` plus 8 hex digits is a legal 9 bytes of the 32 the
/// validator allows. The load itself is unchanged: same message count, same
/// rate, same per-message timing.
fn load_stroke(key: u64) -> Stroke {
    Stroke {
        points: vec![
            StrokePoint { x: 0.25, y: 0.0 },
            StrokePoint { x: 0.5, y: 0.5 },
        ],
        color: format!("#{key:08x}"),
        width: 4.0,
    }
}

/// The key [`load_stroke`] encoded, read back off the wire.
fn load_key_of(stroke: &Stroke) -> u64 {
    stroke
        .color
        .strip_prefix('#')
        .and_then(|hex| u64::from_str_radix(hex, 16).ok())
        .unwrap_or_else(|| panic!("load key colour {:?} is not #hex", stroke.color))
}

/// The marker [`stroke_with`] encoded, read back off the wire.
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

/// Working-set size of this process, straight from the Win32 API — no crate,
/// no non-Rust component (`docs/NON_RUST.md` is unchanged by T1.2).
#[cfg(windows)]
fn working_set_bytes() -> Option<u64> {
    #[repr(C)]
    #[derive(Default)]
    struct ProcessMemoryCounters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn K32GetProcessMemoryInfo(
            process: isize,
            counters: *mut ProcessMemoryCounters,
            cb: u32,
        ) -> i32;
    }

    let mut counters = ProcessMemoryCounters {
        cb: std::mem::size_of::<ProcessMemoryCounters>() as u32,
        ..Default::default()
    };
    // SAFETY: `counters` is a correctly sized, correctly aligned
    // PROCESS_MEMORY_COUNTERS and `cb` is its true size in bytes.
    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) };
    (ok != 0).then_some(counters.working_set_size as u64)
}

#[cfg(not(windows))]
fn working_set_bytes() -> Option<u64> {
    None
}

// ---------------------------------------------------------------------------
// 1 — reconnect backoff
// ---------------------------------------------------------------------------

#[test]
fn t1_2_1_backoff_matches_the_documented_schedule() {
    let expected = [1u64, 2, 4, 8, 15, 30, 30, 30, 30, 30];
    for (attempt, seconds) in expected.iter().enumerate() {
        assert_eq!(
            backoff_base(attempt as u32),
            Duration::from_secs(*seconds),
            "backoff_base({attempt})"
        );
    }
}

#[test]
fn t1_2_1_backoff_jitter_stays_within_twenty_percent() {
    let mut distinct = HashSet::new();
    for attempt in 0..10u32 {
        let base = backoff_base(attempt).as_secs_f64();
        for _ in 0..500 {
            let jittered = backoff(attempt).as_secs_f64();
            assert!(
                jittered >= base * (1.0 - BACKOFF_JITTER) - 1e-9
                    && jittered <= base * (1.0 + BACKOFF_JITTER) + 1e-9,
                "attempt {attempt}: {jittered}s is outside ±20 % of {base}s"
            );
            distinct.insert(jittered.to_bits());
        }
    }
    assert!(
        distinct.len() > 100,
        "the jitter must actually vary; saw {} distinct delays",
        distinct.len()
    );
}

// ---------------------------------------------------------------------------
// 2 — lag: Resync, never close
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn t1_2_2_a_lagging_client_is_resynced_and_the_socket_stays_open() {
    let _guard = hub_lock().await;
    let (addr, server) = spawn_hub().await;

    let mut fast = connect(addr).await;
    let mut slow = connect(addr).await;
    expect_hello(&mut fast).await;
    expect_hello(&mut slow).await;

    // `fast` drains continuously; `slow` reads nothing at all until the
    // producer has finished, so its TCP buffers, then its 256-frame outbound
    // queue, fill up.
    let seen = Arc::new(Mutex::new(HashSet::<i64>::new()));
    let fast_seen = Arc::clone(&seen);
    let fast_reader = tokio::spawn(async move {
        while let Some(message) = next_message(&mut fast, Duration::from_secs(20)).await {
            if let ServerMessage::Draw { seq, .. } = message {
                fast_seen.lock().expect("lock").insert(seq);
            }
        }
        fast
    });

    const TOTAL: i64 = 5_000;
    let payload = stroke_with(0x1a6_7e5, 40);
    for seq in 1..=TOTAL {
        realtime::publish(&ServerMessage::Draw {
            board_id: DEFAULT_BOARD_ID,
            seq,
            origin: ClientId("producer".into()),
            stroke: payload.clone(),
        });
        // ~1,000 messages/second: fast enough to bury a client that never
        // reads, slow enough that a client which does read keeps up.
        tokio::time::sleep(Duration::from_millis(1)).await;
    }

    let resync = wait_for(&mut slow, Duration::from_secs(60), |message| {
        matches!(
            message,
            ServerMessage::Resync {
                reason: ResyncReason::Lagged
            }
        )
    })
    .await;
    assert!(
        resync.is_some(),
        "the lagging client must be sent a Resync, not disconnected"
    );

    // …and the socket is still open: it answers a Ping.
    send(&mut slow, &ClientMessage::Ping { nonce: 42 }).await;
    let pong = wait_for(&mut slow, Duration::from_secs(10), |message| {
        matches!(message, ServerMessage::Pong { nonce: 42 })
    })
    .await;
    assert!(
        pong.is_some(),
        "the lagging client's socket must stay open after a Resync (G20)"
    );

    // Let the fast reader drain, then stop it.
    tokio::time::sleep(Duration::from_secs(1)).await;
    fast_reader.abort();
    let received = seen.lock().expect("lock").len();
    assert_eq!(
        received, TOTAL as usize,
        "the fast client must receive all {TOTAL} messages, saw {received}"
    );

    server.abort();
}

// ---------------------------------------------------------------------------
// 3 — sustained load
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn t1_2_3_eight_clients_at_thirty_messages_per_second_for_thirty_seconds() {
    let _guard = hub_lock().await;
    realtime::reset_board().await;
    let (addr, server) = spawn_hub().await;

    const CLIENTS: u64 = 8;
    const PER_SECOND: u64 = 30;
    const SECONDS: u64 = 30;
    const PER_CLIENT: u64 = PER_SECOND * SECONDS;

    let baseline_rss = working_set_bytes();

    // Send times, keyed by the id encoded in the stroke's first x coordinate.
    let sent_at: Arc<Mutex<HashMap<u64, Instant>>> = Arc::new(Mutex::new(HashMap::new()));
    let latencies: Arc<Mutex<Vec<Duration>>> = Arc::new(Mutex::new(Vec::new()));
    let closed = Arc::new(AtomicBool::new(false));
    let pongs = Arc::new(AtomicU64::new(0));

    let mut writers = Vec::new();
    let mut readers = Vec::new();

    for client in 0..CLIENTS {
        let mut socket = connect(addr).await;
        expect_hello(&mut socket).await;
        let (write, mut read) = socket.split();

        let reader_latencies = Arc::clone(&latencies);
        let reader_sent_at = Arc::clone(&sent_at);
        let reader_closed = Arc::clone(&closed);
        let reader_pongs = Arc::clone(&pongs);
        readers.push(tokio::spawn(async move {
            loop {
                match read.next().await {
                    Some(Ok(WsFrame::Text(text))) => match parse(&text) {
                        ServerMessage::Draw { stroke, .. } => {
                            // The per-message correlation key rides in the
                            // colour: QA round 1 Q1-10 made the coordinates a
                            // validated `0.0..=1.0`, so they can no longer
                            // carry one. `#` + 8 hex digits is inside
                            // `realtime::MAX_STROKE_COLOR_LEN`.
                            let key = load_key_of(&stroke);
                            let start = reader_sent_at.lock().expect("lock").get(&key).copied();
                            if let Some(start) = start {
                                reader_latencies.lock().expect("lock").push(start.elapsed());
                            }
                        }
                        ServerMessage::Pong { .. } => {
                            reader_pongs.fetch_add(1, Ordering::Relaxed);
                        }
                        _ => {}
                    },
                    Some(Ok(_)) => continue,
                    Some(Err(_)) | None => {
                        reader_closed.store(true, Ordering::Relaxed);
                        return;
                    }
                }
            }
        }));

        let writer_sent_at = Arc::clone(&sent_at);
        writers.push(tokio::spawn(async move {
            let mut write = write;
            let mut ticker = tokio::time::interval(Duration::from_micros(1_000_000 / PER_SECOND));
            for n in 0..PER_CLIENT {
                ticker.tick().await;
                let key = client * 1_000_000 + n + 1;
                let stroke = load_stroke(key);
                writer_sent_at
                    .lock()
                    .expect("lock")
                    .insert(key, Instant::now());
                let payload = serde_json::to_string(&ClientMessage::Draw {
                    board_id: DEFAULT_BOARD_ID,
                    stroke,
                })
                .expect("serializes");
                if write.send(WsFrame::text(payload)).await.is_err() {
                    return None;
                }
            }
            // Liveness probe: the socket must still work after the load.
            let ping =
                serde_json::to_string(&ClientMessage::Ping { nonce: 7 }).expect("serializes");
            if write.send(WsFrame::text(ping)).await.is_err() {
                return None;
            }
            Some(write)
        }));
    }

    let mut alive = 0;
    for writer in writers {
        if writer.await.expect("writer task").is_some() {
            alive += 1;
        }
    }
    assert_eq!(
        alive, CLIENTS as usize,
        "every client's send half must survive the load"
    );

    // Give the last frames time to land.
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(
        !closed.load(Ordering::Relaxed),
        "no socket may be closed by the server under load (G20)"
    );
    assert_eq!(
        pongs.load(Ordering::Relaxed),
        CLIENTS,
        "every client must still be answered after 30 s of load"
    );

    for reader in readers {
        reader.abort();
    }

    let mut samples = latencies.lock().expect("lock").clone();
    assert!(
        samples.len() as u64 >= CLIENTS * PER_CLIENT,
        "expected at least {} fan-out samples, got {}",
        CLIENTS * PER_CLIENT,
        samples.len()
    );
    samples.sort_unstable();
    let p99 = samples[(samples.len() as f64 * 0.99) as usize - 1];
    assert!(
        p99 < Duration::from_millis(250),
        "p99 broadcast latency was {p99:?}, above the 250 ms budget"
    );

    if let (Some(before), Some(after)) = (baseline_rss, working_set_bytes()) {
        let growth = after.saturating_sub(before);
        assert!(
            growth < 50 * 1024 * 1024,
            "RSS grew by {} MiB over the load test, above the 50 MiB budget",
            growth / (1024 * 1024)
        );
    }

    server.abort();
}

// ---------------------------------------------------------------------------
// 4 — echo carries the origin
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn t1_2_4_a_draw_is_echoed_to_both_clients_stamped_with_the_sender() {
    let _guard = hub_lock().await;
    realtime::reset_board().await;
    let (addr, server) = spawn_hub().await;

    let mut a = connect(addr).await;
    let mut b = connect(addr).await;
    let a_id = expect_hello(&mut a).await;
    let b_id = expect_hello(&mut b).await;
    assert_ne!(a_id, b_id, "each connection gets its own server-minted id");

    let stroke = stroke_with(0x0ec_407, 4);
    send(
        &mut a,
        &ClientMessage::Draw {
            board_id: DEFAULT_BOARD_ID,
            stroke: stroke.clone(),
        },
    )
    .await;

    let is_draw = |message: &ServerMessage| matches!(message, ServerMessage::Draw { .. });
    let to_a = wait_for(&mut a, Duration::from_secs(5), is_draw)
        .await
        .expect("A receives its own Draw back");
    let to_b = wait_for(&mut b, Duration::from_secs(5), is_draw)
        .await
        .expect("B receives A's Draw");

    let (origin_a, seq_a) = match &to_a {
        ServerMessage::Draw { origin, seq, .. } => (origin.clone(), *seq),
        other => panic!("expected Draw, got {other:?}"),
    };
    let (origin_b, seq_b) = match &to_b {
        ServerMessage::Draw { origin, seq, .. } => (origin.clone(), *seq),
        other => panic!("expected Draw, got {other:?}"),
    };

    assert_eq!(origin_a, a_id);
    assert_eq!(origin_b, a_id);
    assert_eq!(seq_a, seq_b, "both clients see the same server sequence");

    // …and A's renderer skips it while B's paints it (W2).
    assert!(
        is_own_echo(Some(&a_id), &origin_a),
        "A must skip its own stroke"
    );
    assert!(
        !is_own_echo(Some(&b_id), &origin_b),
        "B must paint A's stroke"
    );

    server.abort();
}

// ---------------------------------------------------------------------------
// 5 — spoofing
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn t1_2_5_a_spoofed_server_message_reaches_nobody() {
    let _guard = hub_lock().await;
    let (addr, server) = spawn_hub().await;

    let mut attacker = connect(addr).await;
    let mut victim = connect(addr).await;
    expect_hello(&mut attacker).await;
    expect_hello(&mut victim).await;

    // v1 rebroadcast any client JSON that parsed as a `WsMessage`, so a phone
    // could inject a `CalendarUpdated` (G13). In v2 neither the v1 shape nor
    // the v2 `ServerMessage` shape is a `ClientMessage`.
    send_raw(
        &mut attacker,
        r#"{"type":"calendar_updated","date":"2026-08-29"}"#,
    )
    .await;
    send_raw(&mut attacker, r#"{"CalendarUpdated":{"events":[]}}"#).await;
    send_raw(
        &mut attacker,
        r#"{"type":"set_view","view":"Whiteboard"}"#, // no `auth` field at all
    )
    .await;

    assert!(
        next_message(&mut victim, Duration::from_secs(2))
            .await
            .is_none(),
        "no spoofed frame may reach another client"
    );

    // The attacker's own socket is untouched — malformed input is dropped,
    // not fatal.
    send(&mut attacker, &ClientMessage::Ping { nonce: 9 }).await;
    assert!(
        wait_for(&mut attacker, Duration::from_secs(5), |message| matches!(
            message,
            ServerMessage::Pong { nonce: 9 }
        ))
        .await
        .is_some(),
        "dropping an unknown message must not close the socket"
    );

    server.abort();
}

// ---------------------------------------------------------------------------
// 6 — authorisation on SetView
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn t1_2_6_set_view_requires_a_parent_session() {
    let _guard = hub_lock().await;
    realtime::session::revoke_all();
    let (addr, server) = spawn_hub().await;

    let mut phone = connect(addr).await;
    let mut tv = connect(addr).await;
    expect_hello(&mut phone).await;
    expect_hello(&mut tv).await;

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
        "an unauthenticated SetView must not be delivered"
    );

    send(
        &mut phone,
        &ClientMessage::SetView {
            view: View::Calendar,
            auth: Some("not-a-real-session".to_string()),
        },
    )
    .await;
    assert!(
        next_message(&mut tv, Duration::from_secs(2))
            .await
            .is_none(),
        "an invalid session token must not be accepted"
    );

    let token = realtime::session::issue();
    send(
        &mut phone,
        &ClientMessage::SetView {
            view: View::Calendar,
            auth: Some(token.clone()),
        },
    )
    .await;
    let delivered = wait_for(&mut tv, Duration::from_secs(5), |message| {
        matches!(message, ServerMessage::SetView { .. })
    })
    .await;
    assert_eq!(
        delivered,
        Some(ServerMessage::SetView {
            view: View::Calendar
        }),
        "a valid parent session must be broadcast as a ServerMessage"
    );

    // Same rule for SetActiveProfile (R-23b).
    send(
        &mut phone,
        &ClientMessage::SetActiveProfile {
            user_id: 3,
            auth: None,
        },
    )
    .await;
    assert!(
        next_message(&mut tv, Duration::from_secs(2))
            .await
            .is_none(),
        "an unauthenticated SetActiveProfile must not be delivered"
    );

    realtime::session::revoke(&token);
    assert!(!realtime::session::is_valid(&token));
    realtime::session::revoke_all();
    server.abort();
}

// ---------------------------------------------------------------------------
// QA round 1, Q1-11 — the cookie half of the parent session
// ---------------------------------------------------------------------------

/// Build a `ws://addr/ws` upgrade request carrying extra headers — the
/// `Cookie`/`Origin` a real browser would attach that plain
/// `tokio_tungstenite::connect_async(url)` never does.
fn client_request_with_headers(
    addr: SocketAddr,
    headers: &[(&str, &str)],
) -> tokio_tungstenite::tungstenite::handshake::client::Request {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    let mut request = format!("ws://{addr}/ws")
        .into_client_request()
        .expect("a valid client request");
    for (name, value) in headers {
        request.headers_mut().insert(
            tokio_tungstenite::tungstenite::http::HeaderName::from_bytes(name.as_bytes())
                .expect("valid header name"),
            value.parse().expect("valid header value"),
        );
    }
    request
}

#[tokio::test]
async fn t1_4_q1_11_set_view_is_delivered_with_a_valid_session_cookie_and_no_bearer_auth() {
    let _guard = hub_lock().await;
    realtime::session::revoke_all();
    let (addr, server) = spawn_hub().await;

    let token = realtime::session::issue();
    let (phone_socket, phone_response) = tokio_tungstenite::connect_async(
        client_request_with_headers(addr, &[("cookie", &format!("fh_session={token}"))]),
    )
    .await
    .expect("a same-origin upgrade with a valid session cookie must succeed");
    assert_eq!(phone_response.status().as_u16(), 101);
    let mut phone = phone_socket;
    let mut tv = connect(addr).await;
    expect_hello(&mut phone).await;
    expect_hello(&mut tv).await;

    // No `auth` token on the message itself — the connection's own upgrade
    // cookie is what authorises it (Q1-11).
    send(
        &mut phone,
        &ClientMessage::SetView {
            view: View::Whiteboard,
            auth: None,
        },
    )
    .await;

    let delivered = wait_for(&mut tv, Duration::from_secs(5), |message| {
        matches!(message, ServerMessage::SetView { .. })
    })
    .await;
    assert_eq!(
        delivered,
        Some(ServerMessage::SetView {
            view: View::Whiteboard
        }),
        "a connection whose upgrade carried a valid fh_session cookie must be treated \
         as an authorised parent even when the ClientMessage itself carries no bearer token"
    );

    realtime::session::revoke(&token);
    realtime::session::revoke_all();
    server.abort();
}

#[tokio::test]
async fn t1_4_q1_11_a_cross_origin_websocket_upgrade_is_rejected() {
    let _guard = hub_lock().await;
    let (addr, server) = spawn_hub().await;

    let request = client_request_with_headers(addr, &[("origin", "http://evil.example")]);
    let err = tokio_tungstenite::connect_async(request)
        .await
        .expect_err("a cross-origin websocket upgrade must be refused, not accepted");
    match err {
        tokio_tungstenite::tungstenite::Error::Http(response) => {
            assert_eq!(
                response.status().as_u16(),
                403,
                "expected 403 Forbidden for a cross-origin Origin header"
            );
        }
        other => panic!("expected an HTTP-level rejection, got {other:?}"),
    }

    // A same-origin Origin (matching the Host the request was actually sent
    // to) is unaffected.
    let same_origin = client_request_with_headers(addr, &[("origin", &format!("http://{addr}"))]);
    let (_socket, response) = tokio_tungstenite::connect_async(same_origin)
        .await
        .expect("a same-origin upgrade must still succeed");
    assert_eq!(response.status().as_u16(), 101);

    server.abort();
}

// ---------------------------------------------------------------------------
// 7 — kill, restart, reconnect with a Snapshot
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn t1_2_7_a_client_reconnects_and_resnapshots_within_thirty_seconds() {
    let _guard = hub_lock().await;
    realtime::reset_board().await;
    let (addr, server) = spawn_hub().await;

    let mut client = connect(addr).await;
    expect_hello(&mut client).await;

    // Something to resynchronise to.
    send(
        &mut client,
        &ClientMessage::Draw {
            board_id: DEFAULT_BOARD_ID,
            stroke: stroke_with(0x9e5_7a7, 3),
        },
    )
    .await;
    wait_for(&mut client, Duration::from_secs(5), |m| {
        matches!(m, ServerMessage::Draw { .. })
    })
    .await
    .expect("the stroke is stored before the restart");

    // Kill the server; the socket dies with it.
    server.abort();
    let _ = server.await;
    while next_message(&mut client, Duration::from_secs(5))
        .await
        .is_some()
    {}
    drop(client);

    let restart_at = Instant::now();
    let restarted = serve_on(addr).await;

    // The reconnect supervisor from `client::realtime`, driven by the same
    // `backoff` function the browser uses.
    let mut attempt = 0u32;
    let snapshot = loop {
        assert!(
            restart_at.elapsed() < Duration::from_secs(30),
            "reconnect took longer than 30 s"
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
                break wait_for(&mut socket, Duration::from_secs(5), |m| {
                    matches!(m, ServerMessage::Snapshot { .. })
                })
                .await;
            }
            Err(_) => {
                tokio::time::sleep(backoff(attempt)).await;
                attempt = attempt.saturating_add(1);
            }
        }
    };

    let elapsed = restart_at.elapsed();
    assert!(
        elapsed < Duration::from_secs(30),
        "reconnect + Snapshot took {elapsed:?}"
    );
    let snapshot = snapshot.expect("the reconnected client receives a Snapshot");
    match snapshot {
        ServerMessage::Snapshot {
            board_id, strokes, ..
        } => {
            assert_eq!(board_id, DEFAULT_BOARD_ID);
            assert_eq!(
                strokes.len(),
                1,
                "the snapshot replays the stroke drawn before the restart"
            );
            assert_eq!(marker_of(&strokes[0]), 0x9e5_7a7);
        }
        other => panic!("expected Snapshot, got {other:?}"),
    }

    restarted.abort();
}

// ---------------------------------------------------------------------------
// 8 — DST-safe midnight tick
// ---------------------------------------------------------------------------

mod dst {
    use chrono::{
        DateTime, FixedOffset, MappedLocalTime, NaiveDate, NaiveDateTime, Offset, TimeZone,
    };

    /// A minimal, hand-written `chrono::TimeZone` carrying the real 2026 DST
    /// rules for one zone.
    ///
    /// `chrono-tz` is not a dependency of this project and adding one is not
    /// T1.2's to make (`PURPLE_TEAM.md` §P4: `Cargo.toml` changes go through
    /// the Boss), so the two zones the acceptance test names are modelled
    /// directly. That is *stronger* than a table lookup here: the local→UTC
    /// mapping below derives ambiguity and gaps from the UTC rule, so
    /// `next_midnight`'s `.earliest()` is exercised against genuine
    /// `MappedLocalTime::{None, Ambiguous}` values.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct TestZone {
        pub name: &'static str,
        /// Standard-time offset, in seconds east of UTC.
        pub standard: i32,
        /// Daylight-time offset, in seconds east of UTC.
        pub daylight: i32,
        /// First UTC instant of daylight time, and the first UTC instant back
        /// on standard time, as `(y, m, d, h)`.
        pub dst_start_utc: (i32, u32, u32, u32),
        pub dst_end_utc: (i32, u32, u32, u32),
    }

    /// `America/New_York`, 2026: EDT from 2026-03-08 07:00 UTC to
    /// 2026-11-01 06:00 UTC.
    pub const NEW_YORK: TestZone = TestZone {
        name: "America/New_York",
        standard: -5 * 3600,
        daylight: -4 * 3600,
        dst_start_utc: (2026, 3, 8, 7),
        dst_end_utc: (2026, 11, 1, 6),
    };

    /// `Europe/London`, 2026: BST from 2026-03-29 01:00 UTC to
    /// 2026-10-25 01:00 UTC.
    pub const LONDON: TestZone = TestZone {
        name: "Europe/London",
        standard: 0,
        daylight: 3600,
        dst_start_utc: (2026, 3, 29, 1),
        dst_end_utc: (2026, 10, 25, 1),
    };

    fn naive(parts: (i32, u32, u32, u32)) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(parts.0, parts.1, parts.2)
            .expect("valid date")
            .and_hms_opt(parts.3, 0, 0)
            .expect("valid time")
    }

    impl TestZone {
        fn offset_seconds_at_utc(&self, utc: &NaiveDateTime) -> i32 {
            if *utc >= naive(self.dst_start_utc) && *utc < naive(self.dst_end_utc) {
                self.daylight
            } else {
                self.standard
            }
        }
    }

    impl TimeZone for TestZone {
        type Offset = FixedOffset;

        fn from_offset(offset: &Self::Offset) -> Self {
            // chrono reconstructs the zone from a stored offset in
            // `DateTime::timezone()`, which `next_midnight` itself calls, so
            // this has to be right. The two zones under test have disjoint
            // offset sets (−5/−4 h vs 0/+1 h), so the offset identifies the
            // zone unambiguously.
            let seconds = offset.local_minus_utc();
            if seconds == LONDON.standard || seconds == LONDON.daylight {
                LONDON
            } else {
                NEW_YORK
            }
        }

        fn offset_from_local_date(&self, local: &NaiveDate) -> MappedLocalTime<Self::Offset> {
            self.offset_from_local_datetime(&local.and_hms_opt(0, 0, 0).expect("midnight is valid"))
        }

        fn offset_from_local_datetime(
            &self,
            local: &NaiveDateTime,
        ) -> MappedLocalTime<Self::Offset> {
            // A candidate offset is valid iff interpreting `local` with it
            // lands on a UTC instant that really does use that offset. Zero
            // valid candidates is a spring-forward gap; two is a fall-back
            // fold.
            let mut valid: Vec<i32> = [self.standard, self.daylight]
                .into_iter()
                .filter(|candidate| {
                    let utc = *local - chrono::Duration::seconds(i64::from(*candidate));
                    self.offset_seconds_at_utc(&utc) == *candidate
                })
                .collect();
            valid.sort_unstable();
            valid.dedup();

            let fixed = |seconds: i32| FixedOffset::east_opt(seconds).expect("valid offset");
            match valid.as_slice() {
                [] => MappedLocalTime::None,
                [only] => MappedLocalTime::Single(fixed(*only)),
                // Earliest local time = the *larger* UTC offset.
                [smaller, larger] => MappedLocalTime::Ambiguous(fixed(*larger), fixed(*smaller)),
                _ => unreachable!("a zone has at most two offsets"),
            }
        }

        fn offset_from_utc_date(&self, utc: &NaiveDate) -> Self::Offset {
            self.offset_from_utc_datetime(&utc.and_hms_opt(0, 0, 0).expect("midnight is valid"))
        }

        fn offset_from_utc_datetime(&self, utc: &NaiveDateTime) -> Self::Offset {
            FixedOffset::east_opt(self.offset_seconds_at_utc(utc)).expect("valid offset")
        }
    }

    /// Local wall-clock time in `zone`, panicking if it is ambiguous or absent
    /// (the test only builds unambiguous starting points).
    pub fn local(
        zone: TestZone,
        y: i32,
        m: u32,
        d: u32,
        hh: u32,
        mm: u32,
        ss: u32,
    ) -> DateTime<TestZone> {
        zone.with_ymd_and_hms(y, m, d, hh, mm, ss)
            .single()
            .unwrap_or_else(|| panic!("{y}-{m}-{d} {hh}:{mm}:{ss} is not a single local time"))
    }

    pub fn offset_seconds(moment: &DateTime<TestZone>) -> i32 {
        moment.offset().fix().local_minus_utc()
    }
}

#[test]
fn t1_2_8_the_midnight_tick_is_correct_across_both_dst_transitions() {
    use chrono::Timelike;
    use dst::{local, offset_seconds, LONDON, NEW_YORK};
    use family_calendar::server::api::realtime::{duration_until_midnight, next_midnight};

    // (zone, spring-forward date, fall-back date, standard offset, dst offset)
    let cases = [
        (NEW_YORK, (2026, 3, 8), (2026, 11, 1), -5 * 3600, -4 * 3600),
        (LONDON, (2026, 3, 29), (2026, 10, 25), 0, 3600),
    ];

    for (zone, spring, fall, standard, daylight) in cases {
        for (transition, expected_hours) in [(spring, 23i64), (fall, 25i64)] {
            let (y, m, d) = transition;
            let eve = local(zone, y, m, d, 0, 0, 0) - chrono::Duration::seconds(2);
            assert_eq!(
                eve.hour(),
                23,
                "{}: the injected clock is 23:59:58 local",
                zone.name
            );
            assert_eq!(eve.minute(), 59);
            assert_eq!(eve.second(), 58);

            // The tick fires exactly two seconds later, at local midnight.
            let first = next_midnight(&eve);
            assert_eq!(
                duration_until_midnight(&eve),
                Duration::from_secs(2),
                "{}: 23:59:58 must be 2 s before the {y}-{m}-{d} tick",
                zone.name
            );
            assert_eq!((first.hour(), first.minute(), first.second()), (0, 0, 0));
            assert_eq!(
                (first.date_naive().year_month_day()),
                (y, m, d),
                "{}: the tick lands on {y}-{m}-{d}",
                zone.name
            );

            // …and exactly once: the next tick is a whole local day later,
            // which is 23 h on the spring-forward day and 25 h on the
            // fall-back day.
            let second = next_midnight(&first);
            let span = (second - first).num_hours();
            assert_eq!(
                span, expected_hours,
                "{}: the local day starting {y}-{m}-{d} is {expected_hours} h long, got {span}",
                zone.name
            );
            assert_eq!((second.hour(), second.minute(), second.second()), (0, 0, 0));
        }

        // Sanity: the zone really does change offset across the transitions.
        let (sy, sm, sd) = spring;
        let before = local(zone, sy, sm, sd, 0, 0, 0);
        let after = local(zone, sy, sm, sd, 12, 0, 0);
        assert_eq!(offset_seconds(&before), standard, "{}", zone.name);
        assert_eq!(offset_seconds(&after), daylight, "{}", zone.name);
    }
}

/// Helper so the assertion above reads as one comparison.
trait YearMonthDay {
    fn year_month_day(&self) -> (i32, u32, u32);
}

impl YearMonthDay for chrono::NaiveDate {
    fn year_month_day(&self) -> (i32, u32, u32) {
        use chrono::Datelike;
        (self.year(), self.month(), self.day())
    }
}

// ---------------------------------------------------------------------------
// 9 — rate limiting isolates the offender
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn t1_2_9_a_flooding_client_is_throttled_and_closed_without_touching_the_others() {
    let _guard = hub_lock().await;
    let (addr, server) = spawn_hub().await;

    let mut bystander = connect(addr).await;
    expect_hello(&mut bystander).await;

    let offender = connect(addr).await;
    let (mut offender_write, mut offender_read) = offender.split();
    // Drain the Hello.
    offender_read.next().await;

    // 200 msg/s — five times the 40/s budget — for up to six seconds.
    let flood = tokio::spawn(async move {
        let payload = serde_json::to_string(&ClientMessage::Ping { nonce: 1 }).expect("serializes");
        for _ in 0..1_200 {
            if offender_write
                .send(WsFrame::text(payload.clone()))
                .await
                .is_err()
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    });

    // The offender is warned with a Resync…
    let mut saw_resync = false;
    let mut closed = false;
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_secs(5), offender_read.next()).await {
            Ok(Some(Ok(WsFrame::Text(text)))) => {
                if matches!(parse(&text), ServerMessage::Resync { .. }) {
                    saw_resync = true;
                }
            }
            Ok(Some(Ok(WsFrame::Close(_)))) | Ok(None) | Ok(Some(Err(_))) => {
                closed = true;
                break;
            }
            Ok(Some(Ok(_))) => continue,
            Err(_) => break,
        }
    }
    flood.abort();

    assert!(saw_resync, "the flooding client must be sent a Resync");
    assert!(
        closed,
        "after three consecutive over-budget seconds the offender's socket is closed"
    );

    // …and nobody else notices.
    send(&mut bystander, &ClientMessage::Ping { nonce: 5 }).await;
    let pong = wait_for(&mut bystander, Duration::from_secs(5), |message| {
        matches!(message, ServerMessage::Pong { nonce: 5 })
    })
    .await;
    assert!(
        pong.is_some(),
        "throttling one client must not affect any other (§P2c assertion 9)"
    );

    server.abort();
}

// ---------------------------------------------------------------------------
// docs/PROTOCOL.md completeness
// ---------------------------------------------------------------------------

/// Exhaustive match — adding a variant to `ClientMessage` stops this compiling
/// until `docs/PROTOCOL.md` is updated too.
fn client_variant_name(message: &ClientMessage) -> &'static str {
    match message {
        ClientMessage::Hello { .. } => "Hello",
        ClientMessage::Ping { .. } => "Ping",
        ClientMessage::Draw { .. } => "Draw",
        ClientMessage::ClearBoard { .. } => "ClearBoard",
        ClientMessage::SetView { .. } => "SetView",
        ClientMessage::SetActiveProfile { .. } => "SetActiveProfile",
        ClientMessage::RequestSnapshot { .. } => "RequestSnapshot",
    }
}

fn server_variant_name(message: &ServerMessage) -> &'static str {
    match message {
        ServerMessage::Hello { .. } => "Hello",
        ServerMessage::Pong { .. } => "Pong",
        ServerMessage::Resync { .. } => "Resync",
        ServerMessage::Draw { .. } => "Draw",
        ServerMessage::BoardCleared { .. } => "BoardCleared",
        ServerMessage::Snapshot { .. } => "Snapshot",
        ServerMessage::RoutineUpdated { .. } => "RoutineUpdated",
        ServerMessage::TasksUpdated { .. } => "TasksUpdated",
        ServerMessage::ProfilesUpdated => "ProfilesUpdated",
        ServerMessage::CalendarUpdated { .. } => "CalendarUpdated",
        ServerMessage::DayRolled { .. } => "DayRolled",
        ServerMessage::SetView { .. } => "SetView",
        ServerMessage::SetActiveProfile { .. } => "SetActiveProfile",
        ServerMessage::Health { .. } => "Health",
        ServerMessage::HomeschoolUpdated { .. } => "HomeschoolUpdated",
        ServerMessage::CurriculumUpdated { .. } => "CurriculumUpdated",
    }
}

fn every_client_message() -> Vec<ClientMessage> {
    let stroke = stroke_with(0x000000, 2);
    vec![
        ClientMessage::Hello { protocol: 2 },
        ClientMessage::Ping { nonce: 0 },
        ClientMessage::Draw {
            board_id: 1,
            stroke,
        },
        ClientMessage::ClearBoard { board_id: 1 },
        ClientMessage::SetView {
            view: View::None,
            auth: None,
        },
        ClientMessage::SetActiveProfile {
            user_id: 1,
            auth: None,
        },
        ClientMessage::RequestSnapshot {
            board_id: 1,
            since_seq: 0,
        },
    ]
}

fn every_server_message() -> Vec<ServerMessage> {
    vec![
        ServerMessage::Hello {
            client_id: ClientId("x".into()),
            protocol: 2,
            server_time: String::new(),
            today: String::new(),
        },
        ServerMessage::Pong { nonce: 0 },
        ServerMessage::Resync {
            reason: ResyncReason::Lagged,
        },
        ServerMessage::Draw {
            board_id: 1,
            seq: 1,
            origin: ClientId("x".into()),
            stroke: stroke_with(0x000000, 2),
        },
        ServerMessage::BoardCleared {
            board_id: 1,
            seq: 1,
            origin: ClientId("x".into()),
        },
        ServerMessage::Snapshot {
            board_id: 1,
            seq: 1,
            strokes: Vec::new(),
        },
        ServerMessage::RoutineUpdated {
            user_id: 1,
            date: String::new(),
        },
        ServerMessage::TasksUpdated {
            user_id: 1,
            date: String::new(),
        },
        ServerMessage::ProfilesUpdated,
        ServerMessage::CalendarUpdated {
            date: String::new(),
        },
        ServerMessage::DayRolled {
            date: String::new(),
        },
        ServerMessage::SetView { view: View::None },
        ServerMessage::SetActiveProfile { user_id: 1 },
        ServerMessage::Health {
            stale: false,
            last_update: String::new(),
        },
        ServerMessage::HomeschoolUpdated {
            user_ids: vec![1],
            week: 1,
            date: String::new(),
        },
        ServerMessage::CurriculumUpdated { curriculum_id: 1 },
    ]
}

/// **HS3 accept (d)** — the two homeschool variants joined the protocol, so
/// the sample vector is the previous fourteen **plus two**, and both names
/// reach `docs/PROTOCOL.md` §4 through
/// `t1_2_protocol_doc_names_every_message_variant` above.
///
/// The count is spelled out rather than left implicit because
/// `every_server_message()` is what proves the doc table is complete: a
/// variant added to `ServerMessage` and forgotten here would silently stop
/// being checked against the document.
#[test]
fn hs3_the_server_message_sample_vector_gained_exactly_the_two_homeschool_variants() {
    let messages = every_server_message();
    assert_eq!(
        messages.len(),
        16,
        "fourteen protocol-v2 variants plus HomeschoolUpdated and CurriculumUpdated"
    );

    let names: Vec<&str> = messages.iter().map(server_variant_name).collect();
    for expected in ["HomeschoolUpdated", "CurriculumUpdated"] {
        assert!(
            names.contains(&expected),
            "every_server_message() must carry a ServerMessage::{expected} sample"
        );
    }
    assert_eq!(
        names.iter().collect::<HashSet<_>>().len(),
        names.len(),
        "every sample must be a distinct variant"
    );
}

#[test]
fn t1_2_protocol_doc_names_every_message_variant() {
    let doc = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/PROTOCOL.md"),
    )
    .expect("docs/PROTOCOL.md exists");

    for message in every_client_message() {
        let name = client_variant_name(&message);
        assert!(
            doc.contains(&format!("ClientMessage::{name}")),
            "docs/PROTOCOL.md does not document ClientMessage::{name}"
        );
    }
    for message in every_server_message() {
        let name = server_variant_name(&message);
        assert!(
            doc.contains(&format!("ServerMessage::{name}")),
            "docs/PROTOCOL.md does not document ServerMessage::{name}"
        );
    }
}

#[test]
fn t1_2_protocol_doc_states_the_normative_limits() {
    let doc = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/PROTOCOL.md"),
    )
    .expect("docs/PROTOCOL.md exists");

    for needle in [
        "1024",    // broadcast capacity
        "256",     // outbound queue
        "40",      // token bucket refill
        "80",      // burst
        "30",      // client flush cap / backoff cap
        "20 s",    // heartbeat
        "90 s",    // server idle timeout
        "±20",     // jitter
        "2,000",   // retained strokes
        "256 KiB", // Q1-10 max websocket message
        "25 s",    // Q1-13 Health heartbeat
    ] {
        assert!(
            doc.contains(needle),
            "docs/PROTOCOL.md must state {needle:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// QA round 1 — Q1-10 (bounded frames) and Q1-13 (the Health heartbeat)
// ---------------------------------------------------------------------------

/// **Q1-10** — an oversized frame closes only the socket that sent it.
///
/// `WebSocketUpgrade` used to keep `tungstenite`'s 64 MiB default, so any
/// device on the family LAN could hand the television's wasm JSON parser a
/// multi-megabyte message. The upgrade now caps message *and* frame at
/// `realtime::MAX_WS_MESSAGE_BYTES`; the codec refuses anything larger before
/// `serde_json` ever sees it, and — as with the rate limiter (§P2c assertion
/// 9) — the blast radius is the offender's own connection.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn qa1_10_an_oversized_frame_closes_only_the_sender() {
    let _guard = hub_lock().await;
    realtime::reset_board().await;
    let (addr, server) = spawn_hub().await;

    let mut offender = connect(addr).await;
    let mut bystander = connect(addr).await;
    expect_hello(&mut offender).await;
    let bystander_id = expect_hello(&mut bystander).await;

    // The payload has to be over the cap for the test to prove anything, and
    // a compile-time check says so without waiting for the socket.
    const OVERSIZED: usize = 1024 * 1024;
    const _: () = assert!(OVERSIZED > realtime::MAX_WS_MESSAGE_BYTES);
    send_raw(&mut offender, &"x".repeat(OVERSIZED)).await;

    assert!(
        next_message(&mut offender, Duration::from_secs(10))
            .await
            .is_none(),
        "the sender's socket must be closed, not fed a reply"
    );

    // The bystander is untouched: it can still drive the hub and still hear
    // the answer.
    send(
        &mut bystander,
        &ClientMessage::Draw {
            board_id: DEFAULT_BOARD_ID,
            stroke: stroke_with(0xb57a, 4),
        },
    )
    .await;
    let echoed = wait_for(&mut bystander, Duration::from_secs(5), |message| {
        matches!(message, ServerMessage::Draw { .. })
    })
    .await
    .expect("the bystander's socket survived the offender's oversized frame");
    match echoed {
        ServerMessage::Draw { origin, stroke, .. } => {
            assert_eq!(origin, bystander_id);
            assert_eq!(marker_of(&stroke), 0xb57a);
        }
        other => panic!("expected Draw, got {other:?}"),
    }

    server.abort();
}

/// **Q1-10** — a well-sized frame carrying a hostile stroke is dropped, and
/// the connection carries on.
///
/// The three shapes that used to reach SQLite and the TV canvas unchallenged:
/// an unbounded `color`, a non-finite `width`, and points outside the
/// normalised `0..=1` space. `realtime::valid_stroke` is unit-tested
/// exhaustively; this proves the `Draw` arm actually consults it, over a real
/// socket, without closing the connection.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn qa1_10_an_invalid_stroke_is_dropped_without_closing_the_connection() {
    let _guard = hub_lock().await;
    realtime::reset_board().await;
    let (addr, server) = spawn_hub().await;

    let mut author = connect(addr).await;
    let mut watcher = connect(addr).await;
    expect_hello(&mut author).await;
    expect_hello(&mut watcher).await;

    let hostile = [
        ("unbounded colour", {
            let mut stroke = stroke_with(0x00_0001, 2);
            stroke.color = "z".repeat(64 * 1024);
            stroke
        }),
        ("NaN width", {
            let mut stroke = stroke_with(0x00_0002, 2);
            stroke.width = f64::NAN;
            stroke
        }),
        ("1e308 width", {
            let mut stroke = stroke_with(0x00_0003, 2);
            stroke.width = 1e308;
            stroke
        }),
        ("point off the canvas", {
            let mut stroke = stroke_with(0x00_0004, 2);
            stroke.points[0] = StrokePoint {
                x: 4_096.0,
                y: -1.0,
            };
            stroke
        }),
    ];
    for (label, stroke) in hostile {
        send(
            &mut author,
            &ClientMessage::Draw {
                board_id: DEFAULT_BOARD_ID,
                stroke,
            },
        )
        .await;
        assert!(
            next_message(&mut watcher, Duration::from_millis(400))
                .await
                .is_none(),
            "a stroke with a {label} must not be fanned out"
        );
    }

    // …and the author is still connected, still able to draw.
    send(
        &mut author,
        &ClientMessage::Draw {
            board_id: DEFAULT_BOARD_ID,
            stroke: stroke_with(0x600d, 4),
        },
    )
    .await;
    let good = wait_for(&mut watcher, Duration::from_secs(5), |message| {
        matches!(message, ServerMessage::Draw { .. })
    })
    .await
    .expect("a valid stroke still gets through");
    match good {
        ServerMessage::Draw { stroke, .. } => assert_eq!(marker_of(&stroke), 0x600d),
        other => panic!("expected Draw, got {other:?}"),
    }

    server.abort();
}

/// **Q1-13** — a connected client receives `Health` within 2× the interval.
///
/// `ServerMessage::Health` is documented by D5 and `docs/PROTOCOL.md` as the
/// television badge's freshness signal, and until QA round 1 nothing in the
/// server ever sent one. `spawn_health_heartbeat` is deliberately started by
/// `server::router::run` rather than by `ws_handler`, so every "nothing else
/// arrives on an idle socket" assertion above keeps its meaning — which is
/// also why this test starts the heartbeat itself and aborts it before the
/// hub lock is released.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn qa1_13_a_connected_client_receives_health_within_two_intervals() {
    let _guard = hub_lock().await;
    realtime::reset_board().await;
    let (addr, server) = spawn_hub().await;

    let mut client = connect(addr).await;
    expect_hello(&mut client).await;

    // A sped-up copy of the production loop: the same function `router::run`
    // calls, with the interval the assertion can afford to wait out.
    const INTERVAL: Duration = Duration::from_secs(1);
    let heartbeat = realtime::spawn_health_heartbeat(INTERVAL);

    let health = wait_for(&mut client, INTERVAL * 2, |message| {
        matches!(message, ServerMessage::Health { .. })
    })
    .await;
    heartbeat.abort();

    match health.expect("a connected client receives Health within 2× the interval") {
        ServerMessage::Health { stale, last_update } => {
            assert!(
                !stale,
                "`stale` is reserved until there is a Google poll to be stale about \
                 (docs/PROTOCOL.md §4.1)"
            );
            let parsed = chrono::DateTime::parse_from_rfc3339(&last_update)
                .expect("last_update is RFC 3339");
            let skew = (chrono::Local::now() - parsed.with_timezone(&chrono::Local))
                .num_seconds()
                .abs();
            assert!(
                skew < 60,
                "last_update is the hub's own clock, off by {skew}s"
            );
        }
        other => unreachable!("wait_for matched Health, got {other:?}"),
    }

    // Production runs at 25 s, three of which still fit inside D8's 90 s
    // staleness threshold.
    assert_eq!(realtime::HEALTH_HEARTBEAT_INTERVAL, Duration::from_secs(25));

    server.abort();
}
