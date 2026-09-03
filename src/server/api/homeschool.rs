//! School ("house") tab server functions — **HS4**,
//! `docs/homeschool/PLAN_HOMESCHOOL.md` §3 row HS4.
//!
//! HS1 (`crate::server::homeschool::db`, aliased `hs` below) owns every query
//! over `migrations/0005_homeschool.sql`; HS3 (`crate::shared::homeschool`,
//! aliased `sched` below, plus the DTOs in `crate::shared::types`) owns the
//! pure scheduling core and the wire types. This file's only job is the glue:
//! read the storage rows, convert them into HS3's DTOs, run HS3's pure
//! functions, and — for every mutation — enforce H7's authorization and
//! occurrence-validity rules before writing anything back.
//!
//! Every mutating fn follows the same shape as `api::routine`
//! (`docs/homeschool/PLAN_HOMESCHOOL.md` §3 HS4 "Do"):
//! **date window → begin → claim_mutation → validate against the recomputed
//! occurrences → write → commit → publish.** The settings-style mutations
//! (`enroll`, `unenroll`, `set_paused`, `upsert_assignment`,
//! `set_subject_schedule`, `update_extra`, `delete_extra`) carry neither a
//! `date` nor an `idempotency_key` in their normative signature, so for those
//! the pattern collapses to **validate → write → publish** — there is no
//! ±1 day window to check and no replay to dedupe, matching
//! `api::profiles`'s profile CRUD, which follows the identical shape for the
//! identical reason.
//!
//! **Authorization (H7):** every `auth: String` parameter is checked via
//! [`crate::server::api::profiles::require_session_or_cookie`] (made
//! `pub(crate)` by this task — visibility only, its body is T1.4's). A
//! function with no `auth` parameter is open to anyone on the LAN, exactly
//! mirroring `toggle_routine_task`.
//!
//! **Realtime:** every function that changes one boy's log, extras or
//! enrollment publishes `ServerMessage::HomeschoolUpdated { user_ids, week,
//! date }`; every function that edits the curriculum itself (`upsert_assignment`,
//! `set_subject_schedule`) publishes `ServerMessage::CurriculumUpdated { curriculum_id }`
//! instead, since that affects every boy enrolled on it, not one. For an
//! extras mutation, `week` carries the sentinel `0` — extras are not scoped to
//! a curriculum week (H8) — and `date` carries the extra's own
//! `scheduled_date`, the day whose view actually changed.
//!
//! `dioxus_fullstack_macro::server`'s expansion (0.7.10) does not forward a
//! function's own attributes onto the public wrapper it generates (only doc
//! comments survive), so a per-function `#[allow(clippy::too_many_arguments)]`
//! on e.g. `toggle_lesson` has no effect on the lint clippy raises against
//! that generated wrapper — hence the module-wide allow below, scoped to this
//! file only, for the handful of functions whose normative signature
//! (`docs/homeschool/PLAN_HOMESCHOOL.md` §3 HS4 "Do") genuinely needs more
//! than seven parameters.
#![allow(clippy::too_many_arguments)]

use dioxus::prelude::*;

use crate::shared::homeschool::{Category, LogStatus};
use crate::shared::types::{
    CurriculumSummary, EnrollmentView, ExtraTask, HomeschoolTodayView, MonthView, SubjectSetting,
    WeekGrid,
};

#[cfg(feature = "server")]
use crate::server::homeschool::db as hs;
#[cfg(feature = "server")]
use crate::shared::homeschool as sched;
#[cfg(feature = "server")]
use crate::shared::types::{DayItem, ServerMessage, TogetherGroup};
#[cfg(feature = "server")]
use std::collections::{BTreeMap, HashMap};

/// Build a `ServerFnError` for a validation failure this module owns (never a
/// raw `sqlx::Error`, which [`super::to_server_error`] handles instead).
#[cfg(feature = "server")]
fn validation_error(message: &str) -> ServerFnError {
    ServerFnError::new(message.to_string())
}

/// QH1-10: `toggle_lesson` and `toggle_extra` are LAN-open by contract
/// (H7) — no session cookie bounds who can write a `note` — so this caps
/// it the way the rest of the open surface caps its inputs.
#[cfg(feature = "server")]
const MAX_NOTE_CHARS: usize = 500;

/// Today's date in `YYYY-MM-DD`, as seen by the server — the same clock every
/// other mutation in this crate checks a caller's `date` against.
#[cfg(feature = "server")]
fn today_string() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// Reject `date` unless it falls within the server's ±1 day window (R-24), the
/// same check every other mutating server function in this crate makes.
#[cfg(feature = "server")]
fn check_date_window(date: &str) -> Result<(), ServerFnError> {
    let today = today_string();
    if crate::server::db::date_within_window(date, &today) {
        Ok(())
    } else {
        Err(validation_error(&format!(
            "date {date} is outside the ±1 day window around {today}"
        )))
    }
}

// ---------------------------------------------------------------------------
// Row → DTO conversions (HS1's storage rows → HS3's shared types)
// ---------------------------------------------------------------------------

#[cfg(feature = "server")]
fn to_enrollment(row: &hs::EnrollmentRow) -> Result<sched::Enrollment, ServerFnError> {
    let school_days =
        sched::parse_days(&row.school_days).map_err(|e| validation_error(&e.to_string()))?;
    Ok(sched::Enrollment {
        profile_id: row.profile_id,
        curriculum_id: row.curriculum_id,
        current_week: row.current_week,
        weeks: row.weeks,
        term_weeks: row.term_weeks,
        week_started_on: row.week_started_on.clone(),
        school_days,
        paused: row.paused,
    })
}

