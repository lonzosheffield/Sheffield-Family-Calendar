//! Parent PIN and session tokens (PLAN v2 T1.4).
//!
//! Two secrets live here, and they are not the same thing:
//!
//! * **The parent PIN** — six digits, hashed with argon2id (`=0.6.0`,
//!   PURPLE_TEAM.md §P5.4) and stored in `settings` under [`PIN_HASH_SETTING`].
//!   Never logged, never returned to a client, never compared in a way that
//!   would leak timing (`argon2::PasswordVerifier` is constant-time by
//!   construction).
//! * **The first-run setup code** — a second, throwaway 6-digit code the
//!   server invents once, before any PIN exists, so that *setting* the very
//!   first PIN can be gated on physical proximity to the server (the log,
//!   `<data>\setup-code.txt`, and — once T2.1 lands — the TV) rather than on
//!   nothing at all. It is stored in plain text in `settings`: it is not the
//!   thing being protected, it is what protects the thing that will be.
//!
//! **Sessions.** [`issue_session`] mints a 30-day bearer token
//! (PURPLE §P5.5 default 31) held in an in-memory store — a restart signs
//! every parent out, which is an acceptable trade for a display that is
//! expected to run for months at a time without one. `src/server/api/realtime.rs`
//! reserves exactly this seam for T1.4 (its `session` module's doc comment):
//! this module is what its five function bodies now delegate to.
//!
//! **Backoff, not lockout.** [`verify_pin`] never permanently refuses a
//! guess — PURPLE §P5.5 default 9 is explicit that a hard lockout on a wall
//! display the whole family depends on would be a self-inflicted outage.
//! Instead every wrong guess *waits* before answering, doubling each time
//! ([`backoff_delay`]), which makes both automated and manual guessing
//! impractical without ever locking a parent who mistyped their own PIN out
//! of their own kitchen display.
//!
//! **The backoff is a gate, not just a delay (QA round 1, Q1-02/Q1-03).**
//! [`PIN_GATE`] serialises every [`verify_pin`] and [`set_initial_pin`] call
//! process-wide — the lock is held across the lookup, the argon2 check
//! (moved to [`tokio::task::spawn_blocking`] so it never ties up an async
//! worker) **and** the sleep — so N parallel wrong guesses cannot all be
//! answered within one delay window; each one queues behind the last. A
//! wrong first-run setup code now bumps and waits on exactly the same
//! counter as a wrong PIN: the setup code guards the same secret (it is what
//! stands between a stranger on the LAN and setting the very first parent
//! PIN) and deserves the identical schedule, not an unthrottled loop.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

use tokio::sync::Mutex as AsyncMutex;

use argon2::password_hash::{phc::PasswordHash, PasswordHasher, PasswordVerifier};
use argon2::Argon2;
use sqlx::SqlitePool;

use crate::server::db;

/// `settings.key` the argon2id hash of the current parent PIN is stored under.
pub const PIN_HASH_SETTING: &str = "parent_pin_hash";
/// `settings.key` the first-run setup code is stored under (plain text —
/// see the module doc comment for why that is fine here).
const SETUP_CODE_SETTING: &str = "parent_setup_code";
/// File written under the data directory the first time a setup code is
/// generated (task description: "written ... to `<data>\setup-code.txt`").
pub const SETUP_CODE_FILE_NAME: &str = "setup-code.txt";

/// Parent sessions live 30 days (PURPLE_TEAM.md §P5.5 default 31).
pub const SESSION_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// Ceiling on [`backoff_delay`] so a very long streak of wrong guesses
/// degrades the UI rather than becoming an unbounded sleep — still a delay,
/// never a lockout (PURPLE default 9). `2^10 ms` (≈1 s, the acceptance
/// test's 10th attempt) is far below this, so the cap never shortens the
/// schedule the acceptance test measures.
const MAX_BACKOFF: Duration = Duration::from_secs(30);

