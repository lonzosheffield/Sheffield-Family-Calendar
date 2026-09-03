use serde::{Deserialize, Serialize};

/// Which dashboard panel is currently maximized on the kiosk display.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum MaximizedView {
    #[default]
    None,
    Routine,
    Calendar,
    Whiteboard,
    /// The ambient screensaver, forced on by T2.7's optional schedule
    /// (`ServerMessage::SetView { view: MaximizedView::Screensaver }`),
    /// independent of the idle-timeout that normally drives it
    /// (`client::components::screensaver`). Not reachable through D-pad
    /// navigation or the phone's TV Remote tab — only the schedule sends it.
    Screensaver,
    /// **HS3** — the School panel (`docs/homeschool/PLAN_HOMESCHOOL.md` H6,
    /// TV panel 4 of 4). Appended **last** so every existing variant keeps its
    /// serde name and its position; HS6 replaces the Boss's placeholder
    /// `from_view` arm with the real `TvPanel::Homeschool`.
    Homeschool,
}

/// Number of family profiles supported by the hub (user ids 1..=4).
pub const FAMILY_PROFILE_COUNT: u32 = 4;

/// Display names for the four family profiles, indexed by `user_id - 1`
/// (`migrations/0004_name_the_boys.sql`).
pub const FAMILY_PROFILES: [&str; FAMILY_PROFILE_COUNT as usize] =
    ["Isaiah", "Nathaniel", "Simeon", "Ezekiel"];

pub fn profile_name(user_id: u32) -> &'static str {
    FAMILY_PROFILES
        .get(user_id.saturating_sub(1) as usize)
        .copied()
        .unwrap_or("Family")
}

/// One family profile stored in `profiles` (T1.4, `migrations/0003_profiles.sql`).
///
/// Unlike [`FAMILY_PROFILES`] this is the real, mutable roster: it can hold
/// more than four rows (a 5th/6th profile is a T1.4 acceptance requirement),
/// each with its own display color and optional avatar image, and it is what
/// [`ServerMessage::ProfilesUpdated`] tells a client to refetch.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Profile {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub avatar: Option<String>,
    pub is_parent: bool,
    pub sort_order: i64,
}

/// Whether the parent PIN has been set yet (T1.4 first-run gate).
///
/// Asking this is also what causes the server to generate the first-run
/// setup code the first time anyone needs it: see
/// `crate::server::auth::ensure_setup_code`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SetupStatus {
    pub pin_set: bool,
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
    /// `YYYY-MM-DD`, or `None` for a task that never expires. **T2.5**: a
    /// task whose `due_date` is strictly before the server's today is
    /// filtered out of [`crate::server::db::custom_tasks`] before it ever
    /// reaches a client — the "daily auto-hide" PLAN v2 T2.5 asks for.
    /// Additive field (`docs/HANDOFF.md` "T2.5 → shared/types.rs"); every
    /// existing construction site got a mechanical `due_date: None` the same
    /// way T1.1's migration-count bumps were mechanical.
    #[serde(default)]
    pub due_date: Option<String>,
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

/// A whole pen stroke: every point captured between `pointerdown` and
/// `pointerup`, carried in **one** message (`docs/PROTOCOL.md`). Protocol v2
/// replaced v1's "one message per `pointermove`" segment with this batched
/// form — the change that stops a scribbling child from flooding the
/// broadcast channel and bricking the TV (G20 / R-06).
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Stroke {
    pub points: Vec<StrokePoint>,
    pub color: String,
    pub width: f64,
}

impl Stroke {
    pub fn new(color: String, width: f64) -> Self {
        Self {
            points: Vec::new(),
            color,
            width,
        }
    }

    /// The stroke expanded into the pairwise segments a canvas draws.
    pub fn segments(&self) -> Vec<StrokeSegment> {
        self.points
            .windows(2)
            .map(|pair| StrokeSegment {
                from: pair[0],
                to: pair[1],
                color: self.color.clone(),
                width: self.width,
            })
            .collect()
    }
}

