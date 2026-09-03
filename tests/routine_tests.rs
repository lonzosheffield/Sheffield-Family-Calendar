//! **T1.5 acceptance suite** — `docs/reviews/PURPLE_TEAM.md` §P3 T1.5:
//! "Date correctness + authorization + missing broadcasts".
//!
//! | # | Assertion | Test |
//! | --- | --- | --- |
//! | 1 | `date = yesterday` writes yesterday's row | `t1_5_1_*` |
//! | 2 | `date = 3 days ago` is rejected, nothing written | `t1_5_2_*` |
//! | 3 | the same idempotency key applied twice = one row change | `t1_5_3_*` |
//! | 4 | user 2 cannot toggle user 3's task (error, no write) | `t1_5_4_*` |
//! | 5 | `toggle_custom_task` emits `TasksUpdated` on a connected client | `t1_5_5_*` |
//! | 6 | a simulated `today()` failure renders the `Error` state | `t1_5_6_*` (unit tests in `src/client/components/routine.rs`) |
//!
//! Also covers the pure `db::date_within_window` boundary and the atomicity
//! of `db::claim_mutation` directly, plus the wire-level (HTTP) versions of
//! 1/2/3/4 that a real client actually exercises.
//!
//! This is its own test binary/harness (mirrors `tests/http_tests.rs` and
//! `tests/realtime_tests.rs` rather than sharing their private helpers,
//! since integration test binaries cannot import from one another) so it is
//! immune to `DATABASE_URL`/`realtime::sender()` state any other test binary
//! sets up — each `cargo test` target is its own process.

#![cfg(feature = "server")]

use std::net::SocketAddr;
use std::time::Duration;

use family_calendar::server::db;
use family_calendar::shared::types::ServerMessage;
use futures_util::StreamExt;
use tokio_tungstenite::tungstenite::Message as WsFrame;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type WsStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

// ---------------------------------------------------------------------------
// Harness (mirrors tests/http_tests.rs::init_test_env / spawn_test_server)
// ---------------------------------------------------------------------------

fn init_test_env() -> std::path::PathBuf {
    static ONCE: std::sync::Once = std::sync::Once::new();
    let base = std::env::temp_dir().join(format!("familyhub-routine-tests-{}", std::process::id()));
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
        std::env::set_var("DIOXUS_PUBLIC_PATH", &public);

        // HS9 (`docs/BACKLOG.md` B-3): this harness — never the shell — pins
        // the data directory, so nothing in this binary can resolve config to
        // the family's live `%ProgramData%\FamilyHub`.
        std::env::set_var("FAMILY_HUB_DATA_DIR", &base);
    });
    base
}