#[cfg(feature = "server")]
fn to_plan(rows: &hs::WeekPlanRows, week: i64) -> Result<sched::WeekPlan, ServerFnError> {
    let mut subjects = Vec::with_capacity(rows.subjects.len());
    for subject in &rows.subjects {
        let category = Category::parse(&subject.category).ok_or_else(|| {
            validation_error(&format!(
                "unknown category {:?} in database",
                subject.category
            ))
        })?;
        let days =
            sched::parse_days(&subject.days).map_err(|e| validation_error(&e.to_string()))?;

        let mut rows_for_subject = Vec::new();
        for assignment in rows
            .assignments
            .iter()
            .filter(|a| a.subject_id == subject.id)
        {
            let pinned_days = match &assignment.days {
                Some(letters) => {
                    Some(sched::parse_days(letters).map_err(|e| validation_error(&e.to_string()))?)
                }
                None => None,
            };
            rows_for_subject.push(sched::AssignmentRow {
                assignment_id: assignment.id,
                ordinal: assignment.ordinal,
                text: assignment.text.clone(),
                detail: assignment.detail.clone(),
                days: pinned_days,
            });
        }

        subjects.push(sched::SubjectPlan {
            subject_id: subject.id,
            name: subject.name.clone(),
            category,
            source: subject.source.clone(),
            icon_name: subject.icon_name.clone(),
            sort_order: subject.sort_order,
            days,
            shared: subject.shared,
            rows: rows_for_subject,
        });
    }

    let mut term_notes = Vec::with_capacity(rows.term_notes.len());
    for note in &rows.term_notes {
        let kind = sched::TermNoteKind::parse(&note.kind).ok_or_else(|| {
            validation_error(&format!(
                "unknown term note kind {:?} in database",
                note.kind
            ))
        })?;
        term_notes.push(sched::TermNote {
            id: note.id,
            term: note.term,
            kind,
            text: note.text.clone(),
            sort_order: note.sort_order,
        });
    }

    Ok(sched::WeekPlan {
        curriculum_id: rows.curriculum.id,
        week,
        weeks: rows.curriculum.weeks,
        term: rows.term,
        subjects,
        term_notes,
    })
}

#[cfg(feature = "server")]
fn to_log_rows(rows: &[hs::LessonLogRow]) -> Result<Vec<sched::LogRow>, ServerFnError> {
    rows.iter()
        .map(|row| {
            let status = LogStatus::parse(&row.status).ok_or_else(|| {
                validation_error(&format!("unknown log status {:?} in database", row.status))
            })?;
            Ok(sched::LogRow {
                subject_id: row.subject_id,
                assignment_id: row.assignment_id,
                scheduled_date: row.scheduled_date.clone(),
                status,
                note: row.note.clone(),
            })
        })
        .collect()
}

#[cfg(feature = "server")]
fn to_extra_task(row: &hs::ExtraRow) -> Result<ExtraTask, ServerFnError> {
    let category = Category::parse(&row.category).ok_or_else(|| {
        validation_error(&format!("unknown category {:?} in database", row.category))
    })?;
    let status = match &row.status {
        Some(s) => Some(
            LogStatus::parse(s)
                .ok_or_else(|| validation_error(&format!("unknown status {s:?} in database")))?,
        ),
        None => None,
    };
    Ok(ExtraTask {
        id: row.id,
        user_id: row.profile_id,
        scheduled_date: row.scheduled_date.clone(),
        title: row.title.clone(),
        category,
        text: row.text.clone(),
        sort_order: row.sort_order,
        status,
        note: row.note.clone(),
    })
}

/// `enrolled = false` leaves every curriculum field at its zero value — never
/// an error (H6's empty state, HS4 accept (j)).
#[cfg(feature = "server")]
fn enrollment_view(row: Option<&hs::EnrollmentRow>, user_id: i64) -> EnrollmentView {
    match row {
        Some(r) => EnrollmentView {
            user_id,
            enrolled: true,
            curriculum_id: r.curriculum_id,
            curriculum_name: r.curriculum_name.clone(),
            current_week: r.current_week,
            weeks: r.weeks,
            week_started_on: r.week_started_on.clone(),
            school_days: r.school_days.clone(),
            paused: r.paused,
        },
        None => EnrollmentView {
            user_id,
            enrolled: false,
            curriculum_id: 0,
            curriculum_name: String::new(),
            current_week: 0,
            weeks: 0,
            week_started_on: String::new(),
            school_days: String::new(),
            paused: false,
        },
    }
}

/// Per-date `done + skipped` counts, expanded into placeholder [`sched::LogRow`]s
/// so [`sched::month_view`] — which only counts rows matching a date, never
/// inspects their subject or status — can be fed from `hs::log_counts_between`
/// without HS4 duplicating a per-occurrence query HS1 does not expose.
#[cfg(feature = "server")]
fn synth_logs(counts: &[hs::DayLogCount]) -> Vec<sched::LogRow> {
    let mut out = Vec::new();
    for count in counts {
        for _ in 0..(count.done + count.skipped) {
            out.push(sched::LogRow {
                subject_id: 0,
                assignment_id: None,
                scheduled_date: count.scheduled_date.clone(),
                status: LogStatus::Done,
                note: None,
            });
        }
    }
    out
}

/// The `(curriculum_id, weeks)` a subject belongs to — the lookup
/// `upsert_assignment` and `set_subject_schedule` both need before they can
/// validate a week bound or publish `CurriculumUpdated`.
#[cfg(feature = "server")]
async fn subject_curriculum(
    pool: &sqlx::SqlitePool,
    subject_id: i64,
) -> Result<(i64, i64), ServerFnError> {
    let row: Option<(i64, i64)> = sqlx::query_as(
        "SELECT s.curriculum_id, c.weeks FROM subjects s \
         JOIN curricula c ON c.id = s.curriculum_id WHERE s.id = ?1",
    )
    .bind(subject_id)
    .fetch_optional(pool)
    .await
    .map_err(super::to_server_error)?;
    row.ok_or_else(|| validation_error("unknown subject"))
}

/// H7: the tick/skip target must be one of this week's recomputed occurrences.
#[cfg(feature = "server")]
fn find_occurrence<'a>(
    occurrences: &'a [crate::shared::types::LessonOccurrence],
    subject_id: i64,
    assignment_id: Option<i64>,
    scheduled_date: &str,
) -> Option<&'a crate::shared::types::LessonOccurrence> {
    occurrences.iter().find(|occurrence| {
        occurrence.subject_id == subject_id
            && occurrence.assignment_id == assignment_id
            && occurrence.scheduled_date == scheduled_date
    })
}

// ---------------------------------------------------------------------------
// Reads — unauthenticated (H7: "read Today / settings | anyone on the LAN")
// ---------------------------------------------------------------------------

