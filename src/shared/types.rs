use serde::{Deserialize, Serialize};

/// Which dashboard panel is currently maximized on the kiosk display.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum MaximizedView {
    #[default]
    None,
    Routine,
    Calendar,
    Whiteboard,
}

/// Number of family profiles supported by the hub (user ids 1..=4).
pub const FAMILY_PROFILE_COUNT: u32 = 4;

/// Display names for the four family profiles, indexed by `user_id - 1`.
pub const FAMILY_PROFILES: [&str; FAMILY_PROFILE_COUNT as usize] =
    ["Boy 1", "Boy 2", "Boy 3", "Boy 4"];

pub fn profile_name(user_id: u32) -> &'static str {
    FAMILY_PROFILES
        .get(user_id.saturating_sub(1) as usize)
        .copied()
        .unwrap_or("Family")
}

/// A routine template joined with today's completion state for one profile.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RoutineItemView {
    pub template_id: u32,
    pub title: String,
    pub description: String,
    pub icon_name: String,
    pub sort_order: i64,
    pub completed: bool,
}

/// A user created task, optionally backed by a photo taken on a phone.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct CustomTaskView {
    pub id: u32,
    pub user_id: u32,
    pub title: String,
    pub photo_path: Option<String>,
    pub is_completed: bool,
    pub created_at: String,
}

/// A single calendar entry pulled from Google Calendar.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct CalendarEvent {
    pub id: String,
    pub summary: String,
    /// RFC3339 timestamp, or `YYYY-MM-DD` for all day events.
    pub start: String,
    pub end: String,
    pub all_day: bool,
}

/// A stroke segment drawn on the collaborative whiteboard. Coordinates are
/// normalized to `0.0..=1.0` so clients with different resolutions agree.
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct StrokePoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct StrokeSegment {
    pub from: StrokePoint,
    pub to: StrokePoint,
    pub color: String,
    pub width: f64,
}

/// Messages exchanged over the `/ws` broadcast channel.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsMessage {
    /// A routine task was toggled for `user_id`; clients should refetch.
    RoutineUpdated { user_id: u32 },
    /// A whiteboard stroke segment was drawn by another client.
    Draw { segment: StrokeSegment },
    /// The whiteboard was cleared.
    ClearCanvas,
    /// Freshly polled calendar events for today.
    CalendarUpdated { events: Vec<CalendarEvent> },
}

/// Percentage of the daily routine completed by a single item.
pub fn routine_item_weight(total_items: usize) -> f64 {
    if total_items == 0 {
        0.0
    } else {
        100.0 / total_items as f64
    }
}

pub fn routine_progress(items: &[RoutineItemView]) -> f64 {
    let done = items.iter().filter(|i| i.completed).count();
    routine_item_weight(items.len()) * done as f64
}