/// Why a parent-PIN or session operation failed. Every variant is safe to
/// turn into a `ServerFnError` string: none of them ever include the PIN,
/// the setup code, or a session token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    /// No parent PIN has been set yet — call [`set_initial_pin`] first.
    PinNotSet,
    /// A parent PIN already exists; [`set_initial_pin`] only runs once.
    PinAlreadySet,
    /// A PIN that is not exactly six ASCII digits.
    InvalidPinFormat,
    /// The setup code presented did not match the one on record.
    InvalidSetupCode,
    /// The PIN presented did not match the stored hash. Carries how long the
    /// caller already waited before this error was returned.
    IncorrectPin { waited: Duration },
    /// No valid (unexpired, unrevoked) session token was presented.
    NotAuthenticated,
    /// The profile referenced does not exist.
    UnknownProfile,
    /// A database or filesystem operation failed.
    Storage(String),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PinNotSet => write!(f, "no parent PIN has been set yet"),
            Self::PinAlreadySet => write!(f, "a parent PIN is already set"),
            Self::InvalidPinFormat => write!(f, "a PIN must be exactly six digits"),
            Self::InvalidSetupCode => write!(f, "that setup code is not correct"),
            Self::IncorrectPin { waited } => {
                write!(f, "incorrect PIN (waited {waited:?} before answering)")
            }
            Self::NotAuthenticated => write!(f, "a valid parent session is required"),
            Self::UnknownProfile => write!(f, "no profile with that id exists"),
            Self::Storage(err) => write!(f, "storage error: {err}"),
        }
    }
}

impl std::error::Error for AuthError {}

impl From<sqlx::Error> for AuthError {
    fn from(err: sqlx::Error) -> Self {
        Self::Storage(err.to_string())
    }
}

impl From<std::io::Error> for AuthError {
    fn from(err: std::io::Error) -> Self {
        Self::Storage(err.to_string())
    }
}

// ---------------------------------------------------------------------------
// PIN formatting / hashing
// ---------------------------------------------------------------------------

/// Exactly six ASCII digits (PURPLE §P5.5 default 9: "6 digits ... 100x
/// keyspace at zero UX cost" over the White proposal's four).
pub fn is_valid_pin_format(pin: &str) -> bool {
    pin.len() == 6 && pin.bytes().all(|b| b.is_ascii_digit())
}

fn hash_pin(pin: &str) -> Result<String, AuthError> {
    // `PasswordHasher::hash_password` (password-hash 0.6, PURPLE §P5.4's
    // named trap — the pre-0.6 explicit-salt signature is gone) generates
    // its own random salt internally via the `getrandom` feature.
    Argon2::default()
        .hash_password(pin.as_bytes())
        .map(|hash| hash.to_string())
        .map_err(|err| AuthError::Storage(format!("argon2id hashing failed: {err}")))
}

fn verify_pin_hash(pin: &str, stored_hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(stored_hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(pin.as_bytes(), &parsed)
        .is_ok()
}

/// A random six-digit code, formatted with leading zeros. Reuses
/// `password_hash::try_generate_salt` (the same `getrandom`-backed source
/// [`hash_pin`] relies on internally) rather than reaching for a second RNG
/// type — rand_core 0.10 (pulled in transitively by `password-hash` 0.6.1)
/// dropped the `OsRng` type older argon2 examples use.
fn generate_six_digit_code() -> Result<String, AuthError> {
    let bytes = argon2::password_hash::try_generate_salt()
        .map_err(|err| AuthError::Storage(format!("failed to generate randomness: {err}")))?;
    let n = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) % 1_000_000;
    Ok(format!("{n:06}"))
}

/// Constant-time byte comparison, so checking the setup code does not leak
/// how many leading digits were right through response timing.
fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

// ---------------------------------------------------------------------------
// Parent PIN state
// ---------------------------------------------------------------------------

/// Has a parent PIN ever been set?
pub async fn pin_is_set(pool: &SqlitePool) -> Result<bool, AuthError> {
    Ok(db::get_setting(pool, PIN_HASH_SETTING).await?.is_some())
}