/// Protocol version carried by [`ClientMessage::Hello`] and
/// [`ServerMessage::Hello`]. Raised to 2 by T1.2.
pub const PROTOCOL_VERSION: u8 = 2;

/// The single whiteboard (PURPLE §P5.5 default 15: one board; named boards cut).
pub const DEFAULT_BOARD_ID: i64 = 1;

/// Identity of one WebSocket connection.
///
/// **Minted by the server** at upgrade (a v4 UUID rendered as text) and handed
/// to the client in [`ServerMessage::Hello`]; the server stamps it onto every
/// `Draw`/`BoardCleared` it fans out. Clients drop messages whose `origin`
/// equals their own id, which is how self-echo is suppressed *without* a
/// client being able to claim another client's identity (W2 / R-13).
///
/// Held as a `String` rather than a `uuid::Uuid` because `uuid` is a
/// server-only optional dependency while this module also compiles to wasm;
/// the wire representation is identical either way (`docs/PROTOCOL.md`).
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub struct ClientId(pub String);

impl ClientId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ClientId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Opaque parent session token; issued and verified by T1.4's `auth.rs`.
pub type SessionToken = String;

/// Panel the kiosk should show. Aliased because `docs/PROTOCOL.md` and
/// PURPLE §P2c name this field's type `View`.
pub type View = MaximizedView;

/// Why the server asked a client to resynchronise.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResyncReason {
    /// The client fell behind the broadcast channel or its outbound queue.
    Lagged,
    /// The connection was (re-)established after a server restart.
    ServerRestart,
    /// The client asked for a resync itself.
    ClientRequested,
}

/// Everything a browser may send to the server over `/ws`.
///
/// The server **never** rebroadcasts one of these verbatim (G13): each is
/// parsed, rate-limited, authorised and translated into a [`ServerMessage`]
/// the server itself mints. Anything that does not parse as a `ClientMessage`
/// is dropped with a warning and reaches no other client.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Hello {
        protocol: u8,
    },
    Ping {
        nonce: u64,
    },
    Draw {
        board_id: i64,
        stroke: Stroke,
    },
    ClearBoard {
        board_id: i64,
    },
    SetView {
        view: View,
        auth: Option<SessionToken>,
    },
    SetActiveProfile {
        user_id: i64,
        auth: Option<SessionToken>,
    },
    RequestSnapshot {
        board_id: i64,
        since_seq: i64,
    },
}

/// Everything the server may send to a browser over `/ws`.
///
/// `server_time`, `today`, `date` and `last_update` are carried as strings
/// (RFC3339 and `YYYY-MM-DD` respectively) because `chrono` is a server-only
/// optional dependency; the bytes on the wire are identical to what
/// `DateTime<Local>` / `NaiveDate` would produce (`docs/PROTOCOL.md`).
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Hello {
        client_id: ClientId,
        protocol: u8,
        server_time: String,
        today: String,
    },
    Pong {
        nonce: u64,
    },
    Resync {
        reason: ResyncReason,
    },
    Draw {
        board_id: i64,
        seq: i64,
        origin: ClientId,
        stroke: Stroke,
    },
    BoardCleared {
        board_id: i64,
        seq: i64,
        origin: ClientId,
    },
    Snapshot {
        board_id: i64,
        seq: i64,
        strokes: Vec<Stroke>,
    },
    RoutineUpdated {
        user_id: i64,
        date: String,
    },
    TasksUpdated {
        user_id: i64,
        date: String,
    },
    ProfilesUpdated,
    CalendarUpdated {
        date: String,
    },
    DayRolled {
        date: String,
    },
    SetView {
        view: View,
    },
    SetActiveProfile {
        user_id: i64,
    },
    Health {
        stale: bool,
        last_update: String,
    },
    /// **HS3** — a homeschool log, enrollment or extra changed
    /// (`docs/homeschool/PLAN_HOMESCHOOL.md` H6 "Realtime"). Scoped like
    /// `RoutineUpdated`, but `user_ids` is a **list**: a Together tick fans a
    /// `lesson_log` row out to every boy in the group in one transaction (H4)
    /// and names all of them here.
    HomeschoolUpdated {
        user_ids: Vec<i64>,
        week: i64,
        date: String,
    },
    /// **HS3** — a curriculum's subjects or assignments changed (an inline
    /// edit, a subject's days/shared toggle, or an
    /// `import-curriculum --replace`). Unscoped by boy: it affects every
    /// enrollment on that curriculum.
    CurriculumUpdated {
        curriculum_id: i64,
    },
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