/// Every group, every boy, for one date. `anyone_enrolled = false` is the
/// empty state ("No school plan yet"), never an error.
#[server(endpoint = "get_homeschool_today")]
pub async fn get_homeschool_today(date: String) -> Result<HomeschoolTodayView, ServerFnError> {
    #[cfg(feature = "server")]
    {
        let pool = crate::server::db::read_pool()
            .await
            .map_err(super::to_server_error)?;
        let enrollments = hs::all_enrollments(pool)
            .await
            .map_err(super::to_server_error)?;
        if enrollments.is_empty() {
            return Ok(HomeschoolTodayView {
                date,
                is_school_day: false,
                anyone_enrolled: false,
                groups: Vec::new(),
            });
        }

        let name_rows: Vec<(i64, String)> = sqlx::query_as("SELECT id, name FROM profiles")
            .fetch_all(pool)
            .await
            .map_err(super::to_server_error)?;
        let names: HashMap<i64, String> = name_rows.into_iter().collect();

        // H4: the Together group is every enrollment sharing (curriculum_id,
        // current_week). Grouping by a `BTreeMap` key gives a deterministic
        // (curriculum, week) order across calls, which nothing depends on but
        // which makes this function's output reproducible in a test.
        let mut grouped: BTreeMap<(i64, i64), Vec<hs::EnrollmentRow>> = BTreeMap::new();
        for row in enrollments {
            grouped
                .entry((row.curriculum_id, row.current_week))
                .or_default()
                .push(row);
        }

        let mut groups = Vec::new();
        let mut is_school_day = false;
        for ((curriculum_id, week), members) in grouped {
            let Some(plan_rows) = hs::week_plan(pool, curriculum_id, week)
                .await
                .map_err(super::to_server_error)?
            else {
                continue;
            };
            let plan = to_plan(&plan_rows, week)?;

            let mut boys_with_logs: Vec<(sched::Enrollment, Vec<sched::LogRow>)> = Vec::new();
            for member in &members {
                let log_rows = hs::logs(pool, member.profile_id, week)
                    .await
                    .map_err(super::to_server_error)?;
                boys_with_logs.push((to_enrollment(member)?, to_log_rows(&log_rows)?));
            }

            let together = sched::together_view(&boys_with_logs, &plan, &date);

            let mut boys = Vec::new();
            let mut all_can_finish = true;
            for (enrollment, logs) in &boys_with_logs {
                if sched::is_school_day(enrollment, &date) {
                    is_school_day = true;
                }
                let mut today = sched::today_view(&plan, enrollment, logs, &date);
                today.name = names
                    .get(&enrollment.profile_id)
                    .cloned()
                    .unwrap_or_default();

                if let Some((span_from, span_to)) = sched::week_span(&enrollment.week_started_on) {
                    let earliest = sched::add_days(&date, -sched::EXTRA_CATCH_UP_DAYS)
                        .unwrap_or_else(|| span_from.clone());
                    let window_from = if earliest.as_str() < span_from.as_str() {
                        earliest
                    } else {
                        span_from.clone()
                    };
                    let window_to = if date.as_str() > span_to.as_str() {
                        date.clone()
                    } else {
                        span_to.clone()
                    };
                    let extra_rows =
                        hs::extras_between(pool, enrollment.profile_id, &window_from, &window_to)
                            .await
                            .map_err(super::to_server_error)?;
                    let extras = extra_rows
                        .iter()
                        .map(to_extra_task)
                        .collect::<Result<Vec<_>, _>>()?;
                    sched::merge_extras(&mut today, &extras, &date, (&span_from, &span_to));
                }

                if !sched::can_finish_week(&plan, enrollment, logs, &date) {
                    all_can_finish = false;
                }
                boys.push(today);
            }

            let representative = &boys_with_logs[0].0;
            groups.push(TogetherGroup {
                curriculum_id,
                curriculum_name: plan_rows.curriculum.name.clone(),
                week,
                weeks: plan.weeks,
                term: plan.term,
                week_started_on: representative.week_started_on.clone(),
                // The group as a whole reads "School's out" only once every
                // member is paused; a single paused boy still has an empty
                // personal block, but his brothers' Together items stay live.
                paused: boys_with_logs
                    .iter()
                    .all(|(enrollment, _)| enrollment.paused),
                year_complete: representative.year_complete(),
                can_finish_week: all_can_finish,
                days_on_week: sched::days_on_week(&representative.week_started_on, &date),
                together,
                boys,
                term_notes: plan.term_notes.clone(),
            });
        }

        Ok(HomeschoolTodayView {
            date,
            is_school_day,
            anyone_enrolled: true,
            groups,
        })
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = date;
        unreachable!("server function bodies only run on the server")
    }
}

/// Every profile's enrollment (or the `enrolled = false` zero row), in
/// display order — School settings' enrollment list.
#[server(endpoint = "get_enrollments")]
pub async fn get_enrollments() -> Result<Vec<EnrollmentView>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        let pool = crate::server::db::read_pool()
            .await
            .map_err(super::to_server_error)?;
        let ids: Vec<(i64,)> = sqlx::query_as("SELECT id FROM profiles ORDER BY sort_order, id")
            .fetch_all(pool)
            .await
            .map_err(super::to_server_error)?;
        let mut out = Vec::with_capacity(ids.len());
        for (id,) in ids {
            let row = hs::enrollment(pool, id)
                .await
                .map_err(super::to_server_error)?;
            out.push(enrollment_view(row.as_ref(), id));
        }
        Ok(out)
    }
    #[cfg(not(feature = "server"))]
    {
        unreachable!("server function bodies only run on the server")
    }
}

/// Every curriculum on the box, with its subject count, for the picker.
#[server(endpoint = "list_curricula")]
pub async fn list_curricula() -> Result<Vec<CurriculumSummary>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        let pool = crate::server::db::read_pool()
            .await
            .map_err(super::to_server_error)?;
        let rows: Vec<(i64, String, String, i64, i64, i64)> = sqlx::query_as(
            "SELECT c.id, c.slug, c.name, c.weeks, c.term_weeks, COUNT(s.id) \
             FROM curricula c LEFT JOIN subjects s ON s.curriculum_id = c.id \
             GROUP BY c.id ORDER BY c.id",
        )
        .fetch_all(pool)
        .await
        .map_err(super::to_server_error)?;
        Ok(rows
            .into_iter()
            .map(
                |(id, slug, name, weeks, term_weeks, subject_count)| CurriculumSummary {
                    id,
                    slug,
                    name,
                    weeks,
                    term_weeks,
                    subject_count,
                },
            )
            .collect())
    }
    #[cfg(not(feature = "server"))]
    {
        unreachable!("server function bodies only run on the server")
    }
}

