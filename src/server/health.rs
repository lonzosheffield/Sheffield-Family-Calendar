//! `/health` — machine-readable liveness for the hub (PLAN v2 D8/R-30, task
//! T1.7). Nothing else tells anyone the hub is broken (R-30): this is the
//! single place a monitor, the owner's browser, or the TV's own badge can ask
//! "is the hub actually working right now?" and get a typed answer.
//!
//! Two things ship here:
//!
//! 1. [`health_handler`] — the real `GET /health` body, replacing
//!    `router::health_stub` (`docs/HANDOFF.md` "H-14. For T1.7 —
//!    `/health` cert fields"). Eight keys, every one independently sourced so
//!    a failure in one (the database, say) does not blank out the rest: `db`,
//!    `last_google_poll`, `cert_not_after`, `days_to_expiry`, `disk_free_bytes`,
//!    `ws_clients`, `uptime_seconds`, `migration_version`. The database is
//!    reachability-checked with a real `SELECT 1` against the **read** pool
//!    (`docs/HANDOFF.md` H-9 — reads move to `db::read_pool()`), and the HTTP
//!    status itself carries the same signal: 200 when it answered, 503 when
//!    it did not.
//! 2. [`StalenessTracker`] — the pure, time-injected state machine behind the
//!    TV's permanent "updated HH:MM" / red disconnected badge (D8): stale at
//!    more than 90 s of silence, un-stale the instant a message arrives. It
//!    takes no dependency on a socket, a clock, or Dioxus, so it is exercised
//!    entirely by the unit tests at the bottom of this file — the same style
//!    `server::api::realtime::{TokenBucket, RateLimiter}` already use. The TV
//!    kiosk view (`src/client/components/tv/**`, T2.1's file) is the eventual
//!    caller; wiring it in is left to that task per `docs/reviews/PURPLE_TEAM.md`
//!    §P4 (T1.7 owns only this file).

use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use crate::server::config::FamilyHubConfig;
use crate::server::db;
use crate::server::pki::CertProvider;

// ---------------------------------------------------------------------------
// GET /health
// ---------------------------------------------------------------------------

/// The eight keys PURPLE_TEAM.md §P3 T1.7 names, in the order it names them:
/// "db reachable, last successful Google poll, cert `not_after`,
/// days-to-expiry, disk free on the data volume, connected WS clients,
/// uptime, migration version".
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HealthBody {
    pub db: bool,
    pub last_google_poll: Option<String>,
    pub cert_not_after: Option<String>,
    pub days_to_expiry: Option<i64>,
    pub disk_free_bytes: u64,
    pub ws_clients: usize,
    pub uptime_seconds: u64,
    pub migration_version: Option<i64>,
}