// ---------------------------------------------------------------------------
// HS3 — Homeschool ("School") DTOs
//
// `docs/homeschool/PLAN_HOMESCHOOL.md` §3 HS3. Appended, never interleaved:
// every type below is new, and the two additions above (`MaximizedView::
// Homeschool`, last; `ServerMessage::HomeschoolUpdated` / `CurriculumUpdated`,
// after `Health`) are the only edits HS3 makes to what was already here.
//
// Dates are `YYYY-MM-DD` strings for the same reason every other date in this
// module is: the date crate is a server-only optional dependency and these
// types compile to wasm (`docs/PROTOCOL.md`). The scheduling core that makes
// them is `crate::shared::homeschool`, which is where `Weekday`, `Category`,
// `LogStatus` and `TermNote` are defined.
// ---------------------------------------------------------------------------

use crate::shared::homeschool::{Category, LogStatus, TermNote, Weekday};

/// One dated occurrence of a curriculum subject for one boy (H3).
///
/// `assignment_id` is `None` for the untitled daily occurrence a subject with
/// no per-week row produces (H3 rule 3); `IFNULL(assignment_id, 0)` is what
/// the `lesson_log_occurrence` unique index keys on (rule 9). `part` is
/// `Some((k, n))` on a reading split over `n` days (rule 5). An **extra** is
/// never a `LessonOccurrence` (D-2) — see [`ExtraTask`] and [`DayItem`].
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct LessonOccurrence {
    pub subject_id: i64,
    pub assignment_id: Option<i64>,
    pub week: i64,
    pub scheduled_date: String,
    pub weekday: Weekday,
    pub category: Category,
    pub title: String,
    pub text: Option<String>,
    pub detail: Option<String>,
    pub source: Option<String>,
    pub icon_name: Option<String>,
    pub part: Option<(u32, u32)>,
    pub shared: bool,
    pub sort_order: i64,
    pub status: Option<LogStatus>,
    pub note: Option<String>,
}

/// A parent-authored task pinned to one boy and one date (H8, `lesson_extras`).
///
/// Independent of the curriculum pointer, so a parent can plan ahead into any
/// date; `status` is `None` while it is still to do.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ExtraTask {
    pub id: i64,
    pub user_id: i64,
    pub scheduled_date: String,
    pub title: String,
    pub category: Category,
    pub text: Option<String>,
    pub sort_order: i64,
    pub status: Option<LogStatus>,
    pub note: Option<String>,
}

/// One row of a boy's day: either a curriculum occurrence or one of his
/// parent-added tasks (D-2).
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DayItem {
    Lesson(LessonOccurrence),
    Extra(ExtraTask),
}

/// One boy's part of the Today view (H3 rule 8, H6 §4).
///
/// `due_today` is everything dated today, `catch_up` everything earlier in the
/// week still unlogged (daily work included — R-13/P-11), `done` everything
/// with a log row. The counts drive the header chip
/// `14 done · 2 skipped / 22`.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct BoyToday {
    pub user_id: i64,
    pub name: String,
    pub due_today: Vec<DayItem>,
    pub catch_up: Vec<DayItem>,
    pub done: Vec<DayItem>,
    pub done_count: u32,
    pub skipped_count: u32,
    pub total_count: u32,
}