/// Make sure a first-run setup code exists, generating (and logging, and
/// writing to `<data>\setup-code.txt`) one if this is the very first time
/// anyone has asked. A no-op once a PIN has been set, and idempotent before
/// that: a restart before the parent finishes setup reuses the same code
/// rather than printing a different one (the file and the log line from the
/// first boot stay accurate).
///
/// Called from [`crate::server::api::profiles::parent_setup_status`], which
/// every client (TV included, once T2.1 lands) asks at least once — that is
/// this module's self-starting trigger, the same pattern
/// `api::realtime::ensure_background_tasks` uses for the midnight tick.
pub async fn ensure_setup_code(
    pool: &SqlitePool,
    data_dir: &Path,
) -> Result<Option<String>, AuthError> {
    if pin_is_set(pool).await? {
        return Ok(None);
    }

    if let Some(existing) = db::get_setting(pool, SETUP_CODE_SETTING).await? {
        write_setup_code_file(data_dir, &existing)?;
        return Ok(Some(existing));
    }

    let code = generate_six_digit_code()?;
    db::set_setting(pool, SETUP_CODE_SETTING, &code).await?;
    write_setup_code_file(data_dir, &code)?;
    tracing::info!(
        setup_code = %code,
        file = %data_dir.join(SETUP_CODE_FILE_NAME).display(),
        "generated the first-run parent PIN setup code"
    );
    Ok(Some(code))
}

fn write_setup_code_file(data_dir: &Path, code: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(data_dir)?;
    std::fs::write(data_dir.join(SETUP_CODE_FILE_NAME), code)
}

/// The setup code on record, if a PIN has not been set yet. Plain
/// `pub async fn`, not a `#[server]` fn: reachable from Rust (this crate's
/// own TV component once T2.1 lands, and this module's own tests) but never
/// exposed over the network, which is the whole point of gating first-run
/// setup on physical access to the server/TV rather than on an API call any
/// LAN client could make.
pub async fn read_setup_code(pool: &SqlitePool) -> Result<Option<String>, AuthError> {
    Ok(db::get_setting(pool, SETUP_CODE_SETTING).await?)
}

/// Set the very first parent PIN. Requires the first-run setup code (proving
/// the caller has physical access to the server's log/file/TV) and only ever
/// succeeds once — call [`change_pin`] afterwards.
pub async fn set_initial_pin(
    pool: &SqlitePool,
    data_dir: &Path,
    setup_code: &str,
    pin: &str,
) -> Result<String, AuthError> {
    // Q1-02/Q1-03: the same gate `verify_pin` holds, across the whole
    // check-and-maybe-sleep — a wrong setup code is exactly as brute-forceable
    // as a wrong PIN would be without this, and shares the same counter.
    let _serial = pin_gate().lock().await;

    if pin_is_set(pool).await? {
        return Err(AuthError::PinAlreadySet);
    }
    let expected = db::get_setting(pool, SETUP_CODE_SETTING)
        .await?
        .unwrap_or_default();
    let matches = !expected.is_empty() && constant_time_eq(setup_code, &expected);
    if !matches {
        let attempt = bump_pin_failures();
        let delay = backoff_delay(attempt);
        tokio::time::sleep(delay).await;
        return Err(AuthError::InvalidSetupCode);
    }
    if !is_valid_pin_format(pin) {
        return Err(AuthError::InvalidPinFormat);
    }

    let pin_owned = pin.to_string();
    let hash = tokio::task::spawn_blocking(move || hash_pin(&pin_owned))
        .await
        .map_err(|err| AuthError::Storage(err.to_string()))??;
    db::set_setting(pool, PIN_HASH_SETTING, &hash).await?;
    // The setup code has done its job; clearing it stops it from being
    // reusable and stops `ensure_setup_code` from ever handing it out again.
    db::set_setting(pool, SETUP_CODE_SETTING, "").await?;
    let _ = std::fs::remove_file(data_dir.join(SETUP_CODE_FILE_NAME));

    reset_pin_failures();
    Ok(issue_session())
}

