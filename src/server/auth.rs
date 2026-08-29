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

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

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
    if pin_is_set(pool).await? {
        return Err(AuthError::PinAlreadySet);
    }
    let expected = db::get_setting(pool, SETUP_CODE_SETTING)
        .await?
        .ok_or(AuthError::InvalidSetupCode)?;
    if !constant_time_eq(setup_code, &expected) {
        return Err(AuthError::InvalidSetupCode);
    }
    if !is_valid_pin_format(pin) {
        return Err(AuthError::InvalidPinFormat);
    }

    let hash = hash_pin(pin)?;
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
    let hash = hash_pin(new_pin)?;
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
    let stored_hash = db::get_setting(pool, PIN_HASH_SETTING)
        .await?
        .ok_or(AuthError::PinNotSet)?;

    let correct = is_valid_pin_format(pin) && verify_pin_hash(pin, &stored_hash);

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