/// One curriculum's subjects, as School settings' per-subject controls edit
/// them. An unknown `curriculum_id` is an empty list, not an error.
#[server(endpoint = "get_subject_settings")]
pub async fn get_subject_settings(
    curriculum_id: i64,
) -> Result<Vec<SubjectSetting>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        let pool = crate::server::db::read_pool()
            .await
            .map_err(super::to_server_error)?;
        // Subjects are not filtered by week (HS1's `week_plan` doc comment),
        // so week 1 is as good a week as any to fetch them through.
        let Some(plan) = hs::week_plan(pool, curriculum_id, 1)
            .await
            .map_err(super::to_server_error)?
        else {
            return Ok(Vec::new());
        };
        plan.subjects
            .iter()
            .map(|subject| {
                let category = Category::parse(&subject.category).ok_or_else(|| {
                    validation_error(&format!(
                        "unknown category {:?} in database",
                        subject.category
                    ))
                })?;
                Ok(SubjectSetting {
                    subject_id: subject.id,
                    name: subject.name.clone(),
                    category,
                    days: subject.days.clone(),
                    shared: subject.shared,
                })
            })
            .collect()
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = curriculum_id;
        unreachable!("server function bodies only run on the server")
    }
}

/// The Year view's subject × school-day grid for one week of one boy's
/// curriculum. `dated = false` when `week` is not his current week (§4
/// default 17).
#[server(endpoint = "get_week_grid")]
pub async fn get_week_grid(user_id: i64, week: i64) -> Result<WeekGrid, ServerFnError> {
    #[cfg(feature = "server")]
    {
        let pool = crate::server::db::read_pool()
            .await
            .map_err(super::to_server_error)?;
        let enrollment_row = hs::enrollment(pool, user_id)
            .await
            .map_err(super::to_server_error)?
            .ok_or_else(|| validation_error("this boy is not enrolled in a curriculum"))?;
        if week <= 0 || week > enrollment_row.weeks {
            return Err(validation_error("week is out of range"));
        }
        let plan_rows = hs::week_plan(pool, enrollment_row.curriculum_id, week)
            .await
            .map_err(super::to_server_error)?
            .ok_or_else(|| validation_error("curriculum not found"))?;
        let plan = to_plan(&plan_rows, week)?;
        let enrollment = to_enrollment(&enrollment_row)?;
        let log_rows = hs::logs(pool, user_id, week)
            .await
            .map_err(super::to_server_error)?;
        let logs = to_log_rows(&log_rows)?;

        // H6: the anchor for a week that is not the current one is derived by
        // stepping the live anchor by the difference in week numbers.
        let delta = (week - enrollment_row.current_week) as i32 * 7;
        let anchor = sched::add_days(&enrollment_row.week_started_on, delta)
            .ok_or_else(|| validation_error("could not compute the week's anchor date"))?;
        let dated = week == enrollment_row.current_week;

        Ok(sched::week_grid(&plan, &enrollment, &logs, &anchor, dated))
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (user_id, week);
        unreachable!("server function bodies only run on the server")
    }
}

/// One calendar month for exactly one boy (§4 default 17). For an unenrolled
/// boy this returns extras-only days rather than an error.
#[server(endpoint = "get_month")]
pub async fn get_month(user_id: i64, year: i32, month: u32) -> Result<MonthView, ServerFnError> {
    #[cfg(feature = "server")]
    {
        if !(1..=12).contains(&month) {
            return Err(validation_error("month must be between 1 and 12"));
        }
        let pool = crate::server::db::read_pool()
            .await
            .map_err(super::to_server_error)?;
        let enrollment_row = hs::enrollment(pool, user_id)
            .await
            .map_err(super::to_server_error)?;

        let from = format!("{year:04}-{month:02}-01");
        let (next_year, next_month) = if month == 12 {
            (year + 1, 1)
        } else {
            (year, month + 1)
        };
        let first_of_next = format!("{next_year:04}-{next_month:02}-01");
        let to = sched::add_days(&first_of_next, -1)
            .ok_or_else(|| validation_error("could not compute the month's span"))?;

        let counts = hs::log_counts_between(pool, user_id, &from, &to)
            .await
            .map_err(super::to_server_error)?;
        let logs = synth_logs(&counts);

        let extra_rows = hs::extras_between(pool, user_id, &from, &to)
            .await
            .map_err(super::to_server_error)?;
        let extras = extra_rows
            .iter()
            .map(to_extra_task)
            .collect::<Result<Vec<_>, _>>()?;

        let enrollment = enrollment_row.as_ref().map(to_enrollment).transpose()?;
        // "The current week_plan only if its span intersects the month" (HS4
        // accept (l)): a curriculum row is fetched at all only when it can
        // actually contribute a dealt-out day to this month.
        let plan = match &enrollment_row {
            Some(row) => {
                let intersects =
                    sched::week_span(&row.week_started_on).is_some_and(|(span_from, span_to)| {
                        span_from.as_str() <= to.as_str() && span_to.as_str() >= from.as_str()
                    });
                if intersects {
                    let rows = hs::week_plan(pool, row.curriculum_id, row.current_week)
                        .await
                        .map_err(super::to_server_error)?
                        .ok_or_else(|| validation_error("curriculum not found"))?;
                    Some(to_plan(&rows, row.current_week)?)
                } else {
                    None
                }
            }
            None => None,
        };

        let today = today_string();
        Ok(sched::month_view(
            enrollment.as_ref(),
            plan.as_ref(),
            &logs,
            &extras,
            year,
            month,
            &today,
        ))
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (user_id, year, month);
        unreachable!("server function bodies only run on the server")
    }
}

// ---------------------------------------------------------------------------
// Mutations open to anyone on the LAN (H7: "tick / untick / skip / note a
// single boy's occurrence or extra | anyone — mirrors toggle_routine_task")
// ---------------------------------------------------------------------------

