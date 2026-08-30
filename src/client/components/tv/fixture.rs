//! The canonical kiosk the golden files describe.
//!
//! PURPLE §P3 T2.1 (a) wants "the **exact** ordered list of focusable element
//! ids" pinned to a committed file. A focus order is only exact for a given
//! set of data, so the data has to be committed too — that is this module.
//!
//! It is shaped like the real hub on a real morning: the four Sheffield boys
//! (`migrations/0003_profiles.sql`), the eight-item morning routine
//! (`crate::server::db::SHEFFIELD_MORNING_ROUTINE`), two extra photo tasks
//! and three calendar entries. `tests/tv_tests.rs` asserts the routine length
//! here still matches the seeded routine, so adding a ninth item to the
//! family's morning fails the golden test rather than silently invalidating
//! it.
//!
//! Compiled only for tests and for the server build (which is what runs
//! `cargo test`); it is never part of the wasm bundle the television loads.

use crate::client::components::calendar::CalendarState;
use crate::shared::types::{CalendarEvent, CustomTaskView, RoutineItemView};

use super::model::{TvModel, TvProfile, TvState};

/// The four child profiles seeded by `migrations/0003_profiles.sql`.
pub const CANONICAL_PROFILES: [(i64, &str, &str); 4] = [
    (1, "Boy 1", "#2672B3"),
    (2, "Boy 2", "#8BB5DA"),
    (3, "Boy 3", "#E86A58"),
    (4, "Boy 4", "#F4D03F"),
];

/// Titles standing in for the eight seeded routine templates. Only the count
/// and the `template_id`s matter to the focus order; the prose is trimmed so
/// the golden file stays readable.
pub const CANONICAL_ROUTINE: [&str; 8] = [
    "Wake up and thank God for the day!",
    "Make your bed",
    "Brush your teeth",
    "Drink 8 ounces of water.",
    "Eat protein for breakfast.",
    "Move your body for at least 30 minutes.",
    "Read your Bible",
    "Start your school work.",
];

/// The kiosk every golden file and every focus assertion is written against.
pub fn canonical_model() -> TvModel {
    let profiles = CANONICAL_PROFILES
        .iter()
        .map(|(id, name, color)| TvProfile {
            id: *id,
            name: (*name).to_string(),
            color: (*color).to_string(),
        })
        .collect();

    let routine = CANONICAL_ROUTINE
        .iter()
        .enumerate()
        .map(|(index, title)| RoutineItemView {
            template_id: index as u32 + 1,
            title: (*title).to_string(),
            description: "Sheffield morning routine".to_string(),
            icon_name: "sun".to_string(),
            sort_order: index as i64 + 1,
            // The third item is done: the golden render therefore covers the
            // completed *and* the outstanding row styling in one pass.
            completed: index == 2,
        })
        .collect();

    let tasks = vec![
        CustomTaskView {
            id: 41,
            user_id: 1,
            title: "Put the recycling out".to_string(),
            photo_path: None,
            is_completed: false,
            created_at: "2026-08-29T07:00:00".to_string(),
            // T2.5 added `due_date` to `CustomTaskView`; mechanical `None`
            // here, same as every other pre-existing construction site.
            due_date: None,
        },
        CustomTaskView {
            id: 42,
            user_id: 1,
            title: "Feed the dog".to_string(),
            photo_path: Some("/uploads/dog.jpg".to_string()),
            is_completed: true,
            created_at: "2026-08-29T07:05:00".to_string(),
            due_date: None,
        },
    ];

    let events = vec![
        CalendarEvent {
            id: "local-1".to_string(),
            summary: "Co-op maths".to_string(),
            start: "2026-08-29T09:30:00".to_string(),
            end: "2026-08-29T11:00:00".to_string(),
            all_day: false,
        },
        CalendarEvent {
            id: "google:AbC_12".to_string(),
            summary: "Dentist".to_string(),
            start: "2026-08-29T14:15:00".to_string(),
            end: "2026-08-29T15:00:00".to_string(),
            all_day: false,
        },
        CalendarEvent {
            id: "local-2".to_string(),
            summary: "Grandma visits".to_string(),
            start: "2026-08-29".to_string(),
            end: "2026-08-30".to_string(),
            all_day: true,
        },
    ];

    TvModel {
        profiles,
        routine,
        tasks,
        events: CalendarState::Ready(events),
        state: TvState::initial(),
        connected: true,
        stale: false,
        updated_at: Some("07:42".to_string()),
        join_url: Some("https://10.0.0.42:8443/m".to_string()),
        keys_debug: false,
        key_log: Vec::new(),
    }
}
