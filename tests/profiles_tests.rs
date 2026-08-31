//! T1.4 acceptance suite: profiles, the parent PIN, and session tokens.
//!
//! `docs/reviews/PURPLE_TEAM.md` §P3 T1.4 lists five acceptance points beyond
//! the FK-violation test (which replaces `tests/db_tests.rs`'s old CHECK
//! test, per W5):
//!
//! 1. rename persists and emits `ProfilesUpdated`, observed on a **second**
//!    WS client — [`rename_profile_persists_and_broadcasts_profiles_updated`]
//! 2. a 5th and 6th profile can be created —
//!    [`a_fifth_and_sixth_profile_can_be_created`]
//! 3. PIN verify succeeds once and fails 10x with a monotonically increasing
//!    delay >= 2^n ms —
//!    [`pin_verify_succeeds_once_and_backs_off_over_ten_failures`]
//! 4. the PIN check is enforced in the server fn, not the client: calling a
//!    privileged fn directly with no session errors —
//!    [`privileged_fn_without_a_session_errors`]
//!
//! Follows the same in-process-router pattern `tests/http_tests.rs` (T0.3)
//! established: a throwaway sqlite file via `DATABASE_URL`, the real
//! production router bound to an ephemeral port for the one test that needs
//! a live `/ws` client, and every `#[server]` fn called directly everywhere
//! else — which is the real server-side implementation running in-process
//! (see `src/server/api/routine.rs`'s `#[cfg(feature = "server")]` bodies),
//! not a network round trip.

#![cfg(feature = "server")]

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use family_calendar::server::api::profiles;
use family_calendar::server::auth;
use family_calendar::server::config::FamilyHubConfig;
use family_calendar::server::db;
use family_calendar::shared::types::ServerMessage;
use futures_util::StreamExt;
use tokio_tungstenite::tungstenite::Message as WsClientMessage;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type WsStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

/// The PIN this whole test file sets up and verifies against.
const TEST_PIN: &str = "135790";

/// Point every test in this binary at one throwaway sqlite file and data
/// directory, and give Dioxus 0.7's `serve_static_assets` an existing (empty)
/// public directory (mirrors `tests/http_tests.rs::init_test_env`).
fn init_test_env() -> std::path::PathBuf {
    static ONCE: std::sync::Once = std::sync::Once::new();
    let base =
        std::env::temp_dir().join(format!("familyhub-profiles-tests-{}", std::process::id()));
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
        std::env::set_var("FAMILY_HUB_DATA_DIR", &base);

        let public = base.join("public");
        std::fs::create_dir_all(&public).expect("test public directory is creatable");
        std::env::set_var("DIOXUS_PUBLIC_PATH", &public);
    });
    base
}

/// Boot the real production router on an OS-assigned free port. Only the
/// broadcast test needs a live socket; every other test calls the `#[server]`
/// fns directly.
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

/// Reads frames off `socket` until one whose text contains `marker`, or
/// panics after five seconds (mirrors `tests/http_tests.rs::recv_matching`).
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

/// Serialises every test in this binary that asserts on the *timing* of the
/// process-global PIN failure counter (`auth::current_pin_failures`,
/// `auth::PIN_GATE`) against every other one — QA round 1's Q1-02/Q1-03 gate
/// makes a wrong setup code and a wrong PIN share exactly one counter and one
/// mutex, so a reset triggered by one test's correct guess (`reset_pin_failures`)
/// landing mid-sequence in another test's own wrong-guess loop would corrupt
/// that loop's floor assertions. Every test below that fires more than one
/// deliberately-wrong attempt in a row takes this guard first (mirrors
/// `tests/realtime_tests.rs::hub_lock`).
async fn pin_state_guard() -> tokio::sync::MutexGuard<'static, ()> {
    static LOCK: tokio::sync::OnceCell<tokio::sync::Mutex<()>> = tokio::sync::OnceCell::const_new();
    LOCK.get_or_init(|| async { tokio::sync::Mutex::new(()) })
        .await
        .lock()
        .await
}

/// Runs the real first-run flow exactly once for this process — generate the
/// setup code, use it to set [`TEST_PIN`] — no matter how many tests call it
/// concurrently (`tokio::sync::OnceCell::get_or_init` runs its initializer
/// exactly once even under concurrent callers), and returns a valid parent
/// session token every caller can use.
async fn parent_session() -> String {
    let _guard = pin_state_guard().await;
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

            auth::set_initial_pin(pool, &dir, &code, TEST_PIN)
                .await
                .expect("set the initial parent PIN")
        })
        .await
        .clone()
}

