//! The **Year view** (H8, the owner's first addition): "the parents should be
//! able to see all 36 weeks, Monday through Friday".
//!
//! A week picker over a **subject × school-day grid** built by the same
//! occurrence rule the Today list uses, so what a parent reads here is exactly
//! what will be dealt out — not a second, hand-maintained description of the
//! plan that can drift away from it.
//!
//! Two rules make the surface honest about time (§4 default 17, review
//! finding D-5):
//!
//! * **Only the current week is dated and tickable.** Every other week's
//!   `scheduled_date` is advisory — derived from a synthetic anchor — so the
//!   columns are labelled by weekday name alone and no checkbox is drawn. A
//!   control that the server would reject is worse than no control.
//! * **Every week 1…n is still shown.** The pointer decides what is *dealt*,
//!   not what is *visible*; a parent planning a term needs to see week 30 in
//!   September.
//!
//! The grid scrolls inside its own `overflow-x-auto` container and every row
//! is at least 44 px tall, which is the white team's W-9 concern answered with
//! a sheet instead of in-cell editing.

use std::collections::BTreeMap;

use dioxus::prelude::*;

use crate::client::components::glyphs;
use crate::client::components::homeschool::row::{part_label, weekday_short};
use crate::client::components::homeschool::settings::{SHEET_CARD_CLASS, SHEET_SCRIM_CLASS};
use crate::client::components::homeschool::today::BoyChips;
use crate::client::components::homeschool::SchoolAction;
use crate::client::components::mobile::session;
use crate::shared::homeschool::{date_for, days_to_string, LogStatus, Weekday};
use crate::shared::types::{LessonOccurrence, WeekGrid, WeekGridRow};

/// Every grid row — the weekday header included — is at least this tall, so a
/// thumb can hit a cell (`min-h-[44px]`, W-9).
pub const YEAR_ROW_CLASS: &str = "flex min-h-[44px] items-stretch gap-1";

/// How much of a cell's text fits before it is elided (H6: "the first ~18
/// characters").
pub const YEAR_CELL_CHARS: usize = 18;

/// The first [`YEAR_CELL_CHARS`] characters of `text`, with an ellipsis when
/// something was dropped. Counted in `char`s, not bytes: a curly quote must
/// not be able to split mid-character.
pub fn short_cell_text(text: &str) -> String {
    let mut out: String = text.chars().take(YEAR_CELL_CHARS).collect();
    if text.chars().count() > YEAR_CELL_CHARS {
        out.push('…');
    }
    out
}

/// Recover each assignment row's `ordinal` from one grid row's cells.
///
/// Exact here, unlike the Today list's best effort: a grid row holds the
/// whole week at once, and H3 rule 2 deals a subject's rows out in ordinal
/// order, so first appearance across the cells *is* the ordinal.
pub fn row_ordinals(row: &WeekGridRow) -> BTreeMap<i64, i64> {
    let mut order: Vec<i64> = Vec::new();
    for cell in &row.cells {
        for occurrence in cell {
            if let Some(assignment_id) = occurrence.assignment_id {
                if !order.contains(&assignment_id) {
                    order.push(assignment_id);
                }
            }
        }
    }
    order
        .into_iter()
        .enumerate()
        .map(|(index, assignment_id)| (assignment_id, index as i64 + 1))
        .collect()
}

/// The days **one row of one week** is dealt out on, as a `days` string.
///
/// H6/D-5 calls the cell sheet's second control "inline edit of
/// `assignment.text` / `days` **for that week**", and H3 rule 1 honours
/// `assignments.days` as an override of `subjects.days` — a row that carries
/// its own days is pinned to exactly those days and takes no part in rule 5's
/// spread. The grid in hand already shows the resolved answer: the columns
/// this row occupies **are** `assignment.days ∨ subject.days` intersected with
/// the boy's school days, so first-hand derivation beats a second copy of the
/// rule. Empty (which a rendered entry never is) falls back to the grid's own
/// day columns.
pub fn entry_days(row: &WeekGridRow, days: &[Weekday], assignment_id: Option<i64>) -> Vec<Weekday> {
    let dealt: Vec<Weekday> = row
        .cells
        .iter()
        .enumerate()
        .filter(|(_, cell)| {
            cell.iter()
                .any(|occurrence| occurrence.assignment_id == assignment_id)
        })
        .filter_map(|(column, _)| days.get(column).copied())
        .collect();
    if dealt.is_empty() {
        days.to_vec()
    } else {
        dealt
    }
}

