//! The kiosk's "updated HH:MM" line and its red disconnected badge (D8).
//!
//! This is the client-side port of `crate::server::health::StalenessTracker`,
//! assigned to T2.1 by the Boss note at the wave 1-b close (`docs/HANDOFF.md`,
//! "T1.7 → T2.1"): `src/server/health.rs` is `#[cfg(feature = "server")]` and
//! `std::time::Instant` does not exist on `wasm32`, so the television cannot
//! use the server's struct directly. The **semantics are identical** and are
//! asserted here against the same numbers (on past 90 s, off within 2 s); the
//! only difference is that "now" arrives as milliseconds from the same
//! performance clock `crate::client::realtime::now_millis` already provides.
//!
//! # What counts as proof of life
//!
//! The badge is, per that Boss decision, `!connected || tracker.is_stale(now)`.
//! Two independent signals feed it:
//!
//! * **`connected`** — `RealtimeBus::connected`, which the reconnect
//!   supervisor clears the moment the socket drops. The client's own
//!   heartbeat (20 s, dead at two missed `Pong`s) tears down a silent socket
//!   inside ~45 s, so this alone lights the badge well before 90 s in the
//!   common case: unplugged network, hub rebooting, hub crashed.
//! * **The tracker** — fed by [`TvStaleness::record_message`] from every
//!   observable answer the hub gives the television: any bus signal that
//!   moves, and the periodic [`super::clock::tv_clock`] poll, which is a real
//!   server round trip on the same origin and therefore a real liveness
//!   probe. It covers the case `connected` cannot see: a socket that is still
//!   *open* to a hub that has stopped answering.
//!
//! Nothing here latches. Staleness is recomputed from "how long ago was the
//! last message" on every tick, which is what makes the badge clear inside
//! one tick — comfortably inside D8's 2 s — of the hub coming back.

/// D8 / PURPLE §P5.5 default 32: more than this long without a word from the
/// hub and the badge lights. Deliberately the same 90 s as
/// `crate::server::health::STALENESS_THRESHOLD`; `tests/tv_tests.rs` asserts
/// the two have not drifted apart.
pub const STALENESS_THRESHOLD_MS: u64 = 90 * 1_000;

/// How often the kiosk re-evaluates the badge. Well under D8's "off within
/// 2 s" so the badge clears on the first tick after the hub answers.
pub const BADGE_TICK_MS: u64 = 1_000;

/// Pure, time-injected transition logic for the disconnected badge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TvStaleness {
    last_message_ms: u64,
}

impl TvStaleness {
    /// Start the tracker as of `now_ms` — "the page just loaded, badge off".
    pub fn new(now_ms: u64) -> Self {
        Self {
            last_message_ms: now_ms,
        }
    }

    /// The hub said something. Any message is proof of life.
    pub fn record_message(&mut self, now_ms: u64) {
        self.last_message_ms = now_ms;
    }

    /// Milliseconds since the last message.
    pub fn silence_ms(&self, now_ms: u64) -> u64 {
        now_ms.saturating_sub(self.last_message_ms)
    }

    /// Should the badge be showing at `now_ms`?
    pub fn is_stale(&self, now_ms: u64) -> bool {
        self.silence_ms(now_ms) > STALENESS_THRESHOLD_MS
    }
}

/// The badge's whole rule in one place, so the shell and the tests cannot
/// disagree about it.
pub fn badge_is_lit(connected: bool, tracker: &TvStaleness, now_ms: u64) -> bool {
    !connected || tracker.is_stale(now_ms)
}

/// What the permanent status line says.
///
/// It is *permanent* (D8): the kiosk always shows when it last heard from the
/// hub, whether or not anything is wrong, so a parent glancing at the
/// television can tell "quiet morning" from "frozen since 06:10" without
/// pressing anything.
pub fn status_line(updated_at: Option<&str>) -> String {
    match updated_at {
        Some(hhmm) => format!("updated {hhmm}"),
        None => "updated —".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: u64 = 1_000;

    #[test]
    fn the_badge_stays_off_through_ninety_seconds_of_silence() {
        let tracker = TvStaleness::new(0);
        assert!(!tracker.is_stale(0));
        assert!(!tracker.is_stale(45 * SECOND));
        assert!(
            !tracker.is_stale(90 * SECOND),
            "exactly 90 s must not yet be stale — the threshold is *more than* 90 s"
        );
    }

    #[test]
    fn the_badge_lights_past_ninety_seconds_of_silence() {
        let tracker = TvStaleness::new(0);
        assert!(tracker.is_stale(90 * SECOND + 1));
        assert!(tracker.is_stale(600 * SECOND));
    }

    #[test]
    fn the_badge_clears_within_two_seconds_of_the_hub_answering() {
        let mut tracker = TvStaleness::new(0);
        let long_silence = 300 * SECOND;
        assert!(tracker.is_stale(long_silence));

        tracker.record_message(long_silence);
        assert!(!tracker.is_stale(long_silence));
        assert!(!tracker.is_stale(long_silence + 2 * SECOND));
        const {
            // The badge only *looks* like it cleared within 2 s if the kiosk
            // re-evaluates at least that often.
            assert!(BADGE_TICK_MS <= 2 * SECOND);
        }
    }

    #[test]
    fn each_message_restarts_the_whole_window() {
        let mut tracker = TvStaleness::new(0);
        tracker.record_message(89 * SECOND);
        // Without the reset this is 179 s after the start — long stale.
        assert!(!tracker.is_stale(179 * SECOND));
        assert!(tracker.is_stale(180 * SECOND));
    }

    #[test]
    fn a_dropped_socket_lights_the_badge_without_waiting_for_the_threshold() {
        let tracker = TvStaleness::new(0);
        assert!(!badge_is_lit(true, &tracker, SECOND));
        assert!(badge_is_lit(false, &tracker, SECOND));
    }

    #[test]
    fn the_status_line_is_permanent_even_before_the_first_answer() {
        assert_eq!(status_line(Some("07:42")), "updated 07:42");
        assert_eq!(status_line(None), "updated —");
    }
}