// ---------------------------------------------------------------------------
// 1. rename persists and emits ProfilesUpdated on a second WS client
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rename_profile_persists_and_broadcasts_profiles_updated() {
    let addr = spawn_test_server().await;
    let auth_token = parent_session().await;

    // A fresh profile so this test's own row is independent of execution
    // order relative to the other tests in this file.
    let created = profiles::create_profile(
        auth_token.clone(),
        "Rename Me".to_string(),
        "#334455".to_string(),
        None,
        false,
    )
    .await
    .expect("create a profile to rename");

    let url = format!("ws://{addr}/ws");
    let (mut client, upgrade) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("second WS client connects");
    assert_eq!(upgrade.status().as_u16(), 101);
    // Drain the connection's own Hello so it does not satisfy the match below.
    recv_matching(&mut client, "\"hello\"").await;

    profiles::rename_profile(auth_token, created.id, "Renamed!".to_string())
        .await
        .expect("rename succeeds");

    let received = recv_matching(&mut client, "profiles_updated").await;
    let parsed: ServerMessage = serde_json::from_str(&received).expect("valid ServerMessage JSON");
    assert!(
        matches!(parsed, ServerMessage::ProfilesUpdated),
        "expected ProfilesUpdated, got {parsed:?}"
    );

    // And the rename actually persisted.
    let all = profiles::list_profiles().await.expect("list profiles");
    let renamed = all
        .iter()
        .find(|p| p.id == created.id)
        .expect("the renamed profile still exists");
    assert_eq!(renamed.name, "Renamed!");
}

// ---------------------------------------------------------------------------
// 2. a 5th and 6th profile can be created
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_fifth_and_sixth_profile_can_be_created() {
    let auth_token = parent_session().await;

    // Not asserting a specific id: other tests in this file create profiles
    // of their own and tests run concurrently, so only "beyond the original
    // four, and distinct from each other" is a safe assertion here.
    let fifth = profiles::create_profile(
        auth_token.clone(),
        "Guest".to_string(),
        "#111111".to_string(),
        None,
        false,
    )
    .await
    .expect("a 5th profile must be creatable");
    assert!(fifth.id > 4, "5th profile id {} should exceed 4", fifth.id);

    let sixth = profiles::create_profile(
        auth_token,
        "Grandma".to_string(),
        "#222222".to_string(),
        None,
        false,
    )
    .await
    .expect("a 6th profile must be creatable");
    assert!(sixth.id > 4, "6th profile id {} should exceed 4", sixth.id);
    assert_ne!(fifth.id, sixth.id);

    let all = profiles::list_profiles().await.expect("list profiles");
    assert!(all.iter().any(|p| p.id == fifth.id && p.name == "Guest"));
    assert!(all.iter().any(|p| p.id == sixth.id && p.name == "Grandma"));
}

// ---------------------------------------------------------------------------
// 3. PIN verify succeeds once, then fails 10x with delay >= 2^n ms
// ---------------------------------------------------------------------------

