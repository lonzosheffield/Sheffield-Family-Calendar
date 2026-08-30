//! Server functions for the Fire TV kiosk (T2.1).
//!
//! [`tv_clock`] is the hub's own wall clock. D8 asks the television for a
//! **permanent "updated HH:MM"**, and PURPLE §P5.5 default 14 says every
//! surface shows **server-local** time, never device-local — a Fire TV that
//! has never seen the internet may be hours out. The kiosk shell
//! (`client::components::tv::shell`) polls this every
//! `client::components::tv::clock::CLOCK_POLL_SECS` seconds; the same round
//! trip is the badge's liveness probe until the server broadcasts
//! `ServerMessage::Health` on its own (`docs/HANDOFF.md`, T2.1 H-23).
//!
//! T2.1 declared this in `src/client/components/tv/clock.rs` because every
//! module in this directory was owned by another wave-2a task; Boss moved it
//! here at the 2-a close (T2.1 H-22). The endpoint name is unchanged.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

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
