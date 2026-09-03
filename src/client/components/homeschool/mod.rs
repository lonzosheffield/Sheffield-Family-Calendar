//! The **School** tab — the phone's half of the homeschool surface
//! (`docs/homeschool/PLAN_HOMESCHOOL.md` §2 H6, task HS5).
//!
//! | file | what it is |
//! | --- | --- |
//! | `mod.rs` | the tab itself: fetching, dates, dispatch, the three panes |
//! | `today.rs` | **Today is the tab** — Together, per-boy blocks, This term |
//! | `row.rs` | one row: glyph → checkbox → subject → text, and its chips |
//! | `settings.rs` | the School settings sheet (enrollments, pause, subjects) |
//! | `enroll.rs` | the empty state and the enrollment form |
//! | `year.rs` | the Year view — week picker over a subject × Mon–Fri grid |
//! | `month.rs` | the Month view — one boy, a month of `done/total` |
//! | `day_sheet.rs` | one date: its items, its extras, and **Add task** |
//!
//! **Why the split is presentational-plus-shell.** Every pane below takes
//! plain props and an [`SchoolAction`] sink; this module is the only file that
//! knows a server exists. That is what makes the whole surface renderable by
//! `dioxus::ssr::render_element` in `tests/glyph_tests.rs` with no server, no
//! database and no browser — the same shape `RoutineRow` has had since D4.2,
//! and the reason HS5's acceptance is a set of assertions rather than a set of
//! screenshots.
//!
//! **Dates.** Every mutation carries the server's own `today` (±1 day) *and*
//! the occurrence's `scheduled_date`, which are different values on a catch-up
//! tick. The first comes from [`RoutineDateState`], reused verbatim from the
//! Routine tab so there is one date state machine on this phone, not two.

pub mod day_sheet;
pub mod enroll;
pub mod month;
pub mod row;
pub mod settings;
pub mod today;
pub mod year;

use dioxus::prelude::*;

use crate::client::components::glyphs;
use crate::client::components::homeschool::day_sheet::DaySheet;
use crate::client::components::homeschool::enroll::NoSchoolPlan;
use crate::client::components::homeschool::month::MonthPanel;
use crate::client::components::homeschool::settings::SchoolSettingsSheet;
use crate::client::components::homeschool::today::TodayPanel;
use crate::client::components::homeschool::year::YearPanel;
use crate::client::components::mobile::queue::{self, QueuedMutation};
use crate::client::components::routine::{new_idempotency_key, RoutineDateState};
use crate::client::realtime::use_realtime;
use crate::server::api::{
    add_extra, delete_extra, enroll as enroll_boy, get_enrollments, get_homeschool_today,
    get_month, get_subject_settings, get_week_grid, list_curricula, mark_all_done, set_paused,
    set_school_week, set_subject_schedule, today as today_date, toggle_extra, toggle_lesson,
    toggle_lesson_together, unenroll, update_extra, upsert_assignment,
};
use crate::shared::homeschool::{Category, LogStatus};
use crate::shared::types::{
    profile_name, CurriculumSummary, DayItem, EnrollmentView, HomeschoolTodayView, MonthView,
    SubjectSetting, WeekGrid,
};

/// Which of the three panes the tab is showing. **Today is the tab** (H6), so
/// it is the default and needs no segmented control to reach.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SchoolPane {
    #[default]
    Today,
    Year,
    Month,
}

