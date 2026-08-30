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
//! The server function itself lives in `crate::server::api::tv` (moved
//! there by Boss at the 2-a close, `docs/HANDOFF.md` T2.1 H-22) and is
//! re-exported here so the shell's imports read as the kiosk's own clock.

pub use crate::server::api::tv::{tv_clock, TvClock};

/// How often the kiosk asks the hub for the time. Comfortably inside the
/// 90 s staleness threshold so a healthy hub never trips the badge, and
/// cheap: one small JSON round trip on the LAN, three times a minute.
pub const CLOCK_POLL_SECS: u64 = 20;
