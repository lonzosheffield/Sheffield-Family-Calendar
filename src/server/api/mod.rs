//! Server functions and the realtime hub.
//!
//! **T1.2 split the former single `src/server/api.rs`** into one module per
//! feature so that the wave 1-b tasks own disjoint files
//! (`docs/reviews/PURPLE_TEAM.md` §P4: "T1.2 lands `api/mod.rs`,
//! `api/realtime.rs`, `api/routine.rs`, `api/profiles.rs`, `api/calendar.rs`,
//! `api/screensaver.rs` as part of its refactor, so T1.4 and T1.5 own
//! different files").
//!
//! | Module | Owner from here on |
//! | --- | --- |
//! | [`realtime`] | T1.2 (this task) — protocol v2, `docs/PROTOCOL.md` |
//! | [`routine`] | T1.5 (dates, idempotency, authorization) |
//! | [`profiles`] | T1.4 (profiles, settings, parent PIN) |
//! | [`calendar`] | T2.4 (calendar v2) |
//! | [`screensaver`] | T2.7 (screensaver completion) |
//!
//! Every server function is re-exported here, so call sites keep using
//! `crate::server::api::<name>` exactly as before the split.

pub mod calendar;
pub mod profiles;
#[cfg(feature = "server")]
pub mod realtime;
pub mod routine;
pub mod screensaver;

pub use calendar::get_today_events;
pub use routine::{
    create_photo_task, get_custom_tasks, get_daily_routine, today, toggle_custom_task,
    toggle_routine_task,
};
pub use screensaver::list_screensaver_images;

/// Shared `sqlx::Error` → `ServerFnError` conversion used by every module in
/// this directory.
#[cfg(feature = "server")]
pub(crate) fn to_server_error(err: sqlx::Error) -> dioxus::prelude::ServerFnError {
    dioxus::prelude::ServerFnError::new(err.to_string())
}