///
/// Argon2id verification is deliberately expensive (memory-hard by design —
/// that is the whole point of hashing a PIN with it) and its wall-clock cost
/// is paid on **every** attempt, right or wrong, and varies run to run under
/// system scheduling noise. That cost would swamp the small early backoff
/// deltas (2 ms, 4 ms, 8 ms, ...) in a strict pairwise "each attempt strictly
/// slower than the last" comparison, making such an assertion flaky through
/// no fault of the backoff logic itself. So this test proves the integration
/// really applies the schedule two ways that stay true regardless of argon2
/// jitter — `tokio::time::sleep` guarantees attempt *n* never returns in
/// **less** than its floor, jitter can only ever add, never subtract — while
/// the exact monotonically-increasing schedule itself (the strict pairwise
/// property) is proven precisely, with no crypto cost in the way, by
/// `src/server/auth.rs`'s own
/// `backoff_delay_is_monotonically_increasing_and_at_least_two_to_the_n_ms`.
#[tokio::test]
async fn pin_verify_succeeds_once_and_backs_off_over_ten_failures() {
    parent_session().await; // establishes TEST_PIN via the real set-up flow
                            // Taken *after* `parent_session()` (which briefly takes it itself, to
                            // serialise its own one-time setup against every other guarded test) —
                            // `tokio::sync::Mutex` is not reentrant.
    let _guard = pin_state_guard().await;

    // A correct guess succeeds and resets the failure counter — checked
    // directly rather than by wall-clock, since argon2id verification alone
    // can take a non-trivial and variable amount of time even on the success
    // path (there is no cheap way to check a password hash).
    let token = profiles::verify_parent_pin(TEST_PIN.to_string())
        .await
        .expect("the correct PIN must verify");
    assert!(!token.is_empty());
    assert_eq!(
        auth::current_pin_failures(),
        0,
        "a correct PIN must reset the failure counter"
    );

    // Ten consecutive wrong guesses: each individual answer must take at
    // least 2^n ms (a floor `tokio::time::sleep` guarantees regardless of
    // argon2 jitter on top of it), and the cumulative wait across all ten
    // must reach the schedule's total floor — both of which hold precisely
    // because backoff only ever adds delay, never removes it.
    let mut cumulative = Duration::from_millis(0);
    for attempt in 1..=10u32 {
        let started = Instant::now();
        let result = profiles::verify_parent_pin("000000".to_string()).await;
        let elapsed = started.elapsed();
        cumulative += elapsed;

        assert!(result.is_err(), "attempt {attempt}: a wrong PIN must fail");
        assert!(
            elapsed >= Duration::from_millis(2u64.pow(attempt)),
            "attempt {attempt}: waited only {elapsed:?}, expected >= 2^{attempt} ms"
        );
    }
    let cumulative_floor: u64 = (1..=10u32).map(|n| 2u64.pow(n)).sum();
    assert!(
        cumulative >= Duration::from_millis(cumulative_floor),
        "ten attempts took only {cumulative:?} total, expected >= {cumulative_floor} ms"
    );

    // No lockout (PURPLE default 9): the correct PIN still works immediately
    // after ten straight failures.
    profiles::verify_parent_pin(TEST_PIN.to_string())
        .await
        .expect("no lockout: the correct PIN still verifies after 10 failures");
    assert_eq!(auth::current_pin_failures(), 0);
}

// ---------------------------------------------------------------------------
// QA round 1, Q1-03 — the backoff is a gate, not a per-request sleep
// ---------------------------------------------------------------------------

/// 8 wrong PINs fired concurrently (`tokio::join!`) must still take at least
/// the schedule's cumulative floor (`Σ 2^n ms, n=1..8 = 510 ms`) of *wall*
/// time — proving attempts are serialised (`auth::PIN_GATE`), not merely each
/// individually delayed and then all answered together within roughly one
/// delay window, which is what let a scripted brute force reach the whole
/// keyspace in about an hour at argon2's own throughput before this fix.
#[tokio::test]
async fn eight_parallel_wrong_pins_are_serialised_not_just_individually_delayed() {
    parent_session().await; // establishes TEST_PIN via the real set-up flow
    let _guard = pin_state_guard().await;

    let started = Instant::now();
    let results = tokio::join!(
        profiles::verify_parent_pin("000000".to_string()),
        profiles::verify_parent_pin("000001".to_string()),
        profiles::verify_parent_pin("000002".to_string()),
        profiles::verify_parent_pin("000003".to_string()),
        profiles::verify_parent_pin("000004".to_string()),
        profiles::verify_parent_pin("000005".to_string()),
        profiles::verify_parent_pin("000006".to_string()),
        profiles::verify_parent_pin("000007".to_string()),
    );
    let elapsed = started.elapsed();

    let (r0, r1, r2, r3, r4, r5, r6, r7) = results;
    for result in [r0, r1, r2, r3, r4, r5, r6, r7] {
        assert!(
            result.is_err(),
            "every wrong parallel PIN attempt must fail"
        );
    }

    let floor: u64 = (1..=8u32).map(|n| 2u64.pow(n)).sum(); // 510 ms
    assert!(
        elapsed >= Duration::from_millis(floor),
        "8 parallel wrong PINs took only {elapsed:?} of wall-clock time, expected >= {floor} ms \
         — the backoff gate must serialise attempts (Q1-03), not just delay each request \
         independently and answer them all together"
    );

    // No lockout: the correct PIN still verifies immediately afterwards.
    profiles::verify_parent_pin(TEST_PIN.to_string())
        .await
        .expect("no lockout: the correct PIN still verifies after a parallel burst");
}

