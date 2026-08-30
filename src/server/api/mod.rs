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
//! | [`whiteboard`] | T2.3 (whiteboard v2 — new module, undo-own-last-stroke) |
//! | [`tv`] | T2.1 (kiosk clock; moved here from `client::components::tv::clock` by Boss at the 2-a close) |
//!
//! Every server function is re-exported here, so call sites keep using
//! `crate::server::api::<name>` exactly as before the split.

pub mod calendar;
pub mod profiles;
#[cfg(feature = "server")]
pub mod realtime;
pub mod routine;
pub mod screensaver;
pub mod tv;
pub mod whiteboard;

pub use calendar::{
    create_local_event, delete_local_event, get_calendar_week, get_events_for_day,
    get_today_events, update_local_event,
};
pub use profiles::{
    change_parent_pin, create_profile, delete_profile, list_profiles, parent_setup_status,
    rename_profile, set_initial_parent_pin, set_profile_color, verify_parent_pin,
};
pub use routine::{
    create_photo_task, get_custom_tasks, get_daily_routine, today, toggle_custom_task,
    toggle_routine_task,
};
pub use screensaver::list_screensaver_images;
pub use tv::tv_clock;
pub use whiteboard::undo_last_stroke;

/// Shared `sqlx::Error` → `ServerFnError` conversion used by every module in
/// this directory.
#[cfg(feature = "server")]
pub(crate) fn to_server_error(err: sqlx::Error) -> dioxus::prelude::ServerFnError {
    dioxus::prelude::ServerFnError::new(err.to_string())
}
