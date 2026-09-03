//! **HS4 acceptance suite** — `docs/homeschool/PLAN_HOMESCHOOL.md` §3 row
//! HS4: the School tab's server functions.
//!
//! | # | Assertion | Test(s) |
//! | --- | --- | --- |
//! | a | `date = yesterday` writes yesterday's row; `date = 3 days ago` rejected | `hs4_a_*` |
//! | b | the same idempotency key applied twice is one change | `hs4_b_*` |
//! | c | each `auth` fn without a cookie errors; with one, 200 + a broadcast on a second WS client within 1 s | `hs4_c_*` |
//! | d | `toggle_lesson` needs no cookie | `hs4_d_*` |
//! | e | an occurrence not in the boy's current week is rejected; an unenrolled boy is rejected | `hs4_e_*` |
//! | f | `toggle_lesson_together` on two boys sharing a week writes exactly two rows | `hs4_f_*` |
//! | g | `set_school_week` reaches `weeks + 1` (year complete) and Back returns to `weeks` | `hs4_g_*` |
//! | h | `mark_all_done` ticks only the unticked and is idempotent | `hs4_h_*` |
//! | i | `set_subject_schedule(days = "Th")` errors and writes nothing | `hs4_i_*` |
//! | j | `get_homeschool_today` with nobody enrolled / paused | `hs4_j_*` |
//! | k | `add_extra` / `toggle_extra` / `delete_extra` authorization and date rules | `hs4_k_*` |
//! | l | `get_week_grid` / `get_month` boundary rules | `hs4_l_*` |
//! | m | `toggle_lesson` with `subject_id <= 0` is rejected before any write | `hs4_m_*` |
//!
//! Harness follows `tests/routine_tests.rs` (a throwaway sqlite file per test
//! process, the real production router on an ephemeral port for the WS
//! assertions) and borrows `tests/profiles_tests.rs`'s `parent_session()`
//! shape for a real, in-process-usable `SessionToken`. Every server function
//! is called **directly** — the real `#[cfg(feature = "server")]` body
//! running in-process, not a network round trip — except the two broadcast
//! checks (c), which go over real HTTP + a live `/ws` client the same way
//! `tests/routine_tests.rs::t1_5_5_*` does.
//!
//! Every test in this file holds [`hs4_lock`] for its duration: there are
//! only four seeded profiles (`migrations/0004_name_the_boys.sql`) and every
//! enrollment/log table is shared process-wide, so serialising is simpler and
//! safer than trying to keep 13 accept letters' worth of scenarios on
//! disjoint profiles.
//!
//! **§0 N1.** Every curriculum string here is the committed, invented
//! `tests/fixtures/curricula/sample-year.toml` — nothing from
//! `docs/homeschool/curriculum/`.

#![cfg(feature = "server")]

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use family_calendar::server::api::homeschool as api;
use family_calendar::server::auth;
use family_calendar::server::config::FamilyHubConfig;
use family_calendar::server::db;
use family_calendar::server::homeschool::db as hs;
use family_calendar::server::homeschool::loader;
use family_calendar::shared::homeschool::{Category, LogStatus};
use family_calendar::shared::types::{DayItem, ServerMessage};
use futures_util::StreamExt;
use sqlx::SqlitePool;
use tokio_tungstenite::tungstenite::Message as WsFrame;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type WsStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

// ---------------------------------------------------------------------------
// Harness (mirrors tests/routine_tests.rs::init_test_env / spawn_test_server)
// ---------------------------------------------------------------------------

fn init_test_env() -> PathBuf {
    static ONCE: std::sync::Once = std::sync::Once::new();
    let base = std::env::temp_dir().join(format!("familyhub-hs4-tests-{}", std::process::id()));
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

        let public = base.join("public");
        std::fs::create_dir_all(&public).expect("test public directory is creatable");
        std::env::set_var("DIOXUS_PUBLIC_PATH", &public);
    });
    base
}