/// Change the PIN. The caller must already hold a valid parent session —
/// enforced by [`require_session`] at the call site
/// (`api::profiles::change_pin`), not here, so this function's own contract
/// stays "the PIN storage operation" rather than duplicating authorisation.
pub async fn change_pin(pool: &SqlitePool, new_pin: &str) -> Result<(), AuthError> {
    if !is_valid_pin_format(new_pin) {
        return Err(AuthError::InvalidPinFormat);
    }
    let pin_owned = new_pin.to_string();
    let hash = tokio::task::spawn_blocking(move || hash_pin(&pin_owned))
        .await
        .map_err(|err| AuthError::Storage(err.to_string()))??;
    db::set_setting(pool, PIN_HASH_SETTING, &hash).await?;
    reset_pin_failures();
    Ok(())
}

// ---------------------------------------------------------------------------
// PIN verification + exponential backoff (no lockout)
// ---------------------------------------------------------------------------

/// Consecutive wrong guesses since the last success (or process start).
/// Global, not per-caller: there is exactly one family PIN, so there is
/// nothing more specific to key backoff by, and a global counter is what
/// makes the "no lockout" guarantee trivially true — every guess, from
/// anyone, on any device, is still answered, just increasingly slowly.
static PIN_FAILURES: OnceLock<Mutex<u32>> = OnceLock::new();

fn pin_failures() -> &'static Mutex<u32> {
    PIN_FAILURES.get_or_init(|| Mutex::new(0))
}

fn bump_pin_failures() -> u32 {
    let mut guard = pin_failures().lock().unwrap_or_else(|e| e.into_inner());
    *guard = guard.saturating_add(1);
    *guard
}

fn reset_pin_failures() {
    let mut guard = pin_failures().lock().unwrap_or_else(|e| e.into_inner());
    *guard = 0;
}

/// Test/ops seam: how many consecutive failures are currently on the books.
pub fn current_pin_failures() -> u32 {
    *pin_failures().lock().unwrap_or_else(|e| e.into_inner())
}

/// Serialises every [`verify_pin`] and [`set_initial_pin`] call, process-wide,
/// into exactly one at a time (Q1-03). Held across the *whole* body — the
/// lookup, the (now [`tokio::task::spawn_blocking`]'d) argon2 check, the
/// counter bump and the sleep — so a burst of parallel wrong guesses cannot
/// each pay their own delay concurrently and be answered together; attempt
/// *n* cannot even begin hashing until attempt *n-1*'s full wait has already
/// elapsed. Without this, guessing throughput was bounded only by argon2's
/// CPU cost (hundreds of attempts per second in release on a many-core box)
/// rather than by the backoff schedule at all.
static PIN_GATE: AsyncMutex<()> = AsyncMutex::const_new(());

fn pin_gate() -> &'static AsyncMutex<()> {
    &PIN_GATE
}

/// The delay a caller must wait *before* being told attempt number `attempt`
/// was wrong: `2^attempt` milliseconds, monotonically increasing, capped at
/// [`MAX_BACKOFF`] so an arbitrarily long streak still degrades rather than
/// hangs (PURPLE §P5.5 default 9: exponential backoff, no hard lockout).
pub fn backoff_delay(attempt: u32) -> Duration {
    let ms = 2u64.saturating_pow(attempt.min(32));
    Duration::from_millis(ms).min(MAX_BACKOFF)
}

/// Check a PIN attempt. On success: resets the failure counter and returns a
/// fresh 30-day session token, with no delay. On failure: increments the
/// counter, **sleeps for [`backoff_delay`] before returning**, and reports
/// how long it waited. The sleep (not a rejected-immediately response) is
/// what makes both a scripted brute force and a bored child mashing digits
/// slow to the point of pointlessness without ever refusing a legitimate
/// parent who is simply retrying.
pub async fn verify_pin(pool: &SqlitePool, pin: &str) -> Result<String, AuthError> {
    // Q1-03: hold the gate across the lookup, the (blocking) argon2 check,
    // the counter bump and the sleep, not just the sleep — see `PIN_GATE`'s
    // doc comment for why a per-request-only sleep does not actually throttle
    // parallel guesses.
    let _serial = pin_gate().lock().await;

    let stored_hash = db::get_setting(pool, PIN_HASH_SETTING)
        .await?
        .ok_or(AuthError::PinNotSet)?;

    let pin_owned = pin.to_string();
    let correct = is_valid_pin_format(pin)
        && tokio::task::spawn_blocking(move || verify_pin_hash(&pin_owned, &stored_hash))
            .await
            .map_err(|err| AuthError::Storage(err.to_string()))?;

    if correct {
        reset_pin_failures();
        return Ok(issue_session());
    }

    let attempt = bump_pin_failures();
    let delay = backoff_delay(attempt);
    tokio::time::sleep(delay).await;
    Err(AuthError::IncorrectPin { waited: delay })
}