/// `GET /health`. 200 when the database answered, 503 when it did not — the
/// status line carries the same signal as the `db` key so a monitor that only
/// looks at the status code still sees the hub is unwell.
pub async fn health_handler(config: FamilyHubConfig) -> Response {
    let db = db_reachable().await;
    let migration_version = if db { migration_version().await } else { None };
    let (cert_not_after, days_to_expiry) = cert_status(&config);

    let body = HealthBody {
        db,
        last_google_poll: last_google_poll(),
        cert_not_after,
        days_to_expiry,
        disk_free_bytes: disk_free_bytes(&config.data_dir),
        ws_clients: crate::server::api::realtime::connected_clients(),
        uptime_seconds: uptime_seconds(),
        migration_version,
    };

    let status = if db {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    let json = serde_json::to_string(&body).unwrap_or_else(|_| "{}".to_string());
    (status, [(header::CONTENT_TYPE, "application/json")], json).into_response()
}

/// A real round trip against the **read** pool (`docs/HANDOFF.md` H-9), not
/// just "did the `OnceCell` succeed once at boot" — a pool that opened fine
/// and was later closed (or has lost its file) must also report `db: false`.
async fn db_reachable() -> bool {
    match db::read_pool().await {
        Ok(pool) => sqlx::query("SELECT 1").execute(pool).await.is_ok(),
        Err(_) => false,
    }
}

async fn migration_version() -> Option<i64> {
    let pool = db::read_pool().await.ok()?;
    db::migration_version(pool).await.ok().flatten()
}

/// `(not_after, days_to_expiry)`, both `None` if the local PKI cannot be
/// opened. `router::pki_for` is the same process-wide cache the HTTPS
/// listener and `/ca.crt` resolve, so this can never report a certificate
/// other than the one actually being served (`docs/HANDOFF.md` H-14).
fn cert_status(config: &FamilyHubConfig) -> (Option<String>, Option<i64>) {
    match crate::server::router::pki_for(&config.pki_dir()) {
        Ok(pki) => {
            let leaf = pki.current();
            (Some(rfc3339(leaf.not_after)), Some(leaf.days_remaining()))
        }
        Err(err) => {
            tracing::warn!(%err, "/health: could not load the local certificate authority");
            (None, None)
        }
    }
}

/// `time::OffsetDateTime` -> RFC3339 via `chrono` (already a server
/// dependency) rather than pulling in `time`'s `formatting` feature for one
/// call site.
fn rfc3339(dt: time::OffsetDateTime) -> String {
    chrono::DateTime::from_timestamp(dt.unix_timestamp(), 0)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Uptime
// ---------------------------------------------------------------------------

static STARTED_AT: OnceLock<Instant> = OnceLock::new();

/// Record the process start instant. Called once from `router::run` (the
/// closest thing to "the first line of `main`" T1.7 is allowed to edit —
/// `src/main.rs` itself is frozen by T0.6, `docs/reviews/PURPLE_TEAM.md` §P4).
/// Idempotent: a second call (e.g. a test that never runs `run`) is a no-op,
/// and [`uptime_seconds`] falls back to seeding the clock on its own first
/// call so it is never a hard dependency on this being called at all.
pub fn mark_started() {
    STARTED_AT.get_or_init(Instant::now);
}

/// Seconds since [`mark_started`] (or, failing that, since the first call to
/// this function — still monotonic, just measuring "since `/health` was
/// first asked" instead of "since boot").
pub fn uptime_seconds() -> u64 {
    STARTED_AT.get_or_init(Instant::now).elapsed().as_secs()
}

// ---------------------------------------------------------------------------
// Last successful Google Calendar poll
// ---------------------------------------------------------------------------

static LAST_GOOGLE_POLL: OnceLock<Mutex<Option<String>>> = OnceLock::new();

fn last_google_poll_cell() -> &'static Mutex<Option<String>> {
    LAST_GOOGLE_POLL.get_or_init(|| Mutex::new(None))
}

/// Record a successful Google Calendar poll. **Seam, not wired in**:
/// `src/server/calendar.rs` is T2.4's file (`docs/reviews/PURPLE_TEAM.md`
/// §P4), so T1.7 cannot add the call itself; `docs/HANDOFF.md` asks T2.4 to
/// call this once per successful `fetch_today` inside `store_events`. Until
/// then `/health`'s `last_google_poll` is honestly `null` — which is also the
/// correct value today, since PURPLE §P5.5 default 24 ("no Google service
/// account assumed") means the poller does not even start in this run
/// (A4 — no credentials exist).
pub fn record_google_poll_success(at: chrono::DateTime<chrono::Local>) {
    if let Ok(mut guard) = last_google_poll_cell().lock() {
        *guard = Some(at.to_rfc3339());
    }
}

fn last_google_poll() -> Option<String> {
    last_google_poll_cell()
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
}

// ---------------------------------------------------------------------------
// Disk free space
// ---------------------------------------------------------------------------

/// Bytes free on the volume holding `path`, for the owner's monitor to alarm
/// on before a full disk takes the hub down (R-18: unbounded growth is
/// otherwise silent). A direct `GetDiskFreeSpaceExW` call — the OS's own API,
/// not a spawned process or a new crate — the same reasoning T1.3's
/// `icacls.exe` shell-out was accepted under (`docs/HANDOFF.md` H-12,
/// ratified at the wave 1-a close): an OS built-in invoked at runtime is a
/// declared exception, not an undeclared non-Rust component.
#[cfg(windows)]
fn disk_free_bytes(path: &Path) -> u64 {
    use std::os::windows::ffi::OsStrExt;

    #[allow(non_snake_case)]
    #[link(name = "kernel32")]
    extern "system" {
        fn GetDiskFreeSpaceExW(
            directory_name: *const u16,
            free_bytes_available_to_caller: *mut u64,
            total_number_of_bytes: *mut u64,
            total_number_of_free_bytes: *mut u64,
        ) -> i32;
    }

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let mut free_available_to_caller: u64 = 0;
    // SAFETY: `wide` is a valid, NUL-terminated UTF-16 buffer that outlives
    // the call; the two `total_*` out-params are optional per the Win32
    // contract and null is the documented way to skip them.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free_available_to_caller,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };

    if ok != 0 {
        free_available_to_caller
    } else {
        tracing::warn!(
            path = %path.display(),
            error = std::io::Error::last_os_error().to_string(),
            "/health: GetDiskFreeSpaceExW failed"
        );
        0
    }
}

/// Off Windows (e.g. `cargo check` in a Linux container) there is no portable
/// stdlib call for free disk space and this project targets Windows only
/// (PLAN v2 D9); report 0 rather than pull in a crate for a platform this
/// hub never runs on.
#[cfg(not(windows))]
fn disk_free_bytes(_path: &Path) -> u64 {
    0
}

// ---------------------------------------------------------------------------
// The TV's staleness badge state machine (D8)
// ---------------------------------------------------------------------------

/// D8 / PURPLE §P5.5 default: the badge lights at more than this long since
/// the last message.
pub const STALENESS_THRESHOLD: Duration = Duration::from_secs(90);

/// Pure, time-injected transition logic for the TV's disconnected badge
/// (D8: "permanent 'updated HH:MM' + red disconnected badge after 90 s of
/// silence"). Deliberately free of any socket, clock, or Dioxus dependency —
/// like `server::api::realtime::{TokenBucket, RateLimiter}` — so the 90 s /
/// 2 s transitions in PURPLE §P3 T1.7's acceptance test are provable without
/// a real 90-second sleep.
///
/// The state is exactly one instant, `last_message_at`: staleness is always
/// recomputed from "how long ago was that", never latched, so a caller that
/// re-evaluates at least every couple of seconds (a short interval timer on
/// the TV) sees the badge clear within that same interval of a message
/// arriving — never later than the next tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StalenessTracker {
    last_message_at: Instant,
}