/// The Year pane.
#[component]
pub fn YearPanel(
    grid: WeekGrid,
    /// The boy the grid is drawn for — Year is filtered by the same boy chip
    /// as Today.
    user_id: i64,
    /// `enrollments.current_week` — the one week that is dated and tickable.
    current_week: i64,
    /// The `week_started_on` this grid is laid out against (HS4's synthetic
    /// anchor for a non-current week).
    anchor: String,
    /// Every enrolled boy, for the chip strip (H6: "a boy chip filters Year
    /// exactly as it filters Today"). Empty or single → no strip.
    boys: Vec<(i64, String)>,
    on_boy_filter: EventHandler<Option<i64>>,
    on_select_week: EventHandler<i64>,
    on_action: EventHandler<SchoolAction>,
) -> Element {
    let mut open_cell = use_signal(|| Option::<(usize, usize)>::None);

    let dated = grid.dated;
    let weeks = grid.weeks;
    let week = grid.week;
    let days = grid.days.clone();
    let rows = grid.rows.clone();

    rsx! {
        div { class: "flex flex-col gap-4",
            if boys.len() > 1 {
                BoyChips {
                    boys: boys.clone(),
                    selected: Some(user_id),
                    allow_everyone: false,
                    on_select: on_boy_filter,
                }
            }
            div {
                class: "flex flex-wrap gap-2",
                role: "group",
                aria_label: "Week",
                "data-year-picker": "true",
                for candidate in 1..=weeks {
                    button {
                        key: "{candidate}",
                        class: if candidate == week { "rounded-full bg-sheffield-dark px-3 py-2 text-sm font-bold text-white" } else { "rounded-full bg-white px-3 py-2 text-sm font-semibold text-sheffield-dark ring-1 ring-slate-200" },
                        "data-year-week": "{candidate}",
                        aria_current: if candidate == current_week { "true" } else { "false" },
                        onclick: move |_| on_select_week.call(candidate),
                        "{candidate}"
                        if candidate < current_week {
                            span { class: "ml-1", aria_hidden: "true", "✓" }
                        }
                    }
                }
            }

            p { class: "text-sm text-slate-600",
                if dated {
                    "Week {week} of {weeks} · this week — tick anything here."
                } else {
                    "Week {week} of {weeks} · weekdays only until week {current_week} is finished."
                }
            }

            div { class: "overflow-x-auto rounded-2xl bg-white p-2 shadow-sm ring-1 ring-slate-100",
                "data-year-grid": "true",
                div { class: "flex min-w-[38rem] flex-col gap-1",
                    div { class: YEAR_ROW_CLASS, "data-year-row": "head",
                        div { class: "w-32 shrink-0 self-center text-xs font-bold text-slate-800",
                            "Subject"
                        }
                        for day in days.clone() {
                            div {
                                key: "{day.letter()}",
                                class: "flex-1 self-center text-center text-xs font-bold text-slate-800",
                                "data-year-col": "{day.letter()}",
                                "{weekday_short(day)}"
                                if dated {
                                    if let Some(date) = date_for(&anchor, day) {
                                        span { class: "block font-semibold text-slate-600", "{date}" }
                                    }
                                }
                            }
                        }
                    }

                    for (row_index , row) in rows.clone().into_iter().enumerate() {
                        div {
                            key: "{row.subject_id}",
                            class: YEAR_ROW_CLASS,
                            "data-year-row": "{row.subject_id}",
                            "data-year-subject": "{row.subject_id}",
                            div { class: "flex w-32 shrink-0 items-center gap-1 text-xs font-semibold text-slate-800",
                                span { class: "text-base", aria_hidden: "true",
                                    "{glyphs::category_glyph(row.category.as_str())}"
                                }
                                span { class: "truncate", "{row.title}" }
                            }
                            for (column , cell) in row.cells.clone().into_iter().enumerate() {
                                div {
                                    key: "{column}",
                                    // Marker first: a test slices the page from
                                    // one cell's marker to the next.
                                    "data-year-cell": "{row.subject_id}-{column}",
                                    class: "flex flex-1 flex-col gap-1 rounded-xl bg-slate-50 p-1",
                                    for (index , occurrence) in cell.into_iter().enumerate() {
                                        div {
                                            key: "{index}",
                                            class: "flex items-center gap-1",
                                            "data-year-entry": "true",
                                            if dated {
                                                button {
                                                    class: if matches!(occurrence.status, Some(LogStatus::Done)) { "flex h-6 w-6 shrink-0 items-center justify-center rounded-md bg-sheffield-dark text-xs text-white" } else { "flex h-6 w-6 shrink-0 items-center justify-center rounded-md border-2 border-sheffield-light text-xs" },
                                                    "data-lesson-check": "true",
                                                    aria_label: "Tick {row.title}",
                                                    onclick: {
                                                        let occurrence = occurrence.clone();
                                                        move |_| {
                                                            on_action
                                                                .call(SchoolAction::ToggleLesson {
                                                                    user_id,
                                                                    week,
                                                                    subject_id: occurrence.subject_id,
                                                                    assignment_id: occurrence.assignment_id,
                                                                    scheduled_date: occurrence.scheduled_date.clone(),
                                                                    completed: occurrence.status.is_none(),
                                                                    status: LogStatus::Done,
                                                                    note: None,
                                                                })
                                                        }
                                                    },
                                                    if matches!(occurrence.status, Some(LogStatus::Done)) {
                                                        "✓"
                                                    }
                                                }
                                            }
                                            button {
                                                class: "min-w-0 flex-1 text-left text-xs text-slate-800",
                                                onclick: move |_| open_cell.set(Some((row_index, column))),
                                                "{short_cell_text(occurrence.text.as_deref().unwrap_or(&row.title))}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if let Some((row_index , column)) = open_cell() {
                if let Some(row) = rows.get(row_index) {
                    YearCellSheet {
                        row: row.clone(),
                        days: days.clone(),
                        column,
                        week,
                        dated,
                        on_action,
                        on_close: move |()| open_cell.set(None),
                    }
                }
            }
        }
    }
}

/// One cell, opened: the full text, its `part k of n`, and — for a parent —
/// inline edit of **this week's** `assignment.text` and `days`.
///
/// Both controls are per **entry**, not per subject. Until QA round 3
/// (QH3-04) the days control here sent `SetSubjectSchedule`, rewriting
/// `subjects.days` for all 36 weeks while the parent believed they had moved
/// one week's reading — and it started empty, so it did not even show what it
/// was about to overwrite. The subject-wide control it used to duplicate lives
/// in **School settings**, which is the one place that is honest about its
/// reach.
#[component]
pub fn YearCellSheet(
    row: WeekGridRow,
    /// The grid's day columns, so an entry's own days can be read off the row.
    days: Vec<Weekday>,
    column: usize,
    week: i64,
    dated: bool,
    on_action: EventHandler<SchoolAction>,
    on_close: EventHandler<()>,
) -> Element {
    let parent = session::is_parent();
    let ordinals = row_ordinals(&row);
    let cell: Vec<LessonOccurrence> = row.cells.get(column).cloned().unwrap_or_default();
    let subject_id = row.subject_id;

    rsx! {
        div { class: SHEET_SCRIM_CLASS, role: "dialog", aria_modal: "true",
            aria_label: "{row.title}",
            div { class: SHEET_CARD_CLASS, "data-year-sheet": "{subject_id}-{column}",
                div { class: "flex items-center justify-between",
                    h2 { class: "text-xl font-bold text-sheffield-dark", "{row.title}" }
                    button {
                        class: "rounded-xl px-3 py-2 text-sm font-semibold text-slate-600",
                        onclick: move |_| on_close.call(()),
                        "Close"
                    }
                }

                if cell.is_empty() {
                    p { class: "mt-3 text-sm text-slate-600", "Nothing this day." }
                }

                for occurrence in cell.clone() {
                    CellEntry {
                        key: "a{occurrence.assignment_id.unwrap_or(0)}",
                        occurrence: occurrence.clone(),
                        subject_id,
                        week,
                        dated,
                        parent,
                        ordinal: occurrence
                            .assignment_id
                            .and_then(|id| ordinals.get(&id).copied())
                            .unwrap_or(1),
                        entry_days: days_to_string(
                            &entry_days(&row, &days, occurrence.assignment_id),
                        ),
                        on_action,
                    }
                }
            }
        }
    }
}

/// One occurrence inside the cell sheet.
///
/// Both parent controls write the **same row** — `upsert_assignment` replaces
/// `text`, `detail` and `days` together — so each of them sends all three: the
/// text control carries the days on screen, the days control carries the text
/// on screen, and neither can quietly erase the other's value or the source's
/// `detail` (QH3-02, QH3-04).
#[component]
fn CellEntry(
    occurrence: LessonOccurrence,
    subject_id: i64,
    week: i64,
    dated: bool,
    parent: bool,
    ordinal: i64,
    /// This row's own days **for this week**, prefilled into the days control
    /// so the parent can see what they are about to change.
    entry_days: String,
    on_action: EventHandler<SchoolAction>,
) -> Element {
    let mut draft = use_signal(|| occurrence.text.clone().unwrap_or_default());
    let mut days = use_signal(|| entry_days.clone());
    let detail = occurrence.detail.clone();
    let title = occurrence.title.clone();
    let body = occurrence
        .text
        .clone()
        .unwrap_or_else(|| "No text for this week yet.".to_string());

    rsx! {
        div { class: "mt-3 rounded-2xl bg-white p-3 ring-1 ring-slate-100",
            if let Some(part) = part_label(occurrence.part) {
                p { class: "text-xs font-semibold text-slate-600", "{part}" }
            }
            p { class: "text-sm text-slate-800", "{body}" }
            if dated {
                p { class: "mt-1 text-xs text-slate-600", "due {occurrence.scheduled_date}" }
            }
            if parent {
                div { class: "mt-2 flex items-center gap-2",
                    input {
                        class: "w-full rounded-xl border border-slate-200 bg-white p-2 text-sm text-slate-800",
                        r#type: "text",
                        aria_label: "This week's text",
                        value: "{draft}",
                        oninput: move |event| draft.set(event.value()),
                    }
                    button {
                        class: "shrink-0 rounded-xl bg-sheffield-dark px-3 py-2 text-sm font-bold text-white",
                        onclick: {
                            let detail = detail.clone();
                            move |_| {
                                on_action
                                    .call(SchoolAction::EditAssignment {
                                        subject_id,
                                        week,
                                        ordinal,
                                        text: draft(),
                                        detail: detail.clone(),
                                        days: pinned_days(&days()),
                                    })
                            }
                        },
                        "Save"
                    }
                }
                div { class: "mt-2 flex flex-wrap items-center gap-2",
                    span { class: "text-xs font-semibold text-slate-600",
                        "Days in week {week} only"
                    }
                    input {
                        class: "w-28 rounded-xl border border-slate-200 bg-white p-2 text-sm text-slate-800",
                        r#type: "text",
                        aria_label: "Days for {title} in week {week}",
                        placeholder: "MTWRF",
                        value: "{days}",
                        oninput: move |event| days.set(event.value().to_uppercase()),
                    }
                    button {
                        class: "rounded-xl bg-sheffield-dark px-3 py-2 text-sm font-bold text-white disabled:opacity-50",
                        disabled: days().trim().is_empty(),
                        onclick: {
                            let detail = detail.clone();
                            move |_| {
                                on_action
                                    .call(SchoolAction::EditAssignment {
                                        subject_id,
                                        week,
                                        ordinal,
                                        text: draft(),
                                        detail: detail.clone(),
                                        days: Some(days()),
                                    })
                            }
                        },
                        "Save days"
                    }
                }
            }
        }
    }
}

/// A days control's value as `upsert_assignment` wants it: `None` (inherit the
/// subject's days) rather than an empty string the schema's `GLOB` would take.
fn pinned_days(letters: &str) -> Option<String> {
    let trimmed = letters.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::homeschool::{Category, Weekday};

    fn occurrence(assignment_id: Option<i64>, text: &str) -> LessonOccurrence {
        LessonOccurrence {
            subject_id: 5,
            assignment_id,
            week: 2,
            scheduled_date: "2026-09-08".into(),
            weekday: Weekday::Tue,
            category: Category::Reading,
            title: "Twice Told".into(),
            text: Some(text.to_string()),
            detail: None,
            source: None,
            icon_name: None,
            part: None,
            shared: true,
            sort_order: 5,
            status: None,
            note: None,
            days: None,
        }
    }

    #[test]
    fn a_cell_shows_the_first_eighteen_characters_and_says_so() {
        assert_eq!(short_cell_text("The Tin Whistle"), "The Tin Whistle");
        assert_eq!(
            short_cell_text("The Tin Whistle and the Second Telling"),
            "The Tin Whistle an…"
        );
    }

    #[test]
    fn truncation_counts_characters_not_bytes() {
        // Nineteen `é`s is 38 bytes; slicing by byte would panic mid-char.
        let text = "é".repeat(19);
        let short = short_cell_text(&text);
        assert_eq!(short.chars().count(), YEAR_CELL_CHARS + 1);
    }

    #[test]
    fn ordinals_come_from_first_appearance_across_the_week() {
        let row = WeekGridRow {
            subject_id: 5,
            title: "Twice Told".into(),
            category: Category::Reading,
            shared: true,
            cells: vec![
                Vec::new(),
                vec![occurrence(Some(51), "one"), occurrence(Some(52), "two")],
                Vec::new(),
                Vec::new(),
                Vec::new(),
            ],
        };
        let ordinals = row_ordinals(&row);
        assert_eq!(ordinals.get(&51), Some(&1));
        assert_eq!(ordinals.get(&52), Some(&2));
    }

    #[test]
    fn a_daily_row_with_no_assignment_rows_has_no_ordinals_to_recover() {
        let row = WeekGridRow {
            subject_id: 1,
            title: "Sums".into(),
            category: Category::Daily,
            shared: false,
            cells: vec![vec![occurrence(None, "")], Vec::new()],
        };
        assert!(row_ordinals(&row).is_empty());
    }

    #[test]
    fn every_grid_row_is_at_least_a_thumb_tall() {
        assert!(YEAR_ROW_CLASS.contains("min-h-[44px]"));
    }

    #[test]
    fn an_entrys_days_are_the_columns_it_is_dealt_out_on() {
        // QH3-04: the cell sheet edits `assignments.days` **for that week**,
        // so the control has to open showing the days this row already falls
        // on — which the grid in hand has already resolved.
        let days = fixture_days("MTWRF");
        let row = WeekGridRow {
            subject_id: 3,
            title: "Old Tales".into(),
            category: Category::Reading,
            shared: true,
            cells: vec![
                vec![occurrence(Some(32), "part one")],
                Vec::new(),
                vec![occurrence(Some(32), "part two")],
                Vec::new(),
                vec![occurrence(Some(33), "the other row")],
            ],
        };
        assert_eq!(
            days_to_string(&entry_days(&row, &days, Some(32))),
            "MW",
            "a split reading row is pinned to the two days it was dealt on"
        );
        assert_eq!(days_to_string(&entry_days(&row, &days, Some(33))), "F");
        // A row the grid does not hold falls back to the grid's own columns
        // rather than to nothing, which the server would reject.
        assert_eq!(days_to_string(&entry_days(&row, &days, Some(99))), "MTWRF");
    }

    #[test]
    fn an_empty_days_control_inherits_the_subject_rather_than_writing_nonsense() {
        assert_eq!(pinned_days("MW"), Some("MW".to_string()));
        assert_eq!(pinned_days(" MW "), Some("MW".to_string()));
        assert_eq!(pinned_days("   "), None);
        assert_eq!(pinned_days(""), None);
    }

    fn fixture_days(letters: &str) -> Vec<Weekday> {
        crate::shared::homeschool::parse_days(letters).expect("fixture day letters parse")
    }
}