impl SchoolPane {
    /// The two panes the header chip's neighbour toggles between, in order.
    pub const TOGGLE: [(SchoolPane, &'static str); 2] =
        [(SchoolPane::Year, "Year"), (SchoolPane::Month, "Month")];

    pub fn slug(self) -> &'static str {
        match self {
            SchoolPane::Today => "today",
            SchoolPane::Year => "year",
            SchoolPane::Month => "month",
        }
    }
}

/// Everything a School pane can ask the tab to do.
///
/// One sink rather than a dozen `EventHandler` props: the panes stay
/// presentational, the server calls stay in one place, and a test can drive a
/// pane with a handler that simply records what it was asked for.
#[derive(Clone, PartialEq, Debug)]
pub enum SchoolAction {
    /// Tick, untick or skip one boy's occurrence.
    ToggleLesson {
        user_id: i64,
        week: i64,
        subject_id: i64,
        assignment_id: Option<i64>,
        scheduled_date: String,
        completed: bool,
        status: LogStatus,
        /// H6 row action: a parent's note rides on the log row itself, so
        /// writing one is the same mutation as ticking.
        note: Option<String>,
    },
    /// Tick a **shared** occurrence for the whole Together group (parent only).
    ToggleTogether {
        curriculum_id: i64,
        week: i64,
        subject_id: i64,
        assignment_id: Option<i64>,
        scheduled_date: String,
        completed: bool,
    },
    ToggleExtra {
        user_id: i64,
        extra_id: i64,
        completed: bool,
        status: LogStatus,
    },
    MarkAllDone {
        user_id: i64,
        week: i64,
    },
    /// **Finish week** / **Back a week** for every boy in a group (parent).
    SetWeek {
        user_ids: Vec<i64>,
        week: i64,
    },
    /// H6 item 6 / D-5: inline edit of one week's `assignment.text` — and,
    /// from the Year cell sheet, of that row's own `days`.
    ///
    /// `detail` and `days` ride along because `upsert_assignment` writes the
    /// whole row (`text = excluded.text, detail = excluded.detail,
    /// days = excluded.days`): a variant that carried only `text` wrote `NULL`
    /// over the source's parenthetical second line on every save (QA round 3,
    /// QH3-02). Every caller passes the value it already has in hand.
    EditAssignment {
        subject_id: i64,
        week: i64,
        ordinal: i64,
        text: String,
        detail: Option<String>,
        /// The per-week override of `subjects.days` (QH3-04's amendment).
        /// `None` leaves the row inheriting the subject's days.
        days: Option<String>,
    },
    AddExtra {
        user_id: i64,
        scheduled_date: String,
        title: String,
        category: Category,
        text: Option<String>,
    },
    /// Re-title or re-file one parent-added task (H6 Month view: "extras can
    /// be edited, deleted, ticked or skipped from the same sheet"). The server
    /// side has always been there; QA round 2 (QH2-02) found the client half
    /// missing entirely.
    UpdateExtra {
        extra_id: i64,
        title: String,
        category: Category,
        text: Option<String>,
        scheduled_date: String,
    },
    DeleteExtra {
        extra_id: i64,
    },
    Enroll {
        user_id: i64,
        curriculum_id: i64,
        week: i64,
        school_days: String,
    },
    Unenroll {
        user_id: i64,
    },
    SetPaused {
        user_id: i64,
        paused: bool,
    },
    SetSubjectSchedule {
        subject_id: i64,
        days: String,
        shared: bool,
    },
    /// Open the School settings sheet (the header chip's own tap target).
    OpenSettings,
}

/// The months, spelled out — the phone has no date formatter and this module
/// may not reach for one: `chrono` is a server-only dependency.
const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// `2026-09-08` → `(2026, 9)`.
pub fn year_month_of(date: &str) -> Option<(i32, u32)> {
    let mut parts = date.split('-');
    let year = parts.next()?.parse::<i32>().ok()?;
    let month = parts.next()?.parse::<u32>().ok()?;
    (1..=12).contains(&month).then_some((year, month))
}

/// `(2026, 9)` → `September 2026`.
pub fn month_label(year: i32, month: u32) -> String {
    let name = MONTH_NAMES
        .get(month.saturating_sub(1) as usize)
        .copied()
        .unwrap_or("");
    format!("{name} {year}")
}

/// Step a `(year, month)` cursor by whole months, wrapping the year.
pub fn step_month(year: i32, month: u32, delta: i32) -> (i32, u32) {
    let zero_based = (month as i32 - 1) + delta;
    let year = year + zero_based.div_euclid(12);
    let month = zero_based.rem_euclid(12) as u32 + 1;
    (year, month)
}

/// The items one boy has on one date, as the day sheet renders them.
///
/// `due_today` holds every item dated that day — lessons and extras alike,
/// ticked or not (H3 rule 10, corrected by QH1-03) — so the sheet is exactly
/// that list. Nothing is pulled back out of `done`: a ticked extra never left
/// `due_today` in the first place.
pub fn day_items(view: &HomeschoolTodayView, user_id: i64) -> Vec<DayItem> {
    let mut out = Vec::new();
    for group in &view.groups {
        for boy in &group.boys {
            if boy.user_id != user_id {
                continue;
            }
            out.extend(boy.due_today.iter().cloned());
        }
    }
    out
}

/// The boy the Year and Month panes are drawn for.
///
/// Month view **always** shows exactly one boy (§4 default 17 / D-4): with
/// more than one enrolled, the chip is a required selector and the first
/// enrolled boy is preselected.
pub fn focused_boy(filter: Option<i64>, enrollments: &[EnrollmentView]) -> Option<i64> {
    filter
        .filter(|wanted| {
            enrollments
                .iter()
                .any(|row| row.enrolled && row.user_id == *wanted)
        })
        .or_else(|| {
            enrollments
                .iter()
                .find(|row| row.enrolled)
                .map(|row| row.user_id)
        })
}

/// The School tab.
#[component]
pub fn School() -> Element {
    let (bus, _sender) = use_realtime();

    let mut today_fetch =
        use_resource(move || async move { today_date().await.map_err(|err| err.to_string()) });
    let date_state =
        RoutineDateState::resolve((bus.today)(), (*today_fetch.read_unchecked()).clone());
    let mutation_date: Option<String> = date_state.date().map(str::to_string);

    let mut pane = use_signal(SchoolPane::default);
    let mut boy_filter = use_signal(|| Option::<i64>::None);
    let mut settings_open = use_signal(|| false);
    let mut selected_week = use_signal(|| Option::<i64>::None);
    let mut month_cursor = use_signal(|| Option::<(i32, u32)>::None);
    let mut open_day = use_signal(|| Option::<String>::None);

    let mut view_res = use_resource(move || async move {
        let _version = (bus.homeschool_version)();
        let date =
            RoutineDateState::resolve((bus.today)(), (*today_fetch.read_unchecked()).clone());
        match date {
            RoutineDateState::Ready(date) => get_homeschool_today(date).await.map(Some),
            _ => Ok(None),
        }
    });
    let mut enrollments_res = use_resource(move || async move {
        let _version = (bus.homeschool_version)();
        get_enrollments().await
    });
    let mut curricula_res = use_resource(move || async move {
        let _version = (bus.homeschool_version)();
        list_curricula().await
    });

    let view: Option<HomeschoolTodayView> = match &*view_res.read_unchecked() {
        Some(Ok(Some(view))) => Some(view.clone()),
        _ => None,
    };
    let curricula: Vec<CurriculumSummary> = match &*curricula_res.read_unchecked() {
        Some(Ok(rows)) => rows.clone(),
        _ => Vec::new(),
    };

    // **Everything a resource keys on is a [`Memo`], never a plain local.**
    // Dioxus 0.7's `use_resource` reruns only when a signal it read *inside*
    // its own closure changes. A value computed above the closure and captured
    // by copy is invisible to that machinery, so tapping a week in the Year
    // picker, stepping the month or choosing a boy chip re-rendered the tab
    // but left the fetched grid/month on whatever the first render asked for
    // until an unrelated `homeschool_version` bump happened to restart them
    // (QA round 1, QH1-02). Reading a memo inside the closure subscribes the
    // resource to it, which is what makes the panes refetch.
    let enrollments_memo = use_memo(move || match &*enrollments_res.read() {
        Some(Ok(rows)) => rows.clone(),
        _ => Vec::<EnrollmentView>::new(),
    });
    let focus = use_memo(move || focused_boy(boy_filter(), &enrollments_memo()));
    // Nobody enrolled at all. `focus()` is then `None`, so `grid_res` and
    // `month_res` resolve to `Ok(None)` and both panes used to fall through to
    // a `Loading today's school work…` card that could never finish — a
    // loading message that never ends reads as a hung app (QA round 3,
    // QH3-05). Today has always shown the way in; Year and Month now do too.
    let nobody = use_memo(move || enrollments_memo().iter().all(|row| !row.enrolled));
    let enrollment = use_memo(move || {
        focus().and_then(|id| {
            enrollments_memo()
                .into_iter()
                .find(|row| row.user_id == id && row.enrolled)
        })
    });
    let current_week = use_memo(move || enrollment().map(|row| row.current_week).unwrap_or(1));
    let grid_week = use_memo(move || selected_week().unwrap_or(current_week()));
    // The boy chips every pane offers: the enrolled boys, in enrollment order,
    // named the way every other surface names them (H6).
    let boys = use_memo(move || {
        enrollments_memo()
            .into_iter()
            .filter(|row| row.enrolled)
            .map(|row| {
                let name = u32::try_from(row.user_id)
                    .map(profile_name)
                    .unwrap_or("Family");
                (row.user_id, name.to_string())
            })
            .collect::<Vec<(i64, String)>>()
    });

    let mut subjects_res = use_resource(move || async move {
        let _version = (bus.homeschool_version)();
        match enrollment().map(|row| row.curriculum_id) {
            Some(curriculum_id) => get_subject_settings(curriculum_id).await,
            None => Ok(Vec::new()),
        }
    });
    let subjects: Vec<SubjectSetting> = match &*subjects_res.read_unchecked() {
        Some(Ok(rows)) => rows.clone(),
        _ => Vec::new(),
    };

    let mut grid_res = use_resource(move || async move {
        let _version = (bus.homeschool_version)();
        let week = grid_week();
        match focus() {
            Some(user_id) => get_week_grid(user_id, week).await.map(Some),
            None => Ok(None),
        }
    });
    let grid: Option<WeekGrid> = match &*grid_res.read_unchecked() {
        Some(Ok(Some(grid))) => Some(grid.clone()),
        _ => None,
    };

    let cursor = use_memo(move || {
        month_cursor()
            .or_else(|| {
                RoutineDateState::resolve((bus.today)(), (*today_fetch.read()).clone())
                    .date()
                    .and_then(year_month_of)
            })
            .unwrap_or((2026, 1))
    });
    let mut month_res = use_resource(move || async move {
        let _version = (bus.homeschool_version)();
        let (year, month) = cursor();
        match focus() {
            Some(user_id) => get_month(user_id, year, month).await.map(Some),
            None => Ok(None),
        }
    });
    let month: Option<MonthView> = match &*month_res.read_unchecked() {
        Some(Ok(Some(month))) => Some(month.clone()),
        _ => None,
    };

    let sheet_date = open_day();
    let mut day_res = use_resource(move || async move {
        let _version = (bus.homeschool_version)();
        match open_day() {
            Some(date) => get_homeschool_today(date).await.map(Some),
            None => Ok(None),
        }
    });
    let day_view: Option<HomeschoolTodayView> = match &*day_res.read_unchecked() {
        Some(Ok(Some(view))) => Some(view.clone()),
        _ => None,
    };

    // The one thing on this tab that can fail with nothing to show for it.
    // A Together tick is deliberately **not** queued offline (H6: the fan-out
    // needs the group membership only the server holds), so an expired parent
    // cookie, an unreachable hub or a boy moved to another week has to say so
    // out loud — `docs/PWA.md` promises the parent a message, and QA round 2
    // (QH2-03) found the failure being dropped on the floor instead.
    let mut notice = use_signal(|| Option::<String>::None);

    // One dispatcher for every pane. Each arm is the same shape the Routine
    // tab's toggles already use: call the server function, queue it with its
    // own date and a key when the call fails, then refetch.
    // `use_callback` rather than a plain closure: a `Callback` is `Copy`, so
    // the one dispatcher can be handed to all four panes and both sheets
    // without cloning the whole capture set per pane.
    let dispatch = use_callback(move |action: SchoolAction| {
        let date = mutation_date.clone();
        spawn(async move {
            let Some(date) = date else {
                return;
            };
            match action {
                SchoolAction::OpenSettings => {
                    settings_open.set(true);
                    return;
                }
                SchoolAction::ToggleLesson {
                    user_id,
                    week,
                    subject_id,
                    assignment_id,
                    scheduled_date,
                    completed,
                    status,
                    note,
                } => {
                    let failed = toggle_lesson(
                        user_id,
                        subject_id,
                        assignment_id,
                        week,
                        scheduled_date.clone(),
                        completed,
                        status,
                        note.clone(),
                        date.clone(),
                        new_idempotency_key(),
                    )
                    .await
                    .is_err();
                    // A note is never queued: the queued replay carries no
                    // note field, and silently dropping the words a parent
                    // typed would be worse than asking them to try again.
                    if failed && status == LogStatus::Done && note.is_none() {
                        queue::record_offline_failure(
                            QueuedMutation::ToggleLesson {
                                user_id: clamp_user(user_id),
                                subject_id,
                                assignment_id,
                                week,
                                scheduled_date,
                                completed,
                            },
                            date,
                        );
                    }
                }
                SchoolAction::ToggleTogether {
                    curriculum_id,
                    week,
                    subject_id,
                    assignment_id,
                    scheduled_date,
                    completed,
                } => {
                    // Never queued (H6): the fan-out needs a group membership
                    // only the server holds. So the failure is shown, not
                    // swallowed (QH2-03).
                    if let Err(err) = toggle_lesson_together(
                        curriculum_id,
                        week,
                        subject_id,
                        assignment_id,
                        scheduled_date,
                        completed,
                        date,
                        new_idempotency_key(),
                        String::new(),
                    )
                    .await
                    {
                        notice.set(Some(format!(
                            "Couldn't tick that for everyone — {err}. Sign in as a parent and try again."
                        )));
                    }
                }
                SchoolAction::ToggleExtra {
                    user_id,
                    extra_id,
                    completed,
                    status,
                } => {
                    let failed = toggle_extra(
                        extra_id,
                        completed,
                        status,
                        None,
                        date.clone(),
                        new_idempotency_key(),
                    )
                    .await
                    .is_err();
                    if failed && status == LogStatus::Done {
                        queue::record_offline_failure(
                            QueuedMutation::ToggleExtra {
                                user_id: clamp_user(user_id),
                                extra_id,
                                completed,
                            },
                            date,
                        );
                    }
                }
                SchoolAction::MarkAllDone { user_id, week } => {
                    let _ = mark_all_done(user_id, week, date, new_idempotency_key()).await;
                }
                SchoolAction::SetWeek { user_ids, week } => {
                    for user_id in user_ids {
                        let _ = set_school_week(user_id, week, date.clone(), String::new()).await;
                    }
                    selected_week.set(None);
                }
                SchoolAction::EditAssignment {
                    subject_id,
                    week,
                    ordinal,
                    text,
                    detail,
                    days,
                } => {
                    // Both riders are passed through, never defaulted: the
                    // storage fn replaces the whole row, so dropping `detail`
                    // erased the source's second line (QH3-02) and dropping
                    // `days` would un-pin the Year sheet's per-week override.
                    let _ = upsert_assignment(
                        subject_id,
                        week,
                        ordinal,
                        text,
                        detail,
                        days,
                        String::new(),
                    )
                    .await;
                }
                SchoolAction::AddExtra {
                    user_id,
                    scheduled_date,
                    title,
                    category,
                    text,
                } => {
                    let _ = add_extra(
                        user_id,
                        scheduled_date,
                        title,
                        category,
                        text,
                        date,
                        new_idempotency_key(),
                        String::new(),
                    )
                    .await;
                }
                SchoolAction::UpdateExtra {
                    extra_id,
                    title,
                    category,
                    text,
                    scheduled_date,
                } => {
                    let _ = update_extra(
                        extra_id,
                        title,
                        category,
                        text,
                        scheduled_date,
                        String::new(),
                    )
                    .await;
                }
                SchoolAction::DeleteExtra { extra_id } => {
                    let _ = delete_extra(extra_id, String::new()).await;
                }
                SchoolAction::Enroll {
                    user_id,
                    curriculum_id,
                    week,
                    school_days,
                } => {
                    let _ = enroll_boy(
                        user_id,
                        curriculum_id,
                        week,
                        school_days,
                        date,
                        String::new(),
                    )
                    .await;
                }
                SchoolAction::Unenroll { user_id } => {
                    let _ = unenroll(user_id, String::new()).await;
                }
                SchoolAction::SetPaused { user_id, paused } => {
                    let _ = set_paused(user_id, paused, String::new()).await;
                }
                SchoolAction::SetSubjectSchedule {
                    subject_id,
                    days,
                    shared,
                } => {
                    let _ = set_subject_schedule(subject_id, days, shared, String::new()).await;
                }
            }
            view_res.restart();
            enrollments_res.restart();
            curricula_res.restart();
            subjects_res.restart();
            grid_res.restart();
            month_res.restart();
            day_res.restart();
        });
    });

    if date_state == RoutineDateState::Error {
        return rsx! {
            div { class: "rounded-2xl bg-red-50 p-4 text-red-700 ring-1 ring-red-200",
                p { class: "font-bold", "Can't reach the hub" }
                p { class: "text-sm",
                    "Today's school work can't be shown right now. Check the connection and try again."
                }
                button {
                    class: "mt-3 rounded-xl bg-red-600 px-4 py-2 font-semibold text-white",
                    onclick: move |_| {
                        today_fetch.restart();
                    },
                    "Retry"
                }
            }
        };
    }

    rsx! {
        div { class: "flex flex-col gap-4", "data-school-pane": "{pane().slug()}",
            div { class: "flex items-center justify-between gap-2",
                h2 { class: "text-lg font-bold text-sheffield-dark",
                    span { class: "mr-1", aria_hidden: "true", "{glyphs::HOMESCHOOL_GLYPH}" }
                    "School"
                }
                div { class: "flex gap-1", role: "group", aria_label: "How to look at school",
                    for (candidate , label) in SchoolPane::TOGGLE {
                        button {
                            key: "{label}",
                            class: if pane() == candidate { "rounded-full bg-sheffield-dark px-3 py-2 text-sm font-bold text-white" } else { "rounded-full bg-white px-3 py-2 text-sm font-semibold text-sheffield-dark ring-1 ring-slate-200" },
                            aria_pressed: if pane() == candidate { "true" } else { "false" },
                            onclick: move |_| {
                                let next = if pane() == candidate { SchoolPane::Today } else { candidate };
                                pane.set(next);
                            },
                            "{label}"
                        }
                    }
                }
            }

            if let Some(message) = notice() {
                div {
                    class: "flex items-center justify-between gap-3 rounded-2xl bg-sheffield-sun px-4 py-3 text-sm font-semibold text-slate-800",
                    role: "alert",
                    "data-school-notice": "true",
                    span { "{message}" }
                    button {
                        class: "rounded-xl bg-white px-3 py-1 text-sm font-bold text-sheffield-dark",
                        onclick: move |_| notice.set(None),
                        "OK"
                    }
                }
            }

            match pane() {
                SchoolPane::Today => rsx! {
                    if let Some(view) = view.clone() {
                        TodayPanel {
                            view,
                            boy_filter: boy_filter(),
                            on_boy_filter: move |next| boy_filter.set(next),
                            on_action: dispatch,
                        }
                    } else {
                        LoadingCard {}
                    }
                },
                SchoolPane::Year => rsx! {
                    if let (Some(grid), Some(row), Some(user_id)) = (
                        grid.clone(),
                        enrollment(),
                        focus(),
                    )
                    {
                        YearPanel {
                            grid,
                            user_id,
                            current_week: current_week(),
                            anchor: row.week_started_on.clone(),
                            boys: boys(),
                            on_boy_filter: move |next| boy_filter.set(next),
                            on_select_week: move |week| selected_week.set(Some(week)),
                            on_action: dispatch,
                        }
                    } else {
                        if nobody() {
                            NoSchoolPlan { on_enroll: move |()| settings_open.set(true) }
                        } else {
                            LoadingCard {}
                        }
                    }
                },
                SchoolPane::Month => rsx! {
                    if let Some(month) = month.clone() {
                        MonthPanel {
                            month,
                            label: month_label(cursor().0, cursor().1),
                            boys: boys(),
                            on_boy_filter: move |next| boy_filter.set(next),
                            on_open_day: move |date| open_day.set(Some(date)),
                            on_step: move |delta: i32| {
                                let (year, month) = cursor();
                                month_cursor.set(Some(step_month(year, month, delta)));
                            },
                        }
                    } else {
                        if nobody() {
                            NoSchoolPlan { on_enroll: move |()| settings_open.set(true) }
                        } else {
                            LoadingCard {}
                        }
                    }
                },
            }

            if settings_open() {
                SchoolSettingsSheet {
                    enrollments: enrollments_memo(),
                    curricula: curricula.clone(),
                    subjects: subjects.clone(),
                    on_action: dispatch,
                    on_close: move |()| settings_open.set(false),
                }
            }

            if let (Some(date), Some(user_id)) = (sheet_date.clone(), focus()) {
                DaySheet {
                    date: date.clone(),
                    week: current_week(),
                    in_current_week: month
                        .as_ref()
                        .and_then(|month| {
                            month.days.iter().find(|day| day.date == date)
                        })
                        .is_some_and(|day| day.in_current_week),
                    // QH1-07: a date *before* the span is not "not dealt out
                    // yet" — it has already been and gone.
                    before_span: enrollment()
                        .is_some_and(|row| date.as_str() < row.week_started_on.as_str()),
                    user_id,
                    items: day_view
                        .as_ref()
                        .map(|view| day_items(view, user_id))
                        .unwrap_or_default(),
                    on_action: dispatch,
                    on_close: move |()| open_day.set(None),
                }
            }
        }
    }
}

/// The offline queue keys its entries on the rail's 1-based `u32` profile id,
/// which is what every other queued mutation already carries. A wider id
/// cannot be queued — it is dropped back to 1 rather than silently truncated
/// — and the server re-validates ownership on replay regardless.
fn clamp_user(user_id: i64) -> u32 {
    u32::try_from(user_id).unwrap_or(1)
}

#[component]
fn LoadingCard() -> Element {
    rsx! {
        p { class: "rounded-2xl bg-white p-4 text-sm text-slate-600 ring-1 ring-slate-100",
            "Loading today's school work…"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enrollment(user_id: i64, enrolled: bool) -> EnrollmentView {
        EnrollmentView {
            user_id,
            enrolled,
            curriculum_id: 1,
            curriculum_name: "Sample Year".into(),
            current_week: 2,
            weeks: 3,
            week_started_on: "2026-09-07".into(),
            school_days: "MTWRF".into(),
            paused: false,
        }
    }

    #[test]
    fn a_month_cursor_steps_across_a_year_boundary_in_both_directions() {
        assert_eq!(step_month(2026, 12, 1), (2027, 1));
        assert_eq!(step_month(2026, 1, -1), (2025, 12));
        assert_eq!(step_month(2026, 9, 1), (2026, 10));
        assert_eq!(step_month(2026, 9, -13), (2025, 8));
    }

    #[test]
    fn a_date_yields_its_year_and_month_and_rejects_nonsense() {
        assert_eq!(year_month_of("2026-09-08"), Some((2026, 9)));
        assert_eq!(year_month_of("2026-13-01"), None);
        assert_eq!(year_month_of("2026-00-01"), None);
        assert_eq!(year_month_of("not-a-date"), None);
        assert_eq!(year_month_of(""), None);
    }

    #[test]
    fn a_month_label_names_the_month_in_words() {
        assert_eq!(month_label(2026, 9), "September 2026");
        assert_eq!(month_label(2026, 1), "January 2026");
        assert_eq!(month_label(2026, 12), "December 2026");
    }

    #[test]
    fn the_month_view_always_falls_back_to_the_first_enrolled_boy() {
        // §4 default 17 / D-4: Month shows exactly one boy, and the chip is a
        // required selector rather than an optional filter.
        let rows = vec![
            enrollment(1, false),
            enrollment(2, true),
            enrollment(3, true),
        ];
        assert_eq!(focused_boy(None, &rows), Some(2));
        assert_eq!(focused_boy(Some(3), &rows), Some(3));
        // A filter naming a boy who is not enrolled must not blank the pane.
        assert_eq!(focused_boy(Some(1), &rows), Some(2));
        assert_eq!(focused_boy(Some(9), &rows), Some(2));
    }

    #[test]
    fn with_nobody_enrolled_there_is_no_boy_to_focus() {
        assert_eq!(focused_boy(None, &[]), None);
        assert_eq!(focused_boy(Some(1), &[enrollment(1, false)]), None);
    }

    #[test]
    fn a_queued_tick_never_carries_a_profile_id_the_queue_cannot_hold() {
        assert_eq!(clamp_user(3), 3);
        assert_eq!(clamp_user(-1), 1);
        assert_eq!(clamp_user(i64::MAX), 1);
    }

    #[test]
    fn today_is_the_tab_and_the_toggle_offers_only_the_other_two() {
        assert_eq!(SchoolPane::default(), SchoolPane::Today);
        let labels: Vec<&str> = SchoolPane::TOGGLE.iter().map(|(_, label)| *label).collect();
        assert_eq!(labels, vec!["Year", "Month"]);
        assert_eq!(SchoolPane::Today.slug(), "today");
    }
}