/// Tick, skip or untick one boy's occurrence. Validated against his own
/// recomputed current-week occurrences (H7) — never against an arbitrary
/// `week`.
#[allow(clippy::too_many_arguments)]
#[server(endpoint = "toggle_lesson")]
pub async fn toggle_lesson(
    user_id: i64,
    subject_id: i64,
    assignment_id: Option<i64>,
    week: i64,
    scheduled_date: String,
    completed: bool,
    status: LogStatus,
    note: Option<String>,
    date: String,
    idempotency_key: String,
) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        // QH1-10: the rest of the open surface caps its inputs; this one is
        // LAN-open and unbounded without this check.
        if note
            .as_deref()
            .is_some_and(|n| n.chars().count() > MAX_NOTE_CHARS)
        {
            return Err(validation_error("note is too long"));
        }
        // Accept (m): rejected before any write, ahead of even the date check.
        if subject_id <= 0 {
            return Err(validation_error("subject_id must be a positive id"));
        }
        check_date_window(&date)?;

        let read_pool = crate::server::db::read_pool()
            .await
            .map_err(super::to_server_error)?;
        let enrollment_row = hs::enrollment(read_pool, user_id)
            .await
            .map_err(super::to_server_error)?
            .ok_or_else(|| validation_error("this boy is not enrolled in a curriculum"))?;
        if week != enrollment_row.current_week {
            return Err(validation_error(
                "week does not match this boy's current week",
            ));
        }
        let plan_rows = hs::week_plan(read_pool, enrollment_row.curriculum_id, week)
            .await
            .map_err(super::to_server_error)?
            .ok_or_else(|| validation_error("curriculum not found"))?;
        let plan = to_plan(&plan_rows, week)?;
        let enrollment = to_enrollment(&enrollment_row)?;
        let occurrences = sched::occurrences(&plan, &enrollment);
        if find_occurrence(&occurrences, subject_id, assignment_id, &scheduled_date).is_none() {
            return Err(validation_error(
                "that is not a scheduled occurrence of this boy's current week",
            ));
        }

        let pool = crate::server::db::pool()
            .await
            .map_err(super::to_server_error)?;
        let payload = format!(
            r#"{{"user_id":{user_id},"subject_id":{subject_id},"assignment_id":{},"scheduled_date":"{scheduled_date}","completed":{completed}}}"#,
            assignment_id
                .map(|a| a.to_string())
                .unwrap_or_else(|| "null".into())
        );
        let mut tx = pool.begin().await.map_err(super::to_server_error)?;
        let claimed = crate::server::db::claim_mutation(
            &mut *tx,
            &idempotency_key,
            "toggle_lesson",
            user_id as u32,
            &payload,
        )
        .await
        .map_err(super::to_server_error)?;

        if claimed {
            let key = hs::OccurrenceKey::new(
                user_id,
                week,
                subject_id,
                assignment_id,
                scheduled_date.clone(),
            );
            // QH1-01: an occurrence that already has a log row is always
            // cleared first, so a Skip or a Note on an already-ticked row
            // replaces its state instead of the INSERT ... ON CONFLICT DO
            // NOTHING silently no-oping.
            if completed {
                hs::clear_occurrence(&mut *tx, &key)
                    .await
                    .map_err(super::to_server_error)?;
                hs::set_occurrence(&mut *tx, &key, status.as_str(), note.as_deref(), &date)
                    .await
                    .map_err(super::to_server_error)?;
            } else {
                hs::clear_occurrence(&mut *tx, &key)
                    .await
                    .map_err(super::to_server_error)?;
            }
        }
        tx.commit().await.map_err(super::to_server_error)?;

        super::realtime::publish(&ServerMessage::HomeschoolUpdated {
            user_ids: vec![user_id],
            week,
            date: scheduled_date,
        });
        Ok(())
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (
            user_id,
            subject_id,
            assignment_id,
            week,
            scheduled_date,
            completed,
            status,
            note,
            date,
            idempotency_key,
        );
        unreachable!("server function bodies only run on the server")
    }
}

/// Tick, skip or untick every unticked occurrence in `user_id`'s `due_today`
/// and `catch_up` (extras excluded — H6 item 4 names only "occurrences").
/// Idempotent: a second call with everything already ticked writes nothing.
#[server(endpoint = "mark_all_done")]
pub async fn mark_all_done(
    user_id: i64,
    week: i64,
    date: String,
    idempotency_key: String,
) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        check_date_window(&date)?;

        let read_pool = crate::server::db::read_pool()
            .await
            .map_err(super::to_server_error)?;
        let enrollment_row = hs::enrollment(read_pool, user_id)
            .await
            .map_err(super::to_server_error)?
            .ok_or_else(|| validation_error("this boy is not enrolled in a curriculum"))?;
        if week != enrollment_row.current_week {
            return Err(validation_error(
                "week does not match this boy's current week",
            ));
        }
        let plan_rows = hs::week_plan(read_pool, enrollment_row.curriculum_id, week)
            .await
            .map_err(super::to_server_error)?
            .ok_or_else(|| validation_error("curriculum not found"))?;
        let plan = to_plan(&plan_rows, week)?;
        let enrollment = to_enrollment(&enrollment_row)?;
        let log_rows = hs::logs(read_pool, user_id, week)
            .await
            .map_err(super::to_server_error)?;
        let logs = to_log_rows(&log_rows)?;
        let today = sched::today_view(&plan, &enrollment, &logs, &date);

        let mut to_tick: Vec<(i64, Option<i64>, String)> = Vec::new();
        for item in today.due_today.iter().chain(today.catch_up.iter()) {
            if let DayItem::Lesson(occurrence) = item {
                if occurrence.status.is_none() {
                    to_tick.push((
                        occurrence.subject_id,
                        occurrence.assignment_id,
                        occurrence.scheduled_date.clone(),
                    ));
                }
            }
        }

        let pool = crate::server::db::pool()
            .await
            .map_err(super::to_server_error)?;
        let payload = format!(r#"{{"user_id":{user_id},"week":{week}}}"#);
        let mut tx = pool.begin().await.map_err(super::to_server_error)?;
        let claimed = crate::server::db::claim_mutation(
            &mut *tx,
            &idempotency_key,
            "mark_all_done",
            user_id as u32,
            &payload,
        )
        .await
        .map_err(super::to_server_error)?;

        if claimed {
            for (subject_id, assignment_id, scheduled_date) in to_tick {
                let key = hs::OccurrenceKey::new(
                    user_id,
                    week,
                    subject_id,
                    assignment_id,
                    scheduled_date,
                );
                hs::set_occurrence(&mut *tx, &key, LogStatus::Done.as_str(), None, &date)
                    .await
                    .map_err(super::to_server_error)?;
            }
        }
        tx.commit().await.map_err(super::to_server_error)?;

        super::realtime::publish(&ServerMessage::HomeschoolUpdated {
            user_ids: vec![user_id],
            week,
            date,
        });
        Ok(())
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (user_id, week, date, idempotency_key);
        unreachable!("server function bodies only run on the server")
    }
}