async fn spawn_test_server() -> SocketAddr {
    let base = init_test_env();
    db::pool().await.expect("test sqlite pool opens");

    let config = FamilyHubConfig {
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

/// Every test in this binary shares the four seeded profiles, the
/// `enrollments`/`lesson_log`/`lesson_extras` tables and `realtime::sender()`
/// — this lock (mirrors `tests/calendar_tests.rs::calendar_lock`) makes the
/// suite effectively single-threaded so no test's setup or teardown can land
/// mid-assertion in another's.
///
/// **Calls [`init_test_env`] first, unconditionally.** `db::pool()`'s pool is
/// a process-wide `OnceCell`: whichever call reaches it *first* decides the
/// database for the rest of this test binary's run, env var included. Every
/// test here takes this lock before touching `db::pool()`, so routing every
/// caller through `init_test_env()` on the way in is what guarantees
/// `DATABASE_URL` is this binary's own scratch file — never the real
/// `FamilyHubConfig` default — no matter which test happens to run first.
async fn hs4_lock() -> tokio::sync::MutexGuard<'static, ()> {
    init_test_env();
    static LOCK: tokio::sync::OnceCell<tokio::sync::Mutex<()>> = tokio::sync::OnceCell::const_new();
    LOCK.get_or_init(|| async { tokio::sync::Mutex::new(()) })
        .await
        .lock()
        .await
}

/// A valid parent [`SessionToken`] — runs the real first-run flow exactly
/// once for this process (mirrors `tests/profiles_tests.rs::parent_session`).
async fn parent_session() -> String {
    static SESSION: tokio::sync::OnceCell<String> = tokio::sync::OnceCell::const_new();
    SESSION
        .get_or_init(|| async {
            init_test_env();
            let pool = db::pool().await.expect("test sqlite pool opens");
            let dir = FamilyHubConfig::load().data_dir;

            auth::ensure_setup_code(pool, &dir)
                .await
                .expect("generate the first-run setup code");
            let code = auth::read_setup_code(pool)
                .await
                .expect("read the setup code")
                .expect("a setup code exists before any PIN is set");

            auth::set_initial_pin(pool, &dir, &code, "246810")
                .await
                .expect("set the initial parent PIN")
        })
        .await
        .clone()
}

fn today_string() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn days_ago(n: i64) -> String {
    (chrono::Local::now().date_naive() - chrono::Duration::days(n))
        .format("%Y-%m-%d")
        .to_string()
}

fn uuid_ish() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn fixture_path() -> PathBuf {
    repo_root().join("tests/fixtures/curricula/sample-year.toml")
}

/// Load the committed synthetic fixture (idempotent: `insert_missing` never
/// duplicates), returning its `curriculum_id`.
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

/// Wipe every mutable homeschool table so a test starts from a known-empty
/// state, without re-loading the (idempotent) curriculum rows.
async fn reset_homeschool_state(pool: &SqlitePool) {
    for table in ["lesson_log", "lesson_extras", "enrollments"] {
        sqlx::query(&format!("DELETE FROM {table}"))
            .execute(pool)
            .await
            .unwrap_or_else(|err| panic!("clear {table}: {err}"));
    }
}

/// Enroll a boy directly through storage (bypassing the parent-gated
/// `api::homeschool::enroll` server fn, which this suite tests separately) —
/// every test that just needs "a boy enrolled" uses this.
async fn enroll_direct(
    pool: &SqlitePool,
    profile_id: i64,
    curriculum_id: i64,
    week: i64,
    school_days: &str,
    week_started_on: &str,
) {
    hs::upsert_enrollment(
        pool,
        profile_id,
        curriculum_id,
        week,
        school_days,
        week_started_on,
    )
    .await
    .expect("direct enrollment");
}

/// Widen a subject's `days` to all seven letters (test-only DB write, not
/// through the parent-gated server fn) so it occurs on **every** date of an
/// enrollment's span — the test's own `week_started_on` is arbitrary and
/// unrelated to the real calendar, so this is what makes an occurrence's
/// `scheduled_date` fully deterministic regardless of which real weekday the
/// suite happens to run on. `shared` must be passed explicitly (not read
/// back first) so a caller widening a `reading`/`weekly` subject for a
/// Together test keeps its `shared = true`.
async fn widen_to_every_day(pool: &SqlitePool, subject_id: i64, shared: bool) {
    hs::set_subject_schedule(pool, subject_id, "MTWRFSU", shared)
        .await
        .expect("widen subject days for the test");
}

/// The first occurrence of `subject_name` in `user_id`'s week-`week` grid —
/// `(subject_id, assignment_id, scheduled_date)`, exactly the triple
/// `toggle_lesson` validates against.
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

async fn log_row_count(pool: &SqlitePool, profile_id: i64) -> i64 {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM lesson_log WHERE profile_id = ?1")
        .bind(profile_id)
        .fetch_one(pool)
        .await
        .expect("count lesson_log");
    row.0
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
// (a) date = yesterday writes yesterday's row; date = 3 days ago is rejected
// ---------------------------------------------------------------------------

#[tokio::test]
async fn hs4_a_toggling_with_yesterdays_date_writes_yesterdays_row_and_three_days_ago_is_rejected()
{
    let _guard = hs4_lock().await;
    let pool = db::pool().await.expect("pool");
    reset_homeschool_state(pool).await;
    let curriculum_id = load_fixture(pool).await;
    let sums = subject_id(pool, curriculum_id, "Sums").await;
    widen_to_every_day(pool, sums, false).await;

    const BOY: i64 = 1;
    // An anchor unrelated to the real calendar: only the *mutation's* `date`
    // needs to sit near real "today" (the ±1 day window); the occurrence's
    // own `scheduled_date` is validated purely against this enrollment's own
    // recomputed week, which `week_started_on` fully determines.
    enroll_direct(pool, BOY, curriculum_id, 1, "MTWRFSU", "2026-01-05").await;
    let (subject, assignment, scheduled_date) = first_occurrence(BOY, 1, "Sums").await;

    let yesterday = days_ago(1);
    let ok = api::toggle_lesson(
        BOY,
        subject,
        assignment,
        1,
        scheduled_date.clone(),
        true,
        LogStatus::Done,
        None,
        yesterday.clone(),
        format!("hs4-a-{}", uuid_ish()),
    )
    .await;
    assert!(ok.is_ok(), "yesterday must be inside the window: {ok:?}");

    let (_, completed_on): (String, String) = sqlx::query_as(
        "SELECT scheduled_date, completed_on FROM lesson_log \
         WHERE profile_id = ?1 AND subject_id = ?2 AND scheduled_date = ?3",
    )
    .bind(BOY)
    .bind(subject)
    .bind(&scheduled_date)
    .fetch_one(pool)
    .await
    .expect("the row was written");
    assert_eq!(
        completed_on, yesterday,
        "completed_on must carry the mutation's own date, not today's"
    );

    let three_days_ago = days_ago(3);
    let rejected = api::toggle_lesson(
        BOY,
        subject,
        assignment,
        1,
        scheduled_date,
        true,
        LogStatus::Done,
        None,
        three_days_ago,
        format!("hs4-a-{}", uuid_ish()),
    )
    .await;
    assert!(
        rejected.is_err(),
        "a date outside the ±1 day window must be rejected"
    );
}

// ---------------------------------------------------------------------------
// (b) the same idempotency key applied twice is one change
// ---------------------------------------------------------------------------

#[tokio::test]
async fn hs4_b_the_same_idempotency_key_replayed_produces_one_row_change() {
    let _guard = hs4_lock().await;
    let pool = db::pool().await.expect("pool");
    reset_homeschool_state(pool).await;
    let curriculum_id = load_fixture(pool).await;
    let sums = subject_id(pool, curriculum_id, "Sums").await;
    widen_to_every_day(pool, sums, false).await;

    const BOY: i64 = 2;
    enroll_direct(pool, BOY, curriculum_id, 1, "MTWRFSU", "2026-01-05").await;
    let (subject, assignment, scheduled_date) = first_occurrence(BOY, 1, "Sums").await;

    let key = format!("hs4-b-{}", uuid_ish());
    let today = today_string();

    api::toggle_lesson(
        BOY,
        subject,
        assignment,
        1,
        scheduled_date.clone(),
        true,
        LogStatus::Done,
        None,
        today.clone(),
        key.clone(),
    )
    .await
    .expect("first delivery");
    assert_eq!(log_row_count(pool, BOY).await, 1);

    // A replay of the exact same key, asking to untick — must be a no-op.
    api::toggle_lesson(
        BOY,
        subject,
        assignment,
        1,
        scheduled_date,
        false,
        LogStatus::Done,
        None,
        today,
        key,
    )
    .await
    .expect("replay reports success without re-applying");
    assert_eq!(
        log_row_count(pool, BOY).await,
        1,
        "the replay must not have unticked the row"
    );
}

// ---------------------------------------------------------------------------
// (c) auth funcs: no cookie -> error; a session -> 200 + a broadcast
// ---------------------------------------------------------------------------

#[tokio::test]
async fn hs4_c_set_paused_without_a_session_errors_and_with_one_broadcasts_homeschool_updated() {
    let _guard = hs4_lock().await;
    let addr = spawn_test_server().await;
    let pool = db::pool().await.expect("pool");
    reset_homeschool_state(pool).await;
    let curriculum_id = load_fixture(pool).await;

    const BOY: i64 = 3;
    enroll_direct(pool, BOY, curriculum_id, 1, "MTWRF", "2026-01-05").await;

    // No cookie, no bearer token: the in-process direct call has no live
    // request underneath it, so the cookie fallback always fails closed.
    let denied = api::set_paused(BOY, true, String::new()).await;
    assert!(denied.is_err(), "an empty auth must be rejected");

    let token = parent_session().await;

    let (mut socket, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .expect("websocket upgrade");
    recv_matching(&mut socket, "\"hello\"").await;

    let response = http_client()
        .post(format!("http://{addr}/api/set_paused"))
        .header("content-type", "application/json")
        .body(format!(
            r#"{{"user_id":{BOY},"paused":true,"auth":"{token}"}}"#
        ))
        .send()
        .await
        .expect("set_paused responds");
    assert_eq!(
        response.status().as_u16(),
        200,
        "body: {:?}",
        response.text().await
    );

    let matched = wait_for(&mut socket, Duration::from_secs(1), |message| {
        matches!(
            message,
            ServerMessage::HomeschoolUpdated { user_ids, .. } if user_ids.contains(&BOY)
        )
    })
    .await;
    assert!(
        matched.is_some(),
        "HomeschoolUpdated must reach a second client within 1s"
    );
}

#[tokio::test]
async fn hs4_c_set_subject_schedule_without_a_session_errors_and_with_one_broadcasts_curriculum_updated(
) {
    let _guard = hs4_lock().await;
    let addr = spawn_test_server().await;
    let pool = db::pool().await.expect("pool");
    let curriculum_id = load_fixture(pool).await;
    let painting = subject_id(pool, curriculum_id, "Painting").await;

    let denied = api::set_subject_schedule(painting, "MW".to_string(), true, String::new()).await;
    assert!(denied.is_err(), "an empty auth must be rejected");

    let token = parent_session().await;

    let (mut socket, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .expect("websocket upgrade");
    recv_matching(&mut socket, "\"hello\"").await;

    let response = http_client()
        .post(format!("http://{addr}/api/set_subject_schedule"))
        .header("content-type", "application/json")
        .body(format!(
            r#"{{"subject_id":{painting},"days":"MW","shared":true,"auth":"{token}"}}"#
        ))
        .send()
        .await
        .expect("set_subject_schedule responds");
    assert_eq!(
        response.status().as_u16(),
        200,
        "body: {:?}",
        response.text().await
    );

    let matched = wait_for(&mut socket, Duration::from_secs(1), |message| {
        matches!(
            message,
            ServerMessage::CurriculumUpdated { curriculum_id: id } if *id == curriculum_id
        )
    })
    .await;
    assert!(
        matched.is_some(),
        "CurriculumUpdated must reach a second client within 1s"
    );

    // Put it back so later tests see the fixture's own shape.
    hs::set_subject_schedule(pool, painting, "F", false)
        .await
        .expect("restore Painting's days");
}

// ---------------------------------------------------------------------------
// (d) toggle_lesson needs no cookie
// ---------------------------------------------------------------------------

#[tokio::test]
async fn hs4_d_toggle_lesson_succeeds_with_no_cookie_at_all() {
    let _guard = hs4_lock().await;
    let pool = db::pool().await.expect("pool");
    reset_homeschool_state(pool).await;
    let curriculum_id = load_fixture(pool).await;
    let sums = subject_id(pool, curriculum_id, "Sums").await;
    widen_to_every_day(pool, sums, false).await;

    const BOY: i64 = 4;
    enroll_direct(pool, BOY, curriculum_id, 1, "MTWRFSU", "2026-01-05").await;
    let (subject, assignment, scheduled_date) = first_occurrence(BOY, 1, "Sums").await;

    // toggle_lesson has no `auth` parameter at all — anyone on the LAN.
    let ok = api::toggle_lesson(
        BOY,
        subject,
        assignment,
        1,
        scheduled_date,
        true,
        LogStatus::Done,
        None,
        today_string(),
        format!("hs4-d-{}", uuid_ish()),
    )
    .await;
    assert!(ok.is_ok(), "no cookie should be needed: {ok:?}");
    assert_eq!(log_row_count(pool, BOY).await, 1);
}

// ---------------------------------------------------------------------------
// (e) a triple outside the current week's occurrences is rejected; so is an
//     unenrolled boy
// ---------------------------------------------------------------------------

#[tokio::test]
async fn hs4_e_an_occurrence_outside_the_current_week_and_an_unenrolled_boy_are_both_rejected() {
    let _guard = hs4_lock().await;
    let pool = db::pool().await.expect("pool");
    reset_homeschool_state(pool).await;
    let curriculum_id = load_fixture(pool).await;
    let sums = subject_id(pool, curriculum_id, "Sums").await;

    const ENROLLED: i64 = 1;
    const UNENROLLED: i64 = 2;
    enroll_direct(pool, ENROLLED, curriculum_id, 1, "MTWRF", "2026-01-05").await;

    // A scheduled_date nothing in the current week could ever produce.
    let bogus = api::toggle_lesson(
        ENROLLED,
        sums,
        None,
        1,
        "2099-12-31".to_string(),
        true,
        LogStatus::Done,
        None,
        today_string(),
        format!("hs4-e-{}", uuid_ish()),
    )
    .await;
    assert!(bogus.is_err(), "an unscheduled occurrence must be rejected");
    assert_eq!(log_row_count(pool, ENROLLED).await, 0);

    let (subject, assignment, scheduled_date) = first_occurrence(ENROLLED, 1, "Sums").await;
    let not_enrolled = api::toggle_lesson(
        UNENROLLED,
        subject,
        assignment,
        1,
        scheduled_date,
        true,
        LogStatus::Done,
        None,
        today_string(),
        format!("hs4-e-{}", uuid_ish()),
    )
    .await;
    assert!(
        not_enrolled.is_err(),
        "an unenrolled boy must be rejected, not silently accepted"
    );
    assert_eq!(log_row_count(pool, UNENROLLED).await, 0);
}

// ---------------------------------------------------------------------------
// (f) toggle_lesson_together writes exactly the matched boys' rows
// ---------------------------------------------------------------------------

#[tokio::test]
async fn hs4_f_toggle_lesson_together_writes_exactly_the_two_boys_sharing_the_week() {
    let _guard = hs4_lock().await;
    let pool = db::pool().await.expect("pool");
    reset_homeschool_state(pool).await;
    let curriculum_id = load_fixture(pool).await;
    let old_tales = subject_id(pool, curriculum_id, "Old Tales").await;
    widen_to_every_day(pool, old_tales, true).await;

    const TOGETHER_A: i64 = 1;
    const TOGETHER_B: i64 = 2;
    const SOLO: i64 = 3;
    enroll_direct(pool, TOGETHER_A, curriculum_id, 2, "MTWRFSU", "2026-01-05").await;
    enroll_direct(pool, TOGETHER_B, curriculum_id, 2, "MTWRFSU", "2026-01-05").await;
    enroll_direct(pool, SOLO, curriculum_id, 1, "MTWRFSU", "2026-01-05").await;

    let (subject, assignment, scheduled_date) = first_occurrence(TOGETHER_A, 2, "Old Tales").await;

    let token = parent_session().await;
    api::toggle_lesson_together(
        curriculum_id,
        2,
        subject,
        assignment,
        scheduled_date,
        true,
        today_string(),
        format!("hs4-f-{}", uuid_ish()),
        token,
    )
    .await
    .expect("the Together tick succeeds");

    assert_eq!(log_row_count(pool, TOGETHER_A).await, 1);
    assert_eq!(log_row_count(pool, TOGETHER_B).await, 1);
    assert_eq!(
        log_row_count(pool, SOLO).await,
        0,
        "a boy on a different week must be untouched"
    );
}

// ---------------------------------------------------------------------------
// (g) set_school_week reaches weeks + 1 (year complete) and Back returns
// ---------------------------------------------------------------------------

#[tokio::test]
async fn hs4_g_set_school_week_reaches_year_complete_and_back_returns_to_the_last_week() {
    let _guard = hs4_lock().await;
    let pool = db::pool().await.expect("pool");
    reset_homeschool_state(pool).await;
    let curriculum_id = load_fixture(pool).await;

    const BOY: i64 = 1;
    // The fixture has 3 weeks.
    enroll_direct(pool, BOY, curriculum_id, 3, "MTWRF", "2026-03-02").await;
    let token = parent_session().await;

    let finished = api::set_school_week(BOY, 4, days_ago(0), token.clone())
        .await
        .expect("finishing the last week must succeed");
    assert_eq!(finished.current_week, 4, "weeks + 1 is the terminal state");
    assert!(
        finished.current_week > finished.weeks,
        "current_week > weeks is Year complete (H2)"
    );
    assert_eq!(finished.week_started_on, days_ago(0));

    let back = api::set_school_week(BOY, 3, days_ago(0), token)
        .await
        .expect("Back a week must succeed from the terminal state");
    assert_eq!(back.current_week, 3);
    assert!(!(back.current_week > back.weeks));
}

// ---------------------------------------------------------------------------
// (h) mark_all_done ticks only the unticked and is idempotent
// ---------------------------------------------------------------------------

#[tokio::test]
async fn hs4_h_mark_all_done_ticks_only_unticked_items_and_is_idempotent() {
    let _guard = hs4_lock().await;
    let pool = db::pool().await.expect("pool");
    reset_homeschool_state(pool).await;
    let curriculum_id = load_fixture(pool).await;
    let sums = subject_id(pool, curriculum_id, "Sums").await;
    let copywork = subject_id(pool, curriculum_id, "Copywork").await;
    widen_to_every_day(pool, sums, false).await;
    widen_to_every_day(pool, copywork, false).await;

    const BOY: i64 = 2;
    enroll_direct(pool, BOY, curriculum_id, 1, "MTWRFSU", "2026-01-05").await;

    // Tick one of the two due/catch-up occurrences by hand first, so the
    // test can prove mark_all_done leaves it alone rather than re-writing it.
    let (subject, assignment, scheduled_date) = first_occurrence(BOY, 1, "Sums").await;
    let date = today_string();
    api::toggle_lesson(
        BOY,
        subject,
        assignment,
        1,
        scheduled_date,
        true,
        LogStatus::Skipped,
        Some("already logged by hand".to_string()),
        date.clone(),
        format!("hs4-h-pre-{}", uuid_ish()),
    )
    .await
    .expect("pre-tick");
    let before = log_row_count(pool, BOY).await;
    assert_eq!(before, 1);

    api::mark_all_done(BOY, 1, date.clone(), format!("hs4-h-{}", uuid_ish()))
        .await
        .expect("mark_all_done");
    let after_first = log_row_count(pool, BOY).await;
    assert!(
        after_first > before,
        "mark_all_done must tick the remaining unticked items"
    );

    // The hand-ticked row must still read 'skipped' — mark_all_done never
    // touches an item that already has a status.
    let (status,): (String,) =
        sqlx::query_as("SELECT status FROM lesson_log WHERE profile_id = ?1 AND subject_id = ?2")
            .bind(BOY)
            .bind(subject)
            .fetch_one(pool)
            .await
            .expect("the hand-ticked row");
    assert_eq!(status, "skipped", "mark_all_done must not overwrite it");

    // Idempotent: a second call with a fresh key changes nothing further.
    api::mark_all_done(BOY, 1, date, format!("hs4-h-{}", uuid_ish()))
        .await
        .expect("second mark_all_done");
    assert_eq!(
        log_row_count(pool, BOY).await,
        after_first,
        "a second call must be a no-op once everything is logged"
    );
}

// ---------------------------------------------------------------------------
// (i) set_subject_schedule(days = "Th") errors and writes nothing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn hs4_i_set_subject_schedule_rejects_th_and_writes_nothing() {
    let _guard = hs4_lock().await;
    let pool = db::pool().await.expect("pool");
    let curriculum_id = load_fixture(pool).await;
    let sums = subject_id(pool, curriculum_id, "Sums").await;
    let token = parent_session().await;

    let before: (String,) = sqlx::query_as("SELECT days FROM subjects WHERE id = ?1")
        .bind(sums)
        .fetch_one(pool)
        .await
        .expect("days before");

    let result = api::set_subject_schedule(sums, "Th".to_string(), false, token).await;
    assert!(result.is_err(), "'Th' is not a valid day letter");

    let after: (String,) = sqlx::query_as("SELECT days FROM subjects WHERE id = ?1")
        .bind(sums)
        .fetch_one(pool)
        .await
        .expect("days after");
    assert_eq!(before, after, "a rejected call must write nothing");
}

// ---------------------------------------------------------------------------
// (j) get_homeschool_today: nobody enrolled / paused
// ---------------------------------------------------------------------------

#[tokio::test]
async fn hs4_j_get_homeschool_today_reports_nobody_enrolled_and_a_paused_boys_empty_lists() {
    let _guard = hs4_lock().await;
    let pool = db::pool().await.expect("pool");
    reset_homeschool_state(pool).await;
    let _curriculum_id = load_fixture(pool).await;

    let empty = api::get_homeschool_today(today_string())
        .await
        .expect("nobody enrolled must not be an error");
    assert!(!empty.anyone_enrolled);
    assert!(empty.groups.is_empty());

    let curriculum_id = load_fixture(pool).await;
    const BOY: i64 = 1;
    enroll_direct(pool, BOY, curriculum_id, 1, "MTWRF", "2026-01-05").await;
    hs::set_paused(pool, BOY, true).await.expect("pause");

    let paused = api::get_homeschool_today(today_string())
        .await
        .expect("paused is a normal view");
    assert!(paused.anyone_enrolled);
    let group = paused
        .groups
        .first()
        .expect("the paused boy's group is still reported");
    assert!(group.paused);
    let boy_view = group
        .boys
        .iter()
        .find(|b| b.user_id == BOY)
        .expect("the boy's own entry");
    assert!(boy_view.due_today.is_empty());
    assert!(boy_view.catch_up.is_empty());
    assert!(boy_view.done.is_empty());
}

// ---------------------------------------------------------------------------
// (k) add_extra / toggle_extra / delete_extra
// ---------------------------------------------------------------------------

#[tokio::test]
async fn hs4_k_add_extra_requires_a_session_and_bounds_scheduled_date() {
    let _guard = hs4_lock().await;
    let pool = db::pool().await.expect("pool");
    reset_homeschool_state(pool).await;

    const BOY: i64 = 1;
    let today = today_string();

    let denied = api::add_extra(
        BOY,
        today.clone(),
        "Copywork".to_string(),
        Category::Daily,
        None,
        today.clone(),
        format!("hs4-k-{}", uuid_ish()),
        String::new(),
    )
    .await;
    assert!(denied.is_err(), "add_extra with no session must error");

    let token = parent_session().await;
    let extra = api::add_extra(
        BOY,
        today.clone(),
        "Copywork".to_string(),
        Category::Daily,
        None,
        today.clone(),
        format!("hs4-k-{}", uuid_ish()),
        token.clone(),
    )
    .await
    .expect("a session is enough");
    assert_eq!(extra.user_id, BOY);
    assert_eq!(extra.title, "Copywork");

    // Far outside [today - 365, today + 365]: rejected, nothing written.
    let too_far = api::add_extra(
        BOY,
        "2099-01-01".to_string(),
        "Nope".to_string(),
        Category::Daily,
        None,
        today.clone(),
        format!("hs4-k-{}", uuid_ish()),
        token.clone(),
    )
    .await;
    assert!(
        too_far.is_err(),
        "a scheduled_date a year+ away must be rejected"
    );
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM lesson_extras WHERE profile_id = ?1 AND title = 'Nope'",
    )
    .bind(BOY)
    .fetch_one(pool)
    .await
    .expect("count");
    assert_eq!(count.0, 0, "the rejected add must write nothing");

    // toggle_extra needs no cookie and honours the ±1 day date window.
    let ticked = api::toggle_extra(
        extra.id,
        true,
        LogStatus::Done,
        None,
        today.clone(),
        format!("hs4-k-{}", uuid_ish()),
    )
    .await;
    assert!(ticked.is_ok(), "toggle_extra needs no cookie: {ticked:?}");
    let rejected_window = api::toggle_extra(
        extra.id,
        true,
        LogStatus::Done,
        None,
        days_ago(3),
        format!("hs4-k-{}", uuid_ish()),
    )
    .await;
    assert!(
        rejected_window.is_err(),
        "toggle_extra must honour the ±1 day date window"
    );

    // delete_extra of a ticked extra removes it outright.
    api::delete_extra(extra.id, token)
        .await
        .expect("delete a ticked extra");
    let gone: Option<(i64,)> = sqlx::query_as("SELECT id FROM lesson_extras WHERE id = ?1")
        .bind(extra.id)
        .fetch_optional(pool)
        .await
        .expect("query");
    assert!(gone.is_none(), "delete_extra must remove the row");
}

// ---------------------------------------------------------------------------
// (l) get_week_grid / get_month boundary rules
// ---------------------------------------------------------------------------

#[tokio::test]
async fn hs4_l_get_week_grid_bounds_and_datedness() {
    let _guard = hs4_lock().await;
    let pool = db::pool().await.expect("pool");
    reset_homeschool_state(pool).await;
    let curriculum_id = load_fixture(pool).await;

    const ENROLLED: i64 = 1;
    const UNENROLLED: i64 = 2;
    enroll_direct(pool, ENROLLED, curriculum_id, 2, "MTWRF", "2026-01-05").await;

    let current = api::get_week_grid(ENROLLED, 2).await.expect("week 2");
    assert!(current.dated, "the current week is dated");

    let other = api::get_week_grid(ENROLLED, 1).await.expect("week 1");
    assert!(
        !other.dated,
        "a week that is not the current one is not dated"
    );

    assert!(
        api::get_week_grid(ENROLLED, 0).await.is_err(),
        "week 0 must error"
    );
    assert!(
        api::get_week_grid(ENROLLED, 4).await.is_err(),
        "a week past the end of the curriculum (weeks = 3) must error"
    );
    assert!(
        api::get_week_grid(UNENROLLED, 1).await.is_err(),
        "an unenrolled boy must error"
    );
}

#[tokio::test]
async fn hs4_l_get_month_fetches_the_current_week_plan_only_when_it_intersects_and_is_extras_only_when_unenrolled(
) {
    let _guard = hs4_lock().await;
    let pool = db::pool().await.expect("pool");
    reset_homeschool_state(pool).await;
    let curriculum_id = load_fixture(pool).await;

    const ENROLLED: i64 = 1;
    const UNENROLLED: i64 = 2;
    // A week whose span sits entirely inside September 2026.
    enroll_direct(pool, ENROLLED, curriculum_id, 1, "MTWRF", "2026-09-07").await;

    let month = api::get_month(ENROLLED, 2026, 9).await.expect("month");
    assert_eq!(month.days.len(), 30);
    let in_week = month.days.iter().filter(|d| d.in_current_week).count();
    assert_eq!(in_week, 7, "the whole 7-day span sits inside September");
    assert!(
        month
            .days
            .iter()
            .any(|d| d.in_current_week && d.total.is_some()),
        "an in-week day must carry a total"
    );
    assert!(
        month
            .days
            .iter()
            .any(|d| !d.in_current_week && d.total.is_none()),
        "an out-of-week day must not"
    );

    // A month the current week's span never touches at all.
    let far_month = api::get_month(ENROLLED, 2027, 1).await.expect("far month");
    assert!(far_month.days.iter().all(|d| !d.in_current_week));
    assert!(far_month.days.iter().all(|d| d.total.is_none()));

    // An unenrolled boy with one extra: extras-only days.
    let token = parent_session().await;
    api::add_extra(
        UNENROLLED,
        "2026-09-10".to_string(),
        "Nature walk".to_string(),
        Category::Weekly,
        None,
        today_string(),
        format!("hs4-l-{}", uuid_ish()),
        token,
    )
    .await
    .expect("add an extra for the unenrolled boy");

    let unenrolled_month = api::get_month(UNENROLLED, 2026, 9)
        .await
        .expect("month for an unenrolled boy is not an error");
    assert!(unenrolled_month.days.iter().all(|d| d.total.is_none()));
    assert!(unenrolled_month.days.iter().all(|d| !d.is_school_day));
    let extra_day = unenrolled_month
        .days
        .iter()
        .find(|d| d.date == "2026-09-10")
        .expect("the 10th is in the month");
    assert_eq!(extra_day.extras, 1);
}

// ---------------------------------------------------------------------------
// (m) toggle_lesson with subject_id <= 0 is rejected before any write
// ---------------------------------------------------------------------------

#[tokio::test]
async fn hs4_m_toggle_lesson_with_a_non_positive_subject_id_is_rejected_before_any_write() {
    let _guard = hs4_lock().await;
    let pool = db::pool().await.expect("pool");
    reset_homeschool_state(pool).await;
    let curriculum_id = load_fixture(pool).await;

    const BOY: i64 = 1;
    enroll_direct(pool, BOY, curriculum_id, 1, "MTWRF", "2026-01-05").await;

    for bad_subject in [0_i64, -1] {
        let result = api::toggle_lesson(
            BOY,
            bad_subject,
            None,
            1,
            today_string(),
            true,
            LogStatus::Done,
            None,
            today_string(),
            format!("hs4-m-{}", uuid_ish()),
        )
        .await;
        assert!(result.is_err(), "subject_id {bad_subject} must be rejected");
    }
    assert_eq!(
        log_row_count(pool, BOY).await,
        0,
        "nothing may be written for a non-positive subject_id"
    );
}

// ---------------------------------------------------------------------------
// DayItem sanity — merge_extras / today_view really do land in BoyToday
// (exercised indirectly above; this proves the enum discriminates correctly
// through the wire type used by every accept test in this file).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn hs4_day_item_lesson_and_extra_are_distinguishable_on_a_real_today_view() {
    let _guard = hs4_lock().await;
    let pool = db::pool().await.expect("pool");
    reset_homeschool_state(pool).await;
    let curriculum_id = load_fixture(pool).await;
    let sums = subject_id(pool, curriculum_id, "Sums").await;
    widen_to_every_day(pool, sums, false).await;

    const BOY: i64 = 1;
    enroll_direct(pool, BOY, curriculum_id, 1, "MTWRFSU", "2026-01-05").await;
    let (subject, assignment, scheduled_date) = first_occurrence(BOY, 1, "Sums").await;
    let today = today_string();

    api::toggle_lesson(
        BOY,
        subject,
        assignment,
        1,
        scheduled_date,
        true,
        LogStatus::Done,
        None,
        today.clone(),
        format!("hs4-day-item-{}", uuid_ish()),
    )
    .await
    .expect("tick");

    let token = parent_session().await;
    api::add_extra(
        BOY,
        today.clone(),
        "Nature walk".to_string(),
        Category::Weekly,
        None,
        today.clone(),
        format!("hs4-day-item-{}", uuid_ish()),
        token,
    )
    .await
    .expect("add an extra");

    let view = api::get_homeschool_today(today).await.expect("today view");
    let boy = view
        .groups
        .iter()
        .flat_map(|g| g.boys.iter())
        .find(|b| b.user_id == BOY)
        .expect("the boy's own view");

    let has_lesson = boy
        .done
        .iter()
        .any(|item| matches!(item, DayItem::Lesson(_)));
    let has_extra = boy
        .due_today
        .iter()
        .any(|item| matches!(item, DayItem::Extra(_)));
    assert!(has_lesson, "the ticked Sums occurrence must be a Lesson");
    assert!(has_extra, "the added task must be an Extra, due today");
}