// ---------------------------------------------------------------------------
// Session store
// ---------------------------------------------------------------------------
//
// `src/server/api/realtime.rs`'s `session` module reserves this as the one
// seam T1.4 owns: its five function bodies (`issue`, `insert`, `revoke`,
// `revoke_all`, `is_valid`) delegate to the five functions below, keeping
// their public signatures unchanged so every existing caller (the WS
// authorisation checks, `tests/realtime_tests.rs`) needs no update.

struct SessionStore {
    tokens: Mutex<HashMap<String, SystemTime>>,
}

static SESSIONS: OnceLock<SessionStore> = OnceLock::new();

fn sessions() -> &'static SessionStore {
    SESSIONS.get_or_init(|| SessionStore {
        tokens: Mutex::new(HashMap::new()),
    })
}

/// Mint and register a fresh 30-day parent session token.
pub fn issue_session() -> String {
    let token = uuid::Uuid::new_v4().to_string();
    insert_session(&token);
    token
}

/// Register (or refresh, if already present) a token with a full
/// [`SESSION_TTL`] from now.
pub fn insert_session(token: &str) {
    let expiry = SystemTime::now() + SESSION_TTL;
    let mut guard = sessions().tokens.lock().unwrap_or_else(|e| e.into_inner());
    guard.insert(token.to_string(), expiry);
}

pub fn revoke_session(token: &str) {
    let mut guard = sessions().tokens.lock().unwrap_or_else(|e| e.into_inner());
    guard.remove(token);
}

pub fn revoke_all_sessions() {
    let mut guard = sessions().tokens.lock().unwrap_or_else(|e| e.into_inner());
    guard.clear();
}

/// Is `token` a live (registered and not yet expired) parent session? An
/// empty token is never valid, so `SetView { auth: None }` and friends fail
/// closed without a database round trip.
pub fn is_valid_session(token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    let mut guard = sessions().tokens.lock().unwrap_or_else(|e| e.into_inner());
    match guard.get(token).copied() {
        Some(expiry) if expiry > SystemTime::now() => true,
        Some(_expired) => {
            guard.remove(token);
            false
        }
        None => false,
    }
}

/// The check every privileged server fn in `api::profiles` starts with —
/// **enforced here, server-side**, independent of anything the client sent
/// besides the token itself (the acceptance requirement that calling a
/// privileged fn directly with no session must error).
pub fn require_session(token: &str) -> Result<(), AuthError> {
    if is_valid_session(token) {
        Ok(())
    } else {
        Err(AuthError::NotAuthenticated)
    }
}

// ---------------------------------------------------------------------------
// Cookie session (QA round 1, Q1-11)
// ---------------------------------------------------------------------------
//
// PLAN v2 §P5.5 default 31 wants the parent session delivered as an
// `HttpOnly`/`Secure`/`SameSite=Lax` cookie on the HTTPS origin, not a bearer
// token the client has to remember to attach. `POST /api/login`
// (`src/server/router.rs`, the file a `Set-Cookie` can actually be attached
// from) mints that cookie from [`verify_pin`]; everything below is the
// server-side half of reading it back.

/// The `Set-Cookie` / `Cookie` name the parent session travels under once
/// `/api/login` mints one.
pub const SESSION_COOKIE_NAME: &str = "fh_session";