impl StalenessTracker {
    /// Start the tracker as of `now` (i.e. "connection just opened, badge is
    /// off").
    pub fn new(now: Instant) -> Self {
        Self {
            last_message_at: now,
        }
    }

    /// Any message from the server (`Hello`, `Pong`, a broadcast — anything)
    /// counts as proof of life and clears the badge immediately.
    pub fn record_message(&mut self, now: Instant) {
        self.last_message_at = now;
    }

    /// Should the badge be showing at `now`?
    pub fn is_stale(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.last_message_at) > STALENESS_THRESHOLD
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------
    // StalenessTracker (PURPLE §P3 T1.7: "a unit test on the staleness
    // state machine asserts the badge turns on at > 90 s and off within
    // 2 s of a message")
    // -----------------------------------------------------------------

    #[test]
    fn badge_stays_off_for_ninety_seconds_of_silence() {
        let t0 = Instant::now();
        let tracker = StalenessTracker::new(t0);

        assert!(!tracker.is_stale(t0));
        assert!(!tracker.is_stale(t0 + Duration::from_secs(45)));
        assert!(
            !tracker.is_stale(t0 + Duration::from_secs(90)),
            "exactly 90 s must not yet be stale — the threshold is *more than* 90 s"
        );
    }

    #[test]
    fn badge_turns_on_past_ninety_seconds_of_silence() {
        let t0 = Instant::now();
        let tracker = StalenessTracker::new(t0);

        assert!(
            tracker.is_stale(t0 + Duration::from_secs(91)),
            "91 s of silence must be stale"
        );
        assert!(tracker.is_stale(t0 + Duration::from_secs(600)));
    }

    #[test]
    fn badge_turns_off_within_two_seconds_of_a_message() {
        let t0 = Instant::now();
        let mut tracker = StalenessTracker::new(t0);

        // Let it go stale.
        let long_silence = t0 + Duration::from_secs(500);
        assert!(tracker.is_stale(long_silence));

        // A message arrives...
        tracker.record_message(long_silence);
        // ...and the very next check, well inside 2 s, is no longer stale.
        assert!(!tracker.is_stale(long_silence + Duration::from_millis(500)));
        assert!(!tracker.is_stale(long_silence + Duration::from_secs(2)));
    }

    #[test]
    fn a_fresh_message_resets_the_ninety_second_clock() {
        let t0 = Instant::now();
        let mut tracker = StalenessTracker::new(t0);

        let almost_stale = t0 + Duration::from_secs(89);
        tracker.record_message(almost_stale);

        // Without the reset this would be 179 s after `t0` — long stale.
        // With it, it is only 90 s after the fresh message: still not stale.
        assert!(!tracker.is_stale(almost_stale + Duration::from_secs(90)));
        assert!(tracker.is_stale(almost_stale + Duration::from_secs(91)));
    }

    // -----------------------------------------------------------------
    // Uptime / last-poll seams (pure, no server boot required)
    // -----------------------------------------------------------------

    #[test]
    fn last_google_poll_round_trips_through_the_recorder() {
        // This mutates process-wide state shared with every other test in
        // this binary, so only assert the shape (present vs absent), not an
        // exact value another test may have already written.
        record_google_poll_success(chrono::Local::now());
        assert!(
            last_google_poll().is_some(),
            "recording a poll must make last_google_poll() Some"
        );
    }

    #[test]
    fn rfc3339_formats_a_known_instant() {
        let dt =
            time::OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("valid unix timestamp");
        let formatted = rfc3339(dt);
        assert!(
            formatted.starts_with("2023-11-14"),
            "unexpected RFC3339 rendering: {formatted}"
        );
    }
}