// ---------------------------------------------------------------------------
// 4. server-side enforcement: a privileged fn with no session errors
// ---------------------------------------------------------------------------

#[tokio::test]
async fn privileged_fn_without_a_session_errors() {
    init_test_env();
    db::pool().await.expect("test sqlite pool opens");

    // Called directly — the real server-side implementation, not a client —
    // with an empty auth token. The PIN/session gate must be enforced here,
    // not merely assumed by whatever UI would normally have called this.
    let result = profiles::rename_profile(String::new(), 1, "Nope".to_string()).await;
    assert!(
        result.is_err(),
        "rename_profile must require a valid parent session, even called directly"
    );

    let result = profiles::create_profile(
        "not-a-real-session".to_string(),
        "Nope".to_string(),
        "#000000".to_string(),
        None,
        false,
    )
    .await;
    assert!(
        result.is_err(),
        "create_profile must require a valid parent session, even called directly"
    );
}

// ---------------------------------------------------------------------------
// First-run setup gate
// ---------------------------------------------------------------------------

#[tokio::test]
async fn setting_the_initial_pin_requires_the_real_setup_code() {
    let _guard = pin_state_guard().await;

    init_test_env();
    let pool = db::pool().await.expect("test sqlite pool opens");
    let dir = FamilyHubConfig::load().data_dir;

    // Make sure a setup code exists to be wrong about (idempotent — a no-op
    // once `parent_session()` from another test has already set the PIN).
    auth::ensure_setup_code(pool, &dir)
        .await
        .expect("ensure a setup code exists");

    let result = auth::set_initial_pin(pool, &dir, "not-the-code", "246810").await;
    assert!(
        result.is_err(),
        "an incorrect setup code must not be enough to set the initial PIN"
    );

    // Q1-02: a wrong setup code now shares `verify_pin`'s counter/backoff/gate
    // — proven here against a *fresh, isolated* database (this binary's
    // shared `db::pool()` may already have a real PIN set by
    // `parent_session()`, at which point `set_initial_pin` short-circuits to
    // `PinAlreadySet` before ever touching the gate, which would make this
    // assertion meaningless) so it is guaranteed to still be pre-PIN no
    // matter what order the rest of this binary's tests happen to run in.
    let scratch = std::env::temp_dir().join(format!(
        "familyhub-profiles-setupcode-backoff-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("isolated scratch directory is creatable");
    let scratch_db = scratch.join("family.db");
    let scratch_url = format!(
        "sqlite://{}",
        scratch_db.display().to_string().replace('\\', "/")
    );
    let scratch_pool = db::connect(&scratch_url)
        .await
        .expect("isolated sqlite pool opens");
    db::migrate(&scratch_pool)
        .await
        .expect("isolated database migrates");
    auth::ensure_setup_code(&scratch_pool, &scratch)
        .await
        .expect("generate a setup code in the isolated database");

    let failures_before = auth::current_pin_failures();
    let mut cumulative = Duration::from_millis(0);
    for attempt in 1..=5u32 {
        let started = Instant::now();
        let result =
            auth::set_initial_pin(&scratch_pool, &scratch, "still-not-the-code", "246810").await;
        let elapsed = started.elapsed();
        cumulative += elapsed;
        assert!(
            result.is_err(),
            "attempt {attempt}: a wrong setup code must fail"
        );
        assert!(
            elapsed >= Duration::from_millis(2u64.pow(attempt)),
            "attempt {attempt}: waited only {elapsed:?}, expected >= 2^{attempt} ms — \
             wrong setup codes must be backed off exactly like wrong PINs"
        );
    }
    let cumulative_floor: u64 = (1..=5u32).map(|n| 2u64.pow(n)).sum();
    assert!(
        cumulative >= Duration::from_millis(cumulative_floor),
        "five attempts took only {cumulative:?} total, expected >= {cumulative_floor} ms"
    );
    assert!(
        auth::current_pin_failures() > failures_before,
        "a wrong setup code must advance the same shared failure counter a wrong PIN does"
    );

    let _ = std::fs::remove_dir_all(&scratch);
}