/// Pull the `fh_session` value out of a request's `Cookie` header, if present.
/// Does not check validity/expiry — callers combine this with
/// [`is_valid_session`] or [`require_session`].
pub fn session_from_headers(headers: &axum::http::HeaderMap) -> Option<String> {
    let cookie_header = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;
    cookie_header.split(';').find_map(|pair| {
        let (name, value) = pair.trim().split_once('=')?;
        (name == SESSION_COOKIE_NAME).then(|| value.to_string())
    })
}

/// Is this request same-origin (by `Origin`, falling back to
/// `Sec-Fetch-Site`, against the `Host` it was actually sent to), or does it
/// carry no origin signal at all?
///
/// A cookie is ambient — the browser attaches it to *every* request to the
/// origin that set it, cross-site ones included — which is exactly the
/// property that made the old bearer-token-in-`localStorage`
/// (`docs/HANDOFF.md` H-19) safe from CSRF and a cookie is not. Anything that
/// now trusts `fh_session` (`/ws`, `POST /api/login` re-authenticating an
/// already-signed-in browser) must refuse a request whose `Origin`/
/// `Sec-Fetch-Site` header explicitly says it came from somewhere else.
/// A request with **no** such header (a non-browser client: `curl`, the
/// `tokio-tungstenite` test harness, the TV's own fetches) is allowed through
/// — there is no ambient-cookie risk to guard against when nothing is riding
/// on a browser's cookie jar in the first place, and PURPLE's default 9
/// ("no lockout") extends to "never refuse a legitimate direct client that
/// simply does not send a browser header".
pub fn same_origin_or_absent(headers: &axum::http::HeaderMap) -> bool {
    if let Some(site) = headers
        .get("sec-fetch-site")
        .and_then(|value| value.to_str().ok())
    {
        if site != "same-origin" && site != "none" {
            return false;
        }
    }

    let Some(origin) = headers
        .get(axum::http::header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    else {
        return true;
    };
    let origin_authority = origin.split("://").nth(1).unwrap_or(origin);
    let host = headers
        .get(axum::http::header::HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    origin_authority.eq_ignore_ascii_case(host)
}

/// The check a server fn falls back to when its explicit `auth: SessionToken`
/// argument is empty (`src/server/api/profiles.rs`): is the `fh_session`
/// cookie on the *current* HTTP request a valid parent session?
///
/// Reads the request via Dioxus fullstack's own extraction seam
/// (`FullstackContext::extract`), which is only ever populated while a real
/// server-fn request is being handled. A direct in-process call (every
/// existing acceptance test calls these functions this way) has no such
/// request underneath it, so this always finds no cookie and fails closed —
/// exactly the "no session" case those tests assert on.
pub async fn require_parent() -> Result<(), AuthError> {
    let headers: axum::http::HeaderMap =
        dioxus::prelude::dioxus_fullstack::FullstackContext::extract()
            .await
            .map_err(|err| AuthError::Storage(err.to_string()))?;
    match session_from_headers(&headers) {
        Some(token) if is_valid_session(&token) => Ok(()),
        _ => Err(AuthError::NotAuthenticated),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The session store is process-global (`SESSIONS`), and `cargo test`
    /// runs unit tests in parallel: without this, one test's
    /// `revoke_all_sessions()` can land between another's `issue_session()`
    /// and its `require_session` assertion (Boss fix-up at the Recovery
    /// close - the race was observed once in a full-suite run).
    static SESSION_STORE_LOCK: Mutex<()> = Mutex::new(());

    fn session_store_guard() -> std::sync::MutexGuard<'static, ()> {
        SESSION_STORE_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    fn pin_format_accepts_exactly_six_digits() {
        assert!(is_valid_pin_format("012345"));
        assert!(!is_valid_pin_format("12345"));
        assert!(!is_valid_pin_format("1234567"));
        assert!(!is_valid_pin_format("12a456"));
        assert!(!is_valid_pin_format(""));
    }

    #[test]
    fn hash_and_verify_round_trip() {
        let hash = hash_pin("482913").expect("hash");
        assert!(verify_pin_hash("482913", &hash));
        assert!(!verify_pin_hash("000000", &hash));
        // argon2id encodes as `$argon2id$...`.
        assert!(hash.starts_with("$argon2id$"));
    }

    #[test]
    fn backoff_delay_is_monotonically_increasing_and_at_least_two_to_the_n_ms() {
        let mut previous = Duration::from_millis(0);
        for attempt in 1..=10u32 {
            let delay = backoff_delay(attempt);
            assert!(
                delay >= Duration::from_millis(2u64.pow(attempt)),
                "attempt {attempt}: {delay:?} < 2^{attempt} ms"
            );
            assert!(
                delay > previous,
                "attempt {attempt}: {delay:?} did not increase over {previous:?}"
            );
            previous = delay;
        }
    }

    #[test]
    fn backoff_delay_is_capped_and_starts_at_one_millisecond() {
        assert_eq!(backoff_delay(0), Duration::from_millis(1));
        assert_eq!(backoff_delay(30), MAX_BACKOFF, "2^30 ms would be ~12 days");
        assert_eq!(
            backoff_delay(1_000),
            MAX_BACKOFF,
            "never grows past the cap"
        );
    }

    #[test]
    fn session_store_issue_revoke_and_expiry() {
        let _guard = session_store_guard();
        revoke_all_sessions();
        let token = issue_session();
        assert!(is_valid_session(&token));

        revoke_session(&token);
        assert!(!is_valid_session(&token));

        // An expired-but-still-present token is treated as invalid.
        insert_session("stale-token");
        {
            let mut guard = sessions().tokens.lock().expect("lock");
            guard.insert(
                "stale-token".to_string(),
                SystemTime::now() - Duration::from_secs(1),
            );
        }
        assert!(!is_valid_session("stale-token"));

        assert!(!is_valid_session(""));
        revoke_all_sessions();
    }

    #[test]
    fn require_session_rejects_empty_and_unknown_tokens() {
        let _guard = session_store_guard();
        revoke_all_sessions();
        assert!(require_session("").is_err());
        assert!(require_session("not-a-real-token").is_err());
        let token = issue_session();
        assert!(require_session(&token).is_ok());
        revoke_all_sessions();
    }

    #[test]
    fn constant_time_eq_matches_exact_strings_only() {
        assert!(constant_time_eq("123456", "123456"));
        assert!(!constant_time_eq("123456", "123457"));
        assert!(!constant_time_eq("123456", "12345"));
    }

    fn headers(pairs: &[(&str, &str)]) -> axum::http::HeaderMap {
        let mut map = axum::http::HeaderMap::new();
        for (name, value) in pairs {
            map.insert(
                axum::http::HeaderName::from_bytes(name.as_bytes()).expect("valid header name"),
                value.parse().expect("valid header value"),
            );
        }
        map
    }

    #[test]
    fn session_from_headers_finds_the_cookie_among_others() {
        let h = headers(&[("cookie", "other=1; fh_session=abc-123; theme=dark")]);
        assert_eq!(session_from_headers(&h).as_deref(), Some("abc-123"));

        assert_eq!(session_from_headers(&headers(&[])), None);
        assert_eq!(
            session_from_headers(&headers(&[("cookie", "other=1")])),
            None
        );
    }

    #[test]
    fn same_origin_or_absent_allows_no_origin_and_matching_origin() {
        // No browser-origin signal at all (curl, the WS test harness): allowed.
        assert!(same_origin_or_absent(&headers(&[])));

        // Origin matches Host: same-origin.
        assert!(same_origin_or_absent(&headers(&[
            ("host", "10.0.0.5:8443"),
            ("origin", "https://10.0.0.5:8443"),
        ])));

        // Sec-Fetch-Site says same-origin even without an Origin header.
        assert!(same_origin_or_absent(&headers(&[(
            "sec-fetch-site",
            "same-origin"
        )])));
    }

    #[test]
    fn same_origin_or_absent_rejects_cross_origin() {
        assert!(!same_origin_or_absent(&headers(&[
            ("host", "10.0.0.5:8443"),
            ("origin", "https://evil.example"),
        ])));
        assert!(!same_origin_or_absent(&headers(&[(
            "sec-fetch-site",
            "cross-site"
        )])));
    }
}