/// Tick, skip or untick one extra. Anyone may — a boy on the TV can tick a
/// task his parent added (§4 default 6) — but only `add_extra`, `update_extra`
/// and `delete_extra` need the parent cookie.
#[server(endpoint = "toggle_extra")]
pub async fn toggle_extra(
    extra_id: i64,
    completed: bool,
    status: LogStatus,
    note: Option<String>,
    date: String,
    idempotency_key: String,
) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        // QH1-10: the rest of the open surface caps its inputs; this one is
        // LAN-open and unbounded without this check.
        if note
            .as_deref()
            .is_some_and(|n| n.chars().count() > MAX_NOTE_CHARS)
        {
            return Err(validation_error("note is too long"));
        }
        check_date_window(&date)?;

        let pool = crate::server::db::pool()
            .await
            .map_err(super::to_server_error)?;
        let extra = hs::extra(pool, extra_id)
            .await
            .map_err(super::to_server_error)?
            .ok_or_else(|| validation_error("unknown task"))?;

        let payload = format!(r#"{{"extra_id":{extra_id},"completed":{completed}}}"#);
        let mut tx = pool.begin().await.map_err(super::to_server_error)?;
        let claimed = crate::server::db::claim_mutation(
            &mut *tx,
            &idempotency_key,
            "toggle_extra",
            extra.profile_id as u32,
            &payload,
        )
        .await
        .map_err(super::to_server_error)?;

        if claimed {
            let new_status = completed.then_some(status.as_str());
            hs::set_extra_status(&mut *tx, extra_id, new_status, note.as_deref(), &date)
                .await
                .map_err(super::to_server_error)?;
        }
        tx.commit().await.map_err(super::to_server_error)?;

        super::realtime::publish(&ServerMessage::HomeschoolUpdated {
            user_ids: vec![extra.profile_id],
            week: 0,
            date: extra.scheduled_date,
        });
        Ok(())
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (extra_id, completed, status, note, date, idempotency_key);
        unreachable!("server function bodies only run on the server")
    }
}

// ---------------------------------------------------------------------------
// Parent-only mutations (H7)
// ---------------------------------------------------------------------------

/// Fan a shared occurrence's tick out to every boy in the Together group it
/// is valid for. Each member's `week_started_on` is recomputed independently
/// (they only share `curriculum_id` + `current_week`), so a member for whom
/// this triple is not a real, shared occurrence is silently excluded rather
/// than failing the whole call.
#[allow(clippy::too_many_arguments)]
#[server(endpoint = "toggle_lesson_together")]
pub async fn toggle_lesson_together(
    curriculum_id: i64,
    week: i64,
    subject_id: i64,
    assignment_id: Option<i64>,
    scheduled_date: String,
    completed: bool,
    date: String,
    idempotency_key: String,
    auth: String,
) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        crate::server::api::profiles::require_session_or_cookie(&auth).await?;
        if subject_id <= 0 {
            return Err(validation_error("subject_id must be a positive id"));
        }
        check_date_window(&date)?;

        let read_pool = crate::server::db::read_pool()
            .await
            .map_err(super::to_server_error)?;
        let members = hs::together_group(read_pool, curriculum_id, week)
            .await
            .map_err(super::to_server_error)?;
        if members.is_empty() {
            return Err(validation_error("no boy is on that curriculum and week"));
        }
        let plan_rows = hs::week_plan(read_pool, curriculum_id, week)
            .await
            .map_err(super::to_server_error)?
            .ok_or_else(|| validation_error("curriculum not found"))?;
        let plan = to_plan(&plan_rows, week)?;

        let mut matched: Vec<i64> = Vec::new();
        for member in &members {
            let enrollment = to_enrollment(member)?;
            let occurrences = sched::occurrences(&plan, &enrollment);
            if let Some(occurrence) =
                find_occurrence(&occurrences, subject_id, assignment_id, &scheduled_date)
            {
                if occurrence.shared {
                    matched.push(member.profile_id);
                }
            }
        }
        if matched.is_empty() {
            return Err(validation_error(
                "that is not a shared occurrence for any boy in this group",
            ));
        }

        let pool = crate::server::db::pool()
            .await
            .map_err(super::to_server_error)?;
        let payload = format!(
            r#"{{"curriculum_id":{curriculum_id},"week":{week},"subject_id":{subject_id},"scheduled_date":"{scheduled_date}","completed":{completed}}}"#
        );
        let mut tx = pool.begin().await.map_err(super::to_server_error)?;
        // H4: one transaction fans the tick out to every matched boy.
        let claimed = crate::server::db::claim_mutation(
            &mut *tx,
            &idempotency_key,
            "toggle_lesson_together",
            0,
            &payload,
        )
        .await
        .map_err(super::to_server_error)?;

        if claimed {
            for profile_id in &matched {
                let key = hs::OccurrenceKey::new(
                    *profile_id,
                    week,
                    subject_id,
                    assignment_id,
                    scheduled_date.clone(),
                );
                if completed {
                    hs::set_occurrence(&mut *tx, &key, LogStatus::Done.as_str(), None, &date)
                        .await
                        .map_err(super::to_server_error)?;
                } else {
                    hs::clear_occurrence(&mut *tx, &key)
                        .await
                        .map_err(super::to_server_error)?;
                }
            }
        }
        tx.commit().await.map_err(super::to_server_error)?;

        super::realtime::publish(&ServerMessage::HomeschoolUpdated {
            user_ids: matched,
            week,
            date: scheduled_date,
        });
        Ok(())
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (
            curriculum_id,
            week,
            subject_id,
            assignment_id,
            scheduled_date,
            completed,
            date,
            idempotency_key,
            auth,
        );
        unreachable!("server function bodies only run on the server")
    }
}

