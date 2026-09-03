//! **HS7 acceptance (a, b)** — `docs/homeschool/PLAN_HOMESCHOOL.md` §3 HS7:
//! the cross-surface loop for the School tab, the one place (alongside
//! `tests/loop_tests.rs`'s T2.6) that plays both the phone and the TV's live
//! `/ws` connections at once and proves a real household evening: a parent's
//! phone moves the week pointer and the TV sees it; a boy ticks his own work
//! on the TV (no cookie) and the parent's phone sees it; a shared read-aloud
//! ticked as **Together** reaches both; and a power-cycle of the hub is
//! invisible to either surface.
//!
//! | Accept | Proven by |
//! | --- | --- |
//! | (a) phone-authed `set_school_week` reaches the TV client < 1 s | step 2 |
//! | (a) a TV-side `toggle_lesson` reaches the phone < 1 s | step 1 |
//! | (a) a Together tick reaches both | step 3 |
//! | (b) kill + restart ⇒ both resync | step 4 |
//!
//! Every School mutation is a `#[server]` fn, not a `ClientMessage` the
//! socket carries (unlike T2.6's `SetView`/`Draw`) — so "reaches the TV" /
//! "reaches the phone" here means: call the real function body in-process
//! (exactly as `tests/homeschool_tests.rs`'s direct-call tests do — the
//! production code path, not a mock), and observe the `ServerMessage`
//! broadcast on a live WS client tagged `phone` or `tv`, exactly as those
//! two surfaces' own reconnect supervisors would. The WS harness
//! (`connect`/`send`/`next_message`/`wait_for`/`expect_hello`) is
//! deliberately the same shape as `tests/loop_tests.rs` and
//! `tests/realtime_tests.rs` — integration test binaries cannot share code
//! except through a `mod` included by `#[path]`, and this file owns no
//! production module, so it is self-contained rather than reaching into
//! another task's test file.
//!
//! **§0 N1.** Every curriculum string here is the committed, invented
//! `tests/fixtures/curricula/sample-year.toml` — nothing from
//! `docs/homeschool/curriculum/`.

#![cfg(feature = "server")]

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use axum::routing::get;
use axum::Router;
use family_calendar::client::realtime::backoff;
use family_calendar::server::api::homeschool as api;
use family_calendar::server::api::realtime;
use family_calendar::server::db;
use family_calendar::server::homeschool::db as hs;
use family_calendar::server::homeschool::loader;
use family_calendar::shared::homeschool::LogStatus;
use family_calendar::shared::types::ServerMessage;
use futures_util::StreamExt;
use sqlx::SqlitePool;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message as WsFrame;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type WsStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

// ---------------------------------------------------------------------------
// Harness (mirrors tests/loop_tests.rs and tests/homeschool_tests.rs)
// ---------------------------------------------------------------------------

/// Point this test binary at its own throwaway sqlite file, isolated from
/// every other test binary's `DATABASE_URL`. Its `curricula\` directory is
/// left empty on purpose: the boot-time Isaiah seed then finds no
/// `ao-year-1` curriculum and skips (H5 default 6), so it cannot collide
/// with this file's own direct enrollments of profiles 1–3.
fn init_test_env() -> PathBuf {
    static ONCE: std::sync::Once = std::sync::Once::new();
    let base =
        std::env::temp_dir().join(format!("familyhub-hs7-loop-tests-{}", std::process::id()));
    ONCE.call_once(|| {
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
    base
}

/// The realtime hub on its own router — just `/ws`, the same surface both
/// the TV and the phone PWA actually speak (`docs/PROTOCOL.md` §9). School's
/// mutations are called in-process below rather than over HTTP, so no
/// dioxus-registered server-fn router or `DIOXUS_PUBLIC_PATH` is needed.
fn hub_router() -> Router {
    Router::new().route("/ws", get(realtime::ws_handler))
}

async fn spawn_hub() -> (SocketAddr, JoinHandle<()>) {
    init_test_env();
    db::pool().await.expect("test sqlite pool opens");
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind an ephemeral port");
    let addr = listener.local_addr().expect("listener has a local address");
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, hub_router()).await;
    });
    (addr, handle)
}

/// Serve on a *specific* address, retrying while the previous listener's
/// port is still being released — the restart step rebinds the address the
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

