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
use crate::shared::homeschool::{Category, LogStatus, Weekday};
use crate::shared::types::{
    BoyToday, CalendarEvent, CustomTaskView, DayItem, ExtraTask, HomeschoolTodayView,
    LessonOccurrence, RoutineItemView, TogetherGroup,
};

use super::model::{TvModel, TvProfile, TvState};

/// The four child profiles seeded by `migrations/0003_profiles.sql` and named
/// by `migrations/0004_name_the_boys.sql`.
pub const CANONICAL_PROFILES: [(i64, &str, &str); 4] = [
    (1, "Isaiah", "#2672B3"),
    (2, "Nathaniel", "#8BB5DA"),
    (3, "Simeon", "#E86A58"),
    (4, "Ezekiel", "#F4D03F"),
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

// ---------------------------------------------------------------------------
// HS6 — the School panel's canonical day
// ---------------------------------------------------------------------------

/// The date the School fixture is a Wednesday of (`weekday("2026-09-02")` is
/// `Wed`, which `tests/homeschool_tests.rs` pins independently).
pub const CANONICAL_SCHOOL_DATE: &str = "2026-09-02";
/// The Monday the fixture week is anchored on (H2: `week_started_on`).
pub const CANONICAL_SCHOOL_WEEK_STARTED_ON: &str = "2026-08-31";
/// The week the fixture boy is on, and the year's length.
pub const CANONICAL_SCHOOL_WEEK: i64 = 3;
/// Weeks in the fixture curriculum.
pub const CANONICAL_SCHOOL_WEEKS: i64 = 36;
/// The parent-added task on the fixture day (`lesson_extras`, H8).
pub const CANONICAL_EXTRA_ID: i64 = 501;
/// The boy the School fixture enrolls: profile 1, the first on the rail.
pub const CANONICAL_SCHOOL_USER_ID: i64 = 1;

/// **D-3**: the fixture day is exactly **11 lesson rows and 1 extra**, so the
/// School panel's body is 12 focusable rows and the ≤ 12-press reachability
/// search has a worst case worth asserting.
pub const CANONICAL_SCHOOL_LESSON_ROWS: usize = 11;

/// One occurrence, spelled out. Invented subject names only (N1): nothing in
/// this file names or paraphrases a real curriculum.
#[allow(clippy::too_many_arguments)]
fn occurrence(
    subject_id: i64,
    assignment_id: Option<i64>,
    title: &str,
    category: Category,
    icon_name: Option<&str>,
    text: Option<&str>,
    scheduled_date: &str,
    weekday: Weekday,
    part: Option<(u32, u32)>,
    shared: bool,
    sort_order: i64,
    status: Option<LogStatus>,
) -> LessonOccurrence {
    LessonOccurrence {
        subject_id,
        assignment_id,
        week: CANONICAL_SCHOOL_WEEK,
        scheduled_date: scheduled_date.to_string(),
        weekday,
        category,
        title: title.to_string(),
        text: text.map(str::to_string),
        detail: None,
        source: None,
        icon_name: icon_name.map(str::to_string),
        part,
        shared,
        sort_order,
        status,
        note: None,
        // The fixture pins nothing: every row here inherits its subject's days.
        days: None,
    }
}

/// The boy's own Wednesday: eight rows dated today, his parent-added task,
/// and three rows he did not get to on Tuesday.
///
/// One **shared** read-aloud sits in the middle of `due_today` on purpose:
/// H6/W-16 says a shared item is ticked on the phone by whoever holds the
/// book, so the television must not draw it and must not let the remote reach
/// it. `tests/tv_tests.rs` (g) asserts exactly that, which it could not do if
/// the fixture had no shared row at all.
pub fn canonical_school() -> HomeschoolTodayView {
    const TODAY: &str = CANONICAL_SCHOOL_DATE;
    const YESTERDAY: &str = "2026-09-01";

    let due_today: Vec<DayItem> = vec![
        DayItem::Lesson(occurrence(
            101,
            None,
            "Sums",
            Category::Daily,
            Some("math"),
            Some("Lesson 14"),
            TODAY,
            Weekday::Wed,
            None,
            false,
            1,
            None,
        )),
        DayItem::Lesson(occurrence(
            102,
            None,
            "Copywork",
            Category::Daily,
            None,
            None,
            TODAY,
            Weekday::Wed,
            None,
            false,
            2,
            // Ticked, so the golden render covers the stamped checkbox and the
            // outstanding one in a single pass — as the routine fixture does.
            Some(LogStatus::Done),
        )),
        DayItem::Lesson(occurrence(
            103,
            None,
            "Phonics",
            Category::Daily,
            None,
            None,
            TODAY,
            Weekday::Wed,
            None,
            false,
            3,
            None,
        )),
        DayItem::Lesson(occurrence(
            104,
            None,
            "Recitation",
            Category::Daily,
            Some("poetry"),
            None,
            TODAY,
            Weekday::Wed,
            None,
            false,
            4,
            None,
        )),
        DayItem::Lesson(occurrence(
            105,
            Some(9001),
            "Old Tales",
            Category::Reading,
            None,
            Some("Chapter four"),
            TODAY,
            Weekday::Wed,
            Some((1, 2)),
            false,
            5,
            None,
        )),
        DayItem::Lesson(occurrence(
            106,
            Some(9002),
            "Fables",
            Category::Reading,
            None,
            Some("Chapter two"),
            TODAY,
            Weekday::Wed,
            None,
            false,
            6,
            None,
        )),
        // Shared: rendered under Together on the phone, never here (W-16).
        DayItem::Lesson(occurrence(
            109,
            Some(9004),
            "Morning Hymn",
            Category::Weekly,
            Some("music"),
            Some("Verse two"),
            TODAY,
            Weekday::Wed,
            None,
            true,
            7,
            None,
        )),
        DayItem::Lesson(occurrence(
            107,
            Some(9003),
            "Painting",
            Category::Weekly,
            None,
            Some("Study one picture"),
            TODAY,
            Weekday::Wed,
            None,
            false,
            8,
            None,
        )),
        DayItem::Lesson(occurrence(
            108,
            None,
            "Nature Walk",
            Category::Weekly,
            Some("nature"),
            None,
            TODAY,
            Weekday::Wed,
            None,
            false,
            9,
            None,
        )),
        // H3 rule 10: extras join the boy's own lists by date, after the
        // curriculum rows `today_view` produced.
        DayItem::Extra(ExtraTask {
            id: CANONICAL_EXTRA_ID,
            user_id: CANONICAL_SCHOOL_USER_ID,
            scheduled_date: TODAY.to_string(),
            title: "Tidy the schoolroom".to_string(),
            category: Category::Daily,
            text: Some("Books back on the shelf".to_string()),
            sort_order: 1,
            status: None,
            note: None,
        }),
    ];

    let catch_up: Vec<DayItem> = vec![
        DayItem::Lesson(occurrence(
            101,
            None,
            "Sums",
            Category::Daily,
            Some("math"),
            Some("Lesson 13"),
            YESTERDAY,
            Weekday::Tue,
            None,
            false,
            1,
            None,
        )),
        DayItem::Lesson(occurrence(
            110,
            Some(9005),
            "Twice Told",
            Category::Reading,
            None,
            Some("Chapter nine"),
            YESTERDAY,
            Weekday::Tue,
            Some((2, 2)),
            false,
            2,
            None,
        )),
        DayItem::Lesson(occurrence(
            102,
            None,
            "Copywork",
            Category::Daily,
            None,
            None,
            YESTERDAY,
            Weekday::Tue,
            None,
            false,
            3,
            None,
        )),
    ];

    let done: Vec<DayItem> = due_today
        .iter()
        .filter(|item| match item {
            DayItem::Lesson(lesson) => lesson.status.is_some(),
            DayItem::Extra(extra) => extra.status.is_some(),
        })
        .cloned()
        .collect();

    let boy = BoyToday {
        user_id: CANONICAL_SCHOOL_USER_ID,
        name: CANONICAL_PROFILES[0].1.to_string(),
        due_today,
        catch_up,
        done,
        done_count: 1,
        skipped_count: 0,
        total_count: 22,
    };

    HomeschoolTodayView {
        date: TODAY.to_string(),
        is_school_day: true,
        anyone_enrolled: true,
        groups: vec![TogetherGroup {
            curriculum_id: 1,
            curriculum_name: "Sheffield Year One".to_string(),
            week: CANONICAL_SCHOOL_WEEK,
            weeks: CANONICAL_SCHOOL_WEEKS,
            term: 1,
            week_started_on: CANONICAL_SCHOOL_WEEK_STARTED_ON.to_string(),
            paused: false,
            year_complete: false,
            can_finish_week: false,
            days_on_week: 3,
            // Together is a phone concern (W-16): the kiosk never reads this
            // list, so leaving it empty proves the panel's shared-row filter
            // is doing the work rather than this field being.
            together: Vec::new(),
            boys: vec![boy],
            term_notes: Vec::new(),
        }],
    }
}

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
        homeschool: Some(canonical_school()),
        state: TvState::initial(),
        connected: true,
        stale: false,
        updated_at: Some("07:42".to_string()),
        join_url: Some("https://10.0.0.42:8443/m".to_string()),
        keys_debug: false,
        key_log: Vec::new(),
    }
}