/// Move the week pointer: `week` is the destination directly, not a delta, so
/// both **Finish week** (`current_week + 1`, or `weeks + 1` at the end of the
/// year — H2's terminal state) and **Back a week** are this same call.
#[server(endpoint = "set_school_week")]
pub async fn set_school_week(
    user_id: i64,
    week: i64,
    date: String,
    auth: String,
) -> Result<EnrollmentView, ServerFnError> {
    #[cfg(feature = "server")]
    {
        crate::server::api::profiles::require_session_or_cookie(&auth).await?;
        check_date_window(&date)?;

        let pool = crate::server::db::pool()
            .await
            .map_err(super::to_server_error)?;
        let enrollment_row = hs::enrollment(pool, user_id)
            .await
            .map_err(super::to_server_error)?
            .ok_or_else(|| validation_error("this boy is not enrolled in a curriculum"))?;
        if week < 1 || week > enrollment_row.weeks + 1 {
            return Err(validation_error("week is out of range"));
        }

        hs::set_week(pool, user_id, week, &date)
            .await
            .map_err(super::to_server_error)?;
        let updated = hs::enrollment(pool, user_id)
            .await
            .map_err(super::to_server_error)?
            .ok_or_else(|| validation_error("enrollment vanished mid-request"))?;

        super::realtime::publish(&ServerMessage::HomeschoolUpdated {
            user_ids: vec![user_id],
            week,
            date,
        });
        Ok(enrollment_view(Some(&updated), user_id))
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (user_id, week, date, auth);
        unreachable!("server function bodies only run on the server")
    }
}

/// Enroll a boy (or move his existing enrollment onto a new curriculum / week
/// — `hs::upsert_enrollment` replaces rather than duplicating, HS1 accept (e)).
#[server(endpoint = "enroll")]
pub async fn enroll(
    user_id: i64,
    curriculum_id: i64,
    week: i64,
    school_days: String,
    date: String,
    auth: String,
) -> Result<EnrollmentView, ServerFnError> {
    #[cfg(feature = "server")]
    {
        crate::server::api::profiles::require_session_or_cookie(&auth).await?;
        check_date_window(&date)?;
        // H7: "days strings pass parse_days ... in both server fns that write
        // them" — this is the other one (`set_subject_schedule` is the first).
        sched::parse_days(&school_days).map_err(|e| validation_error(&e.to_string()))?;

        let pool = crate::server::db::pool()
            .await
            .map_err(super::to_server_error)?;
        let plan_rows = hs::week_plan(pool, curriculum_id, week)
            .await
            .map_err(super::to_server_error)?
            .ok_or_else(|| validation_error("unknown curriculum"))?;
        if week < 1 || week > plan_rows.curriculum.weeks + 1 {
            return Err(validation_error("week is out of range"));
        }

        hs::upsert_enrollment(pool, user_id, curriculum_id, week, &school_days, &date)
            .await
            .map_err(super::to_server_error)?;
        let updated = hs::enrollment(pool, user_id)
            .await
            .map_err(super::to_server_error)?
            .ok_or_else(|| validation_error("enrollment vanished mid-request"))?;

        super::realtime::publish(&ServerMessage::HomeschoolUpdated {
            user_ids: vec![user_id],
            week,
            date,
        });
        Ok(enrollment_view(Some(&updated), user_id))
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (user_id, curriculum_id, week, school_days, date, auth);
        unreachable!("server function bodies only run on the server")
    }
}

/// Unenroll: delete the enrollment, **keep the log** (§4 default 14).
#[server(endpoint = "unenroll")]
pub async fn unenroll(user_id: i64, auth: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        crate::server::api::profiles::require_session_or_cookie(&auth).await?;
        let pool = crate::server::db::pool()
            .await
            .map_err(super::to_server_error)?;
        let existing = hs::enrollment(pool, user_id)
            .await
            .map_err(super::to_server_error)?;
        let week = existing.as_ref().map(|e| e.current_week).unwrap_or(0);

        let removed = hs::unenroll(pool, user_id)
            .await
            .map_err(super::to_server_error)?;
        if removed == 0 {
            return Err(validation_error("this boy is not enrolled"));
        }

        super::realtime::publish(&ServerMessage::HomeschoolUpdated {
            user_ids: vec![user_id],
            week,
            date: today_string(),
        });
        Ok(())
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (user_id, auth);
        unreachable!("server function bodies only run on the server")
    }
}

/// School's out ⚽ / school's back (H2, W-14).
#[server(endpoint = "set_paused")]
pub async fn set_paused(
    user_id: i64,
    paused: bool,
    auth: String,
) -> Result<EnrollmentView, ServerFnError> {
    #[cfg(feature = "server")]
    {
        crate::server::api::profiles::require_session_or_cookie(&auth).await?;
        let pool = crate::server::db::pool()
            .await
            .map_err(super::to_server_error)?;
        let rows = hs::set_paused(pool, user_id, paused)
            .await
            .map_err(super::to_server_error)?;
        if rows == 0 {
            return Err(validation_error("this boy is not enrolled"));
        }
        let updated = hs::enrollment(pool, user_id)
            .await
            .map_err(super::to_server_error)?
            .ok_or_else(|| validation_error("enrollment vanished mid-request"))?;

        super::realtime::publish(&ServerMessage::HomeschoolUpdated {
            user_ids: vec![user_id],
            week: updated.current_week,
            date: today_string(),
        });
        Ok(enrollment_view(Some(&updated), user_id))
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (user_id, paused, auth);
        unreachable!("server function bodies only run on the server")
    }
}

/// Create or rewrite one week's text for one subject (H6 row action 6).
#[server(endpoint = "upsert_assignment")]
pub async fn upsert_assignment(
    subject_id: i64,
    week: i64,
    ordinal: i64,
    text: String,
    detail: Option<String>,
    auth: String,
) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        crate::server::api::profiles::require_session_or_cookie(&auth).await?;
        let pool = crate::server::db::pool()
            .await
            .map_err(super::to_server_error)?;
        let (curriculum_id, weeks) = subject_curriculum(pool, subject_id).await?;
        if week < 1 || week > weeks {
            return Err(validation_error("week is out of range for this curriculum"));
        }

        hs::upsert_assignment(pool, subject_id, week, ordinal, &text, detail.as_deref())
            .await
            .map_err(super::to_server_error)?;

        super::realtime::publish(&ServerMessage::CurriculumUpdated { curriculum_id });
        Ok(())
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (subject_id, week, ordinal, text, detail, auth);
        unreachable!("server function bodies only run on the server")
    }
}