/// Protocol v2: the server's first frame is always `Hello`.
async fn expect_hello(socket: &mut WsStream) {
    match next_message(socket, Duration::from_secs(5)).await {
        Some(ServerMessage::Hello { client_id, .. }) => {
            assert!(!client_id.as_str().is_empty(), "the server mints an id");
        }
        other => panic!("expected Hello as the first frame, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// School-specific fixture helpers (mirror tests/homeschool_tests.rs)
// ---------------------------------------------------------------------------

fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn fixture_path() -> PathBuf {
    repo_root().join("tests/fixtures/curricula/sample-year.toml")
}

fn today_string() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn uuid_ish() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

/// Load the committed synthetic fixture (idempotent), returning its
/// `curriculum_id`.
async fn load_fixture(pool: &SqlitePool) -> i64 {
    let validated = loader::read_curriculum(&fixture_path()).expect("fixture validates");
    loader::insert_missing(pool, &validated)
        .await
        .expect("insert the fixture")
        .curriculum_id
}

async fn subject_id(pool: &SqlitePool, curriculum_id: i64, name: &str) -> i64 {
    let row: (i64,) =
        sqlx::query_as("SELECT id FROM subjects WHERE curriculum_id = ?1 AND name = ?2")
            .bind(curriculum_id)
            .bind(name)
            .fetch_one(pool)
            .await
            .unwrap_or_else(|err| panic!("subject {name}: {err}"));
    row.0
}

/// Widen a subject's `days` to all seven letters so it occurs on **every**
/// date of an enrollment's span — this suite's `week_started_on` values are
/// arbitrary anchors unrelated to the real calendar, so this is what makes
/// each occurrence's `scheduled_date` deterministic regardless of which real
/// weekday the suite happens to run on.
async fn widen_to_every_day(pool: &SqlitePool, subject_id: i64, shared: bool) {
    hs::set_subject_schedule(pool, subject_id, "MTWRFSU", shared)
        .await
        .expect("widen subject days for the test");
}

/// The first occurrence of `subject_name` in `user_id`'s week-`week` grid —
/// `(subject_id, assignment_id, scheduled_date)`, exactly the triple every
/// School mutation validates against.
async fn first_occurrence(
    user_id: i64,
    week: i64,
    subject_name: &str,
) -> (i64, Option<i64>, String) {
    let grid = api::get_week_grid(user_id, week)
        .await
        .expect("get_week_grid");
    let row = grid
        .rows
        .iter()
        .find(|r| r.title == subject_name)
        .unwrap_or_else(|| panic!("no {subject_name} row in the grid"));
    let occurrence = row
        .cells
        .iter()
        .flatten()
        .next()
        .unwrap_or_else(|| panic!("no occurrence for {subject_name} in week {week}"));
    (
        occurrence.subject_id,
        occurrence.assignment_id,
        occurrence.scheduled_date.clone(),
    )
}

async fn lesson_log_count(pool: &SqlitePool, profile_id: i64, subject_id: i64) -> i64 {
    let row: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM lesson_log WHERE profile_id = ?1 AND subject_id = ?2")
            .bind(profile_id)
            .bind(subject_id)
            .fetch_one(pool)
            .await
            .expect("count lesson_log");
    row.0
}

/// `realtime::sender()` is one process-wide broadcast channel and
/// `realtime::session` a process-wide token set — this lock (mirrors
/// `tests/loop_tests.rs::hub_lock`) keeps this test binary's other tests, if
/// any are ever added, from interleaving with this one.
async fn hub_lock() -> tokio::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await
}

// ---------------------------------------------------------------------------
// The loop
// ---------------------------------------------------------------------------

/// **HS7 (a), (b)** — the whole School control path end-to-end: boot the
/// hub; open two Rust WS clients tagged `phone` and `tv`; a boy ticks his
/// own lesson on the TV (no cookie) and it must reach the phone within 1 s;
/// a parent's phone finishes the week and it must reach the TV within 1 s;
/// a shared read-aloud ticked as Together must reach both; killing and
/// restarting the hub must leave both surfaces resynced — new connections
/// reconnect and the school state written before the restart survives it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn hs7_a_b_the_school_control_path_survives_two_surfaces_and_a_restart() {
    let _guard = hub_lock().await;
    realtime::session::revoke_all();
    let (addr, server) = spawn_hub().await;
    let pool = db::pool().await.expect("pool");

    let curriculum_id = load_fixture(pool).await;
    let sums = subject_id(pool, curriculum_id, "Sums").await;
    let old_tales = subject_id(pool, curriculum_id, "Old Tales").await;
    widen_to_every_day(pool, sums, false).await;
    widen_to_every_day(pool, old_tales, true).await;

    // Three of the four seeded profiles (`migrations/0004_name_the_boys.sql`).
    const BOY_A: i64 = 1; // ticks Sums on the TV; his phone-authed week move
    const BOY_B: i64 = 2; // shares Old Tales with BOY_C for the Together tick
    const BOY_C: i64 = 3;
    hs::upsert_enrollment(pool, BOY_A, curriculum_id, 2, "MTWRFSU", "2026-01-05")
        .await
        .expect("enroll boy A");
    hs::upsert_enrollment(pool, BOY_B, curriculum_id, 2, "MTWRFSU", "2026-01-05")
        .await
        .expect("enroll boy B");
    hs::upsert_enrollment(pool, BOY_C, curriculum_id, 2, "MTWRFSU", "2026-01-05")
        .await
        .expect("enroll boy C");

    // Two Rust WS clients, tagged exactly the way the acceptance test names
    // them: `phone` is the parents' controller surface, `tv` is the kiosk.
    let mut phone = connect(addr).await;
    let mut tv = connect(addr).await;
    expect_hello(&mut phone).await;
    expect_hello(&mut tv).await;

    // ------------------------------------------------------------------
    // 1. A TV-side toggle_lesson (no cookie — H7) reaches the phone < 1 s.
    // ------------------------------------------------------------------
    let (sums_subject, sums_assignment, sums_date) = first_occurrence(BOY_A, 2, "Sums").await;
    let sent_at = Instant::now();
    api::toggle_lesson(
        BOY_A,
        sums_subject,
        sums_assignment,
        2,
        sums_date.clone(),
        true,
        LogStatus::Done,
        None,
        today_string(),
        format!("hs7-tv-{}", uuid_ish()),
    )
    .await
    .expect("a boy's own TV tick needs no cookie");
    let delivered = wait_for(&mut phone, Duration::from_secs(1), |message| {
        matches!(
            message,
            ServerMessage::HomeschoolUpdated { user_ids, .. } if user_ids.contains(&BOY_A)
        )
    })
    .await;
    assert!(
        delivered.is_some(),
        "a TV-side toggle_lesson took {:?} and never reached the phone within the 1 s budget",
        sent_at.elapsed()
    );

    // ------------------------------------------------------------------
    // 2. A phone-authed set_school_week reaches the TV within 1 s.
    // ------------------------------------------------------------------
    let parent_session = realtime::session::issue();
    let sent_at = Instant::now();
    let updated = api::set_school_week(BOY_A, 3, today_string(), parent_session.clone())
        .await
        .expect("a parent session finishes the week");
    assert_eq!(updated.current_week, 3, "Finish week moves the pointer");
    let delivered = wait_for(&mut tv, Duration::from_secs(1), |message| {
        matches!(
            message,
            ServerMessage::HomeschoolUpdated { user_ids, week, .. }
                if user_ids.contains(&BOY_A) && *week == 3
        )
    })
    .await;
    assert!(
        delivered.is_some(),
        "a phone-authed set_school_week took {:?} and never reached the TV within the 1 s budget",
        sent_at.elapsed()
    );

    // ------------------------------------------------------------------
    // 3. A Together tick (shared read-aloud) reaches both surfaces.
    // ------------------------------------------------------------------
    let (together_subject, together_assignment, together_date) =
        first_occurrence(BOY_B, 2, "Old Tales").await;
    api::toggle_lesson_together(
        curriculum_id,
        2,
        together_subject,
        together_assignment,
        together_date,
        true,
        today_string(),
        format!("hs7-together-{}", uuid_ish()),
        parent_session.clone(),
    )
    .await
    .expect("the Together tick succeeds");
    let names_both = |message: &ServerMessage| {
        matches!(
            message,
            ServerMessage::HomeschoolUpdated { user_ids, .. }
                if user_ids.contains(&BOY_B) && user_ids.contains(&BOY_C)
        )
    };
    let phone_saw = wait_for(&mut phone, Duration::from_secs(1), names_both).await;
    let tv_saw = wait_for(&mut tv, Duration::from_secs(1), names_both).await;
    assert!(
        phone_saw.is_some(),
        "the Together tick must reach the phone, naming both boys"
    );
    assert!(
        tv_saw.is_some(),
        "the Together tick must reach the tv, naming both boys"
    );

    // ------------------------------------------------------------------
    // 4. Kill and restart the hub; both surfaces resync.
    // ------------------------------------------------------------------
    server.abort();
    let _ = server.await;
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
    async fn reconnect(addr: SocketAddr, deadline: Instant) {
        let mut attempt = 0u32;
        loop {
            assert!(
                Instant::now() < deadline,
                "reconnect exceeded the 30 s budget"
            );
            match tokio_tungstenite::connect_async(format!("ws://{addr}/ws")).await {
                Ok((mut socket, _)) => {
                    expect_hello(&mut socket).await;
                    let _ = socket.close(None).await;
                    return;
                }
                Err(_) => {
                    tokio::time::sleep(backoff(attempt)).await;
                    attempt = attempt.saturating_add(1);
                }
            }
        }
    }

    let deadline = restart_at + Duration::from_secs(30);
    tokio::join!(reconnect(addr, deadline), reconnect(addr, deadline));
    let elapsed = restart_at.elapsed();
    assert!(
        elapsed < Duration::from_secs(30),
        "both surfaces took {elapsed:?} to resync after the restart, over the 30 s budget"
    );

    // The state written before the restart — the TV's tick, the phone's
    // week move, the Together tick — must still be there: the hub's own
    // in-process pool survived, exactly as a real reboot leaves the sqlite
    // file untouched.
    let enrollment = hs::enrollment(pool, BOY_A)
        .await
        .expect("query enrollment")
        .expect("boy A is still enrolled");
    assert_eq!(
        enrollment.current_week, 3,
        "the week move must survive the restart"
    );
    assert_eq!(
        lesson_log_count(pool, BOY_A, sums_subject).await,
        1,
        "the TV's tick must survive the restart"
    );
    assert_eq!(
        lesson_log_count(pool, BOY_B, together_subject).await,
        1,
        "the Together tick must survive the restart for boy B"
    );
    assert_eq!(
        lesson_log_count(pool, BOY_C, together_subject).await,
        1,
        "the Together tick must survive the restart for boy C"
    );

    realtime::session::revoke(&parent_session);
    realtime::session::revoke_all();
    restarted.abort();
}
