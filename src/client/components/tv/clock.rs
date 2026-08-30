//! The kiosk's clock, and with it its liveness probe.
//!
//! D8 asks the television for a **permanent "updated HH:MM"**. Two things
//! make that harder than it looks on a wasm client:
//!
//! 1. **The time has to be the hub's.** PURPLE §P5.5 default 14 — "all
//!    surfaces display **server-local** time, never device-local". A Fire TV
//!    that has never seen the internet may be hours out.
//! 2. **The realtime bus cannot supply it.** `ServerMessage::Hello` carries
//!    `server_time`, but `RealtimeBus::apply` (T1.2, `src/client/realtime.rs`,
//!    which T2.1 does not own) discards it, and the only other server→client
//!    traffic on an idle socket is `Pong`, which sets nothing observable.
//!
//! So the television asks. [`tv_clock`] is a tiny server function returning
//! the hub's own wall clock; the shell polls it every
//! [`CLOCK_POLL_SECS`] seconds, which is both where "updated HH:MM" comes
//! from and the round trip that feeds
//! [`super::staleness::TvStaleness::record_message`]. One call, two jobs:
//! the badge now has a real server pulse rather than an inference from the
//! absence of one.
//!
//! It is deliberately declared here rather than in `src/server/api/` — that
//! directory's modules are owned by T1.4/T1.5/T2.4/T2.7 (PURPLE §P4) and a
//! wave-2 task may not edit them. `docs/HANDOFF.md` records the request to
//! fold it into `api/` (and to make the server broadcast the protocol's
//! already-specified `ServerMessage::Health`) when Boss next touches those
//! files.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

/// How often the kiosk asks the hub for the time. Comfortably inside the
/// 90 s staleness threshold so a healthy hub never trips the badge, and
/// cheap: one small JSON round trip on the LAN, three times a minute.
pub const CLOCK_POLL_SECS: u64 = 20;

/// The hub's wall clock, as the television displays it.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct TvClock {
    /// Server-local `HH:MM`, 24-hour.
    pub hhmm: String,
    /// Server-local `YYYY-MM-DD`, so the kiosk's header can name the day
    /// without a second round trip.
    pub date: String,
}

/// The hub's own local time (PURPLE §P5.5 default 14).
#[server(endpoint = "tv_clock")]
pub async fn tv_clock() -> Result<TvClock, ServerFnError> {
    #[cfg(feature = "server")]
    {
        let now = chrono::Local::now();
        Ok(TvClock {
            hhmm: now.format("%H:%M").to_string(),
            date: now.format("%Y-%m-%d").to_string(),
        })
    }
    #[cfg(not(feature = "server"))]
    {
        unreachable!("server function bodies only run on the server")
    }
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_hub_reports_its_own_local_time_in_both_formats() {
        let clock = tv_clock().await.expect("the hub knows what time it is");

        let (hours, minutes) = clock.hhmm.split_once(':').expect("HH:MM");
        assert_eq!(hours.len(), 2, "{}", clock.hhmm);
        assert_eq!(minutes.len(), 2, "{}", clock.hhmm);
        assert!(hours.parse::<u32>().expect("hours") < 24);
        assert!(minutes.parse::<u32>().expect("minutes") < 60);

        assert_eq!(clock.date.len(), 10, "{}", clock.date);
        assert_eq!(
            clock.date,
            chrono::Local::now().format("%Y-%m-%d").to_string(),
            "the clock must be the *server's* local day, not UTC"
        );
    }
}