/// School settings' per-subject controls: which days and whether shared (H6).
#[server(endpoint = "set_subject_schedule")]
pub async fn set_subject_schedule(
    subject_id: i64,
    days: String,
    shared: bool,
    auth: String,
) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        crate::server::api::profiles::require_session_or_cookie(&auth).await?;
        // H7 accept (i): rejected before anything is written.
        sched::parse_days(&days).map_err(|e| validation_error(&e.to_string()))?;

        let pool = crate::server::db::pool()
            .await
            .map_err(super::to_server_error)?;
        let (curriculum_id, _weeks) = subject_curriculum(pool, subject_id).await?;

        hs::set_subject_schedule(pool, subject_id, &days, shared)
            .await
            .map_err(super::to_server_error)?;

        super::realtime::publish(&ServerMessage::CurriculumUpdated { curriculum_id });
        Ok(())
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (subject_id, days, shared, auth);
        unreachable!("server function bodies only run on the server")
    }
}

/// Add a parent-authored task to a boy's date (H8).
#[allow(clippy::too_many_arguments)]
#[server(endpoint = "add_extra")]
pub async fn add_extra(
    user_id: i64,
    scheduled_date: String,
    title: String,
    category: Category,
    text: Option<String>,
    date: String,
    idempotency_key: String,
    auth: String,
) -> Result<ExtraTask, ServerFnError> {
    #[cfg(feature = "server")]
    {
        crate::server::api::profiles::require_session_or_cookie(&auth).await?;
        check_date_window(&date)?;
        if sched::weekday(&scheduled_date).is_none() {
            return Err(validation_error(
                "scheduled_date must be a valid YYYY-MM-DD date",
            ));
        }
        let today = today_string();
        let earliest = sched::add_days(&today, -365)
            .ok_or_else(|| validation_error("could not compute the allowed date range"))?;
        let latest = sched::add_days(&today, 365)
            .ok_or_else(|| validation_error("could not compute the allowed date range"))?;
        if scheduled_date.as_str() < earliest.as_str() || scheduled_date.as_str() > latest.as_str()
        {
            return Err(validation_error(
                "scheduled_date must be within a year of today",
            ));
        }

        let pool = crate::server::db::pool()
            .await
            .map_err(super::to_server_error)?;
        let payload = format!(
            r#"{{"user_id":{user_id},"scheduled_date":"{scheduled_date}","title":{title:?}}}"#
        );
        let mut tx = pool.begin().await.map_err(super::to_server_error)?;
        let claimed = crate::server::db::claim_mutation(
            &mut *tx,
            &idempotency_key,
            "add_extra",
            user_id as u32,
            &payload,
        )
        .await
        .map_err(super::to_server_error)?;

        let inserted = if claimed {
            Some(
                hs::add_extra(
                    &mut *tx,
                    user_id,
                    &scheduled_date,
                    &title,
                    category.as_str(),
                    text.as_deref(),
                )
                .await
                .map_err(super::to_server_error)?,
            )
        } else {
            None
        };
        tx.commit().await.map_err(super::to_server_error)?;

        let row = match inserted {
            Some(row) => row,
            None => {
                // A replay of an already-applied add: hand back the row the
                // first delivery created rather than creating a second one.
                let matches = hs::extras_between(pool, user_id, &scheduled_date, &scheduled_date)
                    .await
                    .map_err(super::to_server_error)?;
                matches
                    .into_iter()
                    .rev()
                    .find(|row| row.title == title)
                    .ok_or_else(|| validation_error("could not find the previously-created task"))?
            }
        };

        super::realtime::publish(&ServerMessage::HomeschoolUpdated {
            user_ids: vec![user_id],
            week: 0,
            date: scheduled_date,
        });
        to_extra_task(&row)
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (
            user_id,
            scheduled_date,
            title,
            category,
            text,
            date,
            idempotency_key,
            auth,
        );
        unreachable!("server function bodies only run on the server")
    }
}

/// Edit an extra's title, category, body or date. Never touches its status —
/// that is `toggle_extra`.
#[server(endpoint = "update_extra")]
pub async fn update_extra(
    extra_id: i64,
    title: String,
    category: Category,
    text: Option<String>,
    scheduled_date: String,
    auth: String,
) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        crate::server::api::profiles::require_session_or_cookie(&auth).await?;
        if sched::weekday(&scheduled_date).is_none() {
            return Err(validation_error(
                "scheduled_date must be a valid YYYY-MM-DD date",
            ));
        }

        let pool = crate::server::db::pool()
            .await
            .map_err(super::to_server_error)?;
        let existing = hs::extra(pool, extra_id)
            .await
            .map_err(super::to_server_error)?
            .ok_or_else(|| validation_error("unknown task"))?;

        hs::update_extra(
            pool,
            extra_id,
            &title,
            category.as_str(),
            text.as_deref(),
            &scheduled_date,
        )
        .await
        .map_err(super::to_server_error)?;

        super::realtime::publish(&ServerMessage::HomeschoolUpdated {
            user_ids: vec![existing.profile_id],
            week: 0,
            date: scheduled_date,
        });
        Ok(())
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (extra_id, title, category, text, scheduled_date, auth);
        unreachable!("server function bodies only run on the server")
    }
}

/// Delete an extra outright — ticked or not.
#[server(endpoint = "delete_extra")]
pub async fn delete_extra(extra_id: i64, auth: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        crate::server::api::profiles::require_session_or_cookie(&auth).await?;
        let pool = crate::server::db::pool()
            .await
            .map_err(super::to_server_error)?;
        let existing = hs::extra(pool, extra_id)
            .await
            .map_err(super::to_server_error)?
            .ok_or_else(|| validation_error("unknown task"))?;

        hs::delete_extra(pool, extra_id)
            .await
            .map_err(super::to_server_error)?;

        super::realtime::publish(&ServerMessage::HomeschoolUpdated {
            user_ids: vec![existing.profile_id],
            week: 0,
            date: existing.scheduled_date,
        });
        Ok(())
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (extra_id, auth);
        unreachable!("server function bodies only run on the server")
    }
}