/// A `shared` occurrence rendered **once** under Together, with the boys it
/// covers (H4). Partial completion shows "2 of 3".
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct TogetherOccurrence {
    pub occurrence: LessonOccurrence,
    pub user_ids: Vec<i64>,
    pub done_user_ids: Vec<i64>,
}

/// Every enrollment sharing `(curriculum_id, current_week)` (H4), with the
/// state the header chip and the Finish-week nudge need (H2).
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct TogetherGroup {
    pub curriculum_id: i64,
    pub curriculum_name: String,
    pub week: i64,
    pub weeks: i64,
    pub term: i64,
    pub week_started_on: String,
    pub paused: bool,
    pub year_complete: bool,
    pub can_finish_week: bool,
    pub days_on_week: u32,
    pub together: Vec<TogetherOccurrence>,
    pub boys: Vec<BoyToday>,
    pub term_notes: Vec<TermNote>,
}

/// What `get_homeschool_today` returns: every group, every boy, one date.
///
/// `anyone_enrolled = false` is the empty state ("No school plan yet"), not an
/// error (HS4 accept (j)).
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct HomeschoolTodayView {
    pub date: String,
    pub is_school_day: bool,
    pub anyone_enrolled: bool,
    pub groups: Vec<TogetherGroup>,
}

/// One boy's enrollment as School settings renders it. `enrolled = false`
/// leaves the curriculum fields at their zero values.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct EnrollmentView {
    pub user_id: i64,
    pub enrolled: bool,
    pub curriculum_id: i64,
    pub curriculum_name: String,
    pub current_week: i64,
    pub weeks: i64,
    pub week_started_on: String,
    /// The `MTWRFSU` letters, as stored in `enrollments.school_days`.
    pub school_days: String,
    pub paused: bool,
}

/// One row of the curriculum picker.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct CurriculumSummary {
    pub id: i64,
    pub slug: String,
    pub name: String,
    pub weeks: i64,
    pub term_weeks: i64,
    pub subject_count: i64,
}

/// One subject's editable schedule in School settings (H6).
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct SubjectSetting {
    pub subject_id: i64,
    pub name: String,
    pub category: Category,
    /// The `MTWRFSU` letters, as stored in `subjects.days`.
    pub days: String,
    pub shared: bool,
}

/// One subject's row in the Year view's grid.
///
/// `cells.len() == WeekGrid::days.len()`; a cell holds every occurrence dealt
/// to that day (two, for the fixture's `Twice Told` on a Tuesday).
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct WeekGridRow {
    pub subject_id: i64,
    pub title: String,
    pub category: Category,
    pub shared: bool,
    pub cells: Vec<Vec<LessonOccurrence>>,
}

/// The Year view's subject × school-day grid for one week (H6).
///
/// `dated = false` (§4 default 17) means this is **not** the current week: the
/// dates are advisory and the surface renders neither them nor a checkbox.
/// `free_read` subjects have no row (H3 rule 6).
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct WeekGrid {
    pub week: i64,
    pub weeks: i64,
    pub term: i64,
    pub dated: bool,
    pub days: Vec<Weekday>,
    pub rows: Vec<WeekGridRow>,
}

/// One cell of the Month view (H6). `total` is `Some` **only** when
/// `in_current_week`: a past week's plan is not reconstructed and a future
/// week has not been dealt out.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct MonthDay {
    pub date: String,
    pub weekday: Weekday,
    pub is_school_day: bool,
    pub in_current_week: bool,
    pub week: Option<i64>,
    pub done: u32,
    pub total: Option<u32>,
    pub extras: u32,
}

/// A Mon–Fri month grid for **exactly one boy** (§4 default 17).
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct MonthView {
    pub year: i32,
    pub month: u32,
    pub user_id: i64,
    pub days: Vec<MonthDay>,
}