async fn spawn_test_server() -> SocketAddr {
    let base = init_test_env();
    db::pool().await.expect("test sqlite pool opens");

    let config = family_calendar::server::config::FamilyHubConfig {
        data_dir: base,
        http_addr: "127.0.0.1:0".parse().expect("valid socket address"),
        tls_addr: "127.0.0.1:0".parse().expect("valid socket address"),
        screensaver_schedule_hour: None,
        log_level: None,
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

async fn toggle_routine(
    addr: SocketAddr,
    user_id: u32,
    template_id: u32,
    completed: bool,
    date: &str,
    idempotency_key: &str,
) -> reqwest::Response {
    http_client()
        .post(format!("http://{addr}/api/toggle_routine_task"))
        .header("content-type", "application/json")
        .body(format!(
            r#"{{"user_id":{user_id},"template_id":{template_id},"completed":{completed},"date":"{date}","idempotency_key":"{idempotency_key}"}}"#
        ))
        .send()
        .await
        .expect("the toggle_routine_task endpoint should respond")
}

async fn toggle_custom(
    addr: SocketAddr,
    user_id: u32,
    task_id: u32,
    completed: bool,
    date: &str,
    idempotency_key: &str,
) -> reqwest::Response {
    http_client()
        .post(format!("http://{addr}/api/toggle_custom_task"))
        .header("content-type", "application/json")
        .body(format!(
            r#"{{"user_id":{user_id},"task_id":{task_id},"completed":{completed},"date":"{date}","idempotency_key":"{idempotency_key}"}}"#
        ))
        .send()
        .await
        .expect("the toggle_custom_task endpoint should respond")
}

fn today_string() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn days_ago(n: i64) -> String {
    (chrono::Local::now().date_naive() - chrono::Duration::days(n))
        .format("%Y-%m-%d")
        .to_string()
}

async fn recv_matching(socket: &mut WsStream, marker: &str) -> String {
    let wait = async {
        loop {
            match socket.next().await {
                Some(Ok(WsFrame::Text(text))) if text.contains(marker) => {
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

/// Read frames until `predicate` matches a parsed `ServerMessage`, skipping
/// anything else. `realtime::sender()` is one process-wide broadcast channel
/// shared by every test in this binary running concurrently, so a raw
/// substring match on `"tasks_updated"` alone is not reliable — this test
/// file has more than one `toggle_custom_task` caller in flight at once, and
/// a bare marker would happily accept a *different* test's broadcast. A full
/// structural predicate is what actually proves *this* mutation was the one
/// that published.
async fn wait_for(
    socket: &mut WsStream,
    within: Duration,
    predicate: impl Fn(&ServerMessage) -> bool,
) -> Option<ServerMessage> {
    let wait = async {
        loop {
            match socket.next().await {
                Some(Ok(WsFrame::Text(text))) => {
                    if let Ok(message) = serde_json::from_str::<ServerMessage>(&text) {
                        if predicate(&message) {
                            return Some(message);
                        }
                    }
                }
                Some(Ok(_)) => continue,
                Some(Err(_)) | None => return None,
            }
        }
    };
    tokio::time::timeout(within, wait).await.ok().flatten()
}

// ---------------------------------------------------------------------------
// Pure db::date_within_window boundary
// ---------------------------------------------------------------------------

#[test]
fn t1_5_pure_date_within_window_accepts_yesterday_today_and_tomorrow() {
    assert!(db::date_within_window("2026-08-28", "2026-08-29"));
    assert!(db::date_within_window("2026-08-29", "2026-08-29"));
    assert!(db::date_within_window("2026-08-30", "2026-08-29"));
}

#[test]
fn t1_5_pure_date_within_window_rejects_three_days_either_way() {
    assert!(!db::date_within_window("2026-08-26", "2026-08-29"));
    assert!(!db::date_within_window("2026-09-01", "2026-08-29"));
}

#[test]
fn t1_5_pure_date_within_window_rejects_malformed_input() {
    assert!(!db::date_within_window("not-a-date", "2026-08-29"));
    assert!(!db::date_within_window("2026-08-29", "not-a-date"));
    assert!(!db::date_within_window("", ""));
}

// ---------------------------------------------------------------------------
// Pure db::claim_mutation atomicity (direct DB test, no HTTP)
// ---------------------------------------------------------------------------

async fn memory_pool() -> sqlx::SqlitePool {
    let pool = db::connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite");
    db::migrate(&pool).await.expect("migrations");
    pool
}

#[tokio::test]
async fn t1_5_claim_mutation_is_true_once_and_false_on_every_replay() {
    let pool = memory_pool().await;

    let first = db::claim_mutation(&pool, "key-1", "test_kind", 1, "{}")
        .await
        .expect("first claim");
    assert!(first, "the first delivery of a key must be claimed");

    for _ in 0..5 {
        let replay = db::claim_mutation(&pool, "key-1", "test_kind", 1, "{}")
            .await
            .expect("replayed claim");
        assert!(!replay, "a replayed key must never be claimed twice");
    }

    // A different key is independent.
    let other = db::claim_mutation(&pool, "key-2", "test_kind", 1, "{}")
        .await
        .expect("a different key claims independently");
    assert!(other);
}

// ---------------------------------------------------------------------------
// Q1-08 — a failed (FK) write releases its claim for the next delivery
// ---------------------------------------------------------------------------

/// QA round 1 (Q1-08): `claim_mutation` used to commit in its own statement
/// *before* the write it guards ran. A write that then failed — here, a
/// foreign-key violation from an unknown `user_id` — left the key claimed
/// forever, so the *same* key, replayed afterward against a real profile,
/// saw "already claimed" and silently did nothing while `toggle_routine_task`
/// still reported success (the "200/`null` instead of 500" flake HANDOFF
/// recorded at the wave-3 close). The fix runs the claim and the write in one
/// transaction, so this failed write must roll the claim back too — this
/// test proves that end to end, through the real HTTP endpoint, not just at
/// the `db::claim_mutation` layer.
#[tokio::test]
async fn t1_5_q1_08_a_failed_fk_claim_releases_its_key_for_a_valid_users_replay() {
    let addr = spawn_test_server().await;
    let pool = db::pool().await.expect("test sqlite pool opens");
    let date = today_string();

    // A profile no other test in this binary writes a "today" routine
    // completion for, so this test's assertions cannot race another's.
    const VALID_USER: u32 = 2;
    let key = format!("t1-5-q1-08-{}", uuid_ish());

    let template_id = db::daily_routine(pool, VALID_USER, &date)
        .await
        .expect("seeded routine is readable")
        .first()
        .expect("the morning routine is seeded")
        .template_id;
    db::set_routine_completion(pool, VALID_USER, template_id, false, &date)
        .await
        .expect("clear this test's own row before asserting on it");

    // First delivery: `user_id = 99` has no matching `profiles` row, so the
    // write inside the transaction fails its foreign key and the whole
    // transaction — claim included — must roll back.
    let failed = toggle_routine(addr, 99, template_id, true, &date, &key).await;
    assert_eq!(
        failed.status().as_u16(),
        500,
        "an unknown user_id must fail, body: {:?}",
        failed.text().await
    );

    // Second delivery, the exact same key, a real profile this time: if the
    // first call's claim had stuck (the pre-fix bug), this would see
    // "already claimed" and return 200 while writing nothing.
    let applied = toggle_routine(addr, VALID_USER, template_id, true, &date, &key).await;
    assert_eq!(
        applied.status().as_u16(),
        200,
        "the same key must be free to apply once for a valid user, body: {:?}",
        applied.text().await
    );

    let items = db::daily_routine(pool, VALID_USER, &date).await.unwrap();
    assert!(
        items
            .iter()
            .find(|i| i.template_id == template_id)
            .unwrap()
            .completed,
        "the second (valid-user) delivery of the same key must have actually applied"
    );
}

// ---------------------------------------------------------------------------
// 1 — date = yesterday writes yesterday's row
// ---------------------------------------------------------------------------

#[tokio::test]
async fn t1_5_1_toggling_with_yesterdays_date_writes_yesterdays_row() {
    let addr = spawn_test_server().await;
    let pool = db::pool().await.expect("test sqlite pool opens");

    const USER_ID: u32 = 1;
    let yesterday = days_ago(1);
    let template_id = db::daily_routine(pool, USER_ID, &yesterday)
        .await
        .expect("seeded routine is readable")
        .first()
        .expect("the morning routine is seeded")
        .template_id;

    // Known starting state on both days.
    db::set_routine_completion(pool, USER_ID, template_id, false, &yesterday)
        .await
        .expect("clear yesterday");
    db::set_routine_completion(pool, USER_ID, template_id, false, &today_string())
        .await
        .expect("clear today");

    let response = toggle_routine(
        addr,
        USER_ID,
        template_id,
        true,
        &yesterday,
        "t1-5-1-yesterday",
    )
    .await;
    assert_eq!(
        response.status().as_u16(),
        200,
        "body: {:?}",
        response.text().await
    );

    let yesterday_items = db::daily_routine(pool, USER_ID, &yesterday).await.unwrap();
    assert!(
        yesterday_items
            .iter()
            .find(|i| i.template_id == template_id)
            .unwrap()
            .completed,
        "the completion must land on yesterday's row"
    );

    let today_items = db::daily_routine(pool, USER_ID, &today_string())
        .await
        .unwrap();
    assert!(
        !today_items
            .iter()
            .find(|i| i.template_id == template_id)
            .unwrap()
            .completed,
        "today's row must be untouched by a mutation explicitly dated yesterday"
    );
}

// ---------------------------------------------------------------------------
// 2 — date = 3 days ago is rejected
// ---------------------------------------------------------------------------

#[tokio::test]
async fn t1_5_2_toggling_three_days_ago_is_rejected_and_writes_nothing() {
    let addr = spawn_test_server().await;
    let pool = db::pool().await.expect("test sqlite pool opens");

    const USER_ID: u32 = 2;
    let three_days_ago = days_ago(3);
    let template_id = db::daily_routine(pool, USER_ID, &three_days_ago)
        .await
        .expect("seeded routine is readable")
        .first()
        .expect("the morning routine is seeded")
        .template_id;

    let response = toggle_routine(
        addr,
        USER_ID,
        template_id,
        true,
        &three_days_ago,
        "t1-5-2-three-days-ago",
    )
    .await;
    assert_eq!(
        response.status().as_u16(),
        500,
        "a date outside the ±1 day window must be rejected"
    );

    let items = db::daily_routine(pool, USER_ID, &three_days_ago)
        .await
        .unwrap();
    assert!(
        !items
            .iter()
            .find(|i| i.template_id == template_id)
            .unwrap()
            .completed,
        "a rejected mutation must write nothing"
    );
}

// ---------------------------------------------------------------------------
// 3 — the same idempotency key applied twice = one row change
// ---------------------------------------------------------------------------

#[tokio::test]
async fn t1_5_3_the_same_idempotency_key_replayed_produces_one_row_change() {
    let addr = spawn_test_server().await;
    let pool = db::pool().await.expect("test sqlite pool opens");
    let date = today_string();

    const USER_ID: u32 = 3;
    let title = format!("t1.5.3 task {}", uuid_ish());
    let task_id = db::insert_custom_task(pool, USER_ID, &title, None)
        .await
        .expect("insert task");

    let key = format!("t1-5-3-{}", uuid_ish());

    // First delivery: completes the task.
    let first = toggle_custom(addr, USER_ID, task_id, true, &date, &key).await;
    assert_eq!(
        first.status().as_u16(),
        200,
        "body: {:?}",
        first.text().await
    );

    // A replay of the *same* key, this time asking to un-complete it — since
    // the key was already claimed, this must be a no-op, not a second flip.
    let replay = toggle_custom(addr, USER_ID, task_id, false, &date, &key).await;
    assert_eq!(
        replay.status().as_u16(),
        200,
        "replay must still report success"
    );

    let tasks = db::custom_tasks(pool, USER_ID).await.unwrap();
    let task = tasks.iter().find(|t| t.id == task_id).unwrap();
    assert!(
        task.is_completed,
        "the replayed request must not have re-applied — the row stays at its \
         one real change (completed=true), not flipped back to false"
    );
}

fn uuid_ish() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// 4 — user 2 cannot toggle user 3's task
// ---------------------------------------------------------------------------

#[tokio::test]
async fn t1_5_4_a_profile_cannot_toggle_another_profiles_custom_task() {
    let addr = spawn_test_server().await;
    let pool = db::pool().await.expect("test sqlite pool opens");
    let date = today_string();

    const OWNER: u32 = 3;
    const ATTACKER: u32 = 2;
    let title = format!("t1.5.4 task {}", uuid_ish());
    let task_id = db::insert_custom_task(pool, OWNER, &title, None)
        .await
        .expect("insert task");

    let response = toggle_custom(
        addr,
        ATTACKER,
        task_id,
        true,
        &date,
        &format!("t1-5-4-{}", uuid_ish()),
    )
    .await;
    assert_eq!(
        response.status().as_u16(),
        500,
        "a cross-profile toggle must be rejected, body: {:?}",
        response.text().await
    );

    let tasks = db::custom_tasks(pool, OWNER).await.unwrap();
    let task = tasks.iter().find(|t| t.id == task_id).unwrap();
    assert!(
        !task.is_completed,
        "the rejected cross-profile toggle must not have written anything"
    );
}

// ---------------------------------------------------------------------------
// 5 — toggle_custom_task emits TasksUpdated on a connected client
// ---------------------------------------------------------------------------

#[tokio::test]
async fn t1_5_5_toggle_custom_task_publishes_tasks_updated() {
    let addr = spawn_test_server().await;
    let pool = db::pool().await.expect("test sqlite pool opens");
    let date = today_string();

    const USER_ID: u32 = 4;
    let title = format!("t1.5.5 task {}", uuid_ish());
    let task_id = db::insert_custom_task(pool, USER_ID, &title, None)
        .await
        .expect("insert task");

    let (mut socket, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .expect("websocket upgrade");
    // Drain the Hello frame.
    recv_matching(&mut socket, "\"hello\"").await;

    let response = toggle_custom(
        addr,
        USER_ID,
        task_id,
        true,
        &date,
        &format!("t1-5-5-{}", uuid_ish()),
    )
    .await;
    assert_eq!(
        response.status().as_u16(),
        200,
        "body: {:?}",
        response.text().await
    );

    // G22/W1: the v1 endpoint never broadcast anything for this mutation.
    // Filtered on the exact `user_id` (not just "any TasksUpdated"):
    // `realtime::sender()` is one process-wide channel and another test in
    // this binary may be toggling a *different* profile's task at the same
    // moment.
    let expected_date = date.clone();
    let matched = wait_for(&mut socket, Duration::from_secs(5), |message| {
        matches!(
            message,
            ServerMessage::TasksUpdated { user_id, date } if *user_id == i64::from(USER_ID) && *date == expected_date
        )
    })
    .await;

    match matched {
        Some(ServerMessage::TasksUpdated {
            user_id,
            date: got_date,
        }) => {
            assert_eq!(user_id, i64::from(USER_ID));
            assert_eq!(got_date, date);
        }
        other => panic!(
            "expected a TasksUpdated{{user_id: {USER_ID}, date: {date:?}}} within 5s, got {other:?}"
        ),
    }
}
