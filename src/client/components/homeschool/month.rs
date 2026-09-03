//! The **Month view** (H8, the owner's second addition): "see the month".
//!
//! A Mon–Fri grid with the weekend collapsed into a thin strip, for **exactly
//! one boy** (§4 default 17 / review finding D-4 — two boys' counts in one
//! cell is a number nobody can act on). Each school-day cell carries what is
//! actually knowable about that date and nothing more:
//!
//! | state | what the cell shows |
//! | --- | --- |
//! | inside the current week's span | `done / total` — the plan is dealt out |
//! | any other day with work logged | a bare done count |
//! | a day with parent-added tasks | 📌, whatever the pointer is doing |
//! | a Monday inside the span | the week number |
//!
//! A past week's plan is deliberately **not** reconstructed (H6): the
//! curriculum can be edited, the pointer moved and the school days changed, so
//! a denominator for last March would be a guess dressed as a fact.

use dioxus::prelude::*;

use crate::client::components::glyphs;
use crate::client::components::homeschool::today::BoyChips;
use crate::shared::homeschool::Weekday;
use crate::shared::types::{MonthDay, MonthView};

/// One row of the month grid: the five school days, plus that week's weekend.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct MonthWeek {
    /// Mon…Fri, `None` where the month has not started or has ended.
    pub weekdays: Vec<Option<MonthDay>>,
    /// Saturday and Sunday, for the thin strip.
    pub weekend: Vec<MonthDay>,
}

/// The five weekday columns of the grid, in order.
pub const MONTH_COLUMNS: [Weekday; 5] = [
    Weekday::Mon,
    Weekday::Tue,
    Weekday::Wed,
    Weekday::Thu,
    Weekday::Fri,
];

/// Fold a month's days into calendar weeks, Monday-first.
///
/// Pure and total: a month starting mid-week gets leading `None`s rather than
/// a shifted grid, and every day of the month appears exactly once across the
/// weekday cells and the weekend strips — which is what makes "30 day cells"
/// an assertion rather than a hope.
pub fn month_weeks(days: &[MonthDay]) -> Vec<MonthWeek> {
    let mut weeks: Vec<MonthWeek> = Vec::new();
    for day in days {
        let column = MONTH_COLUMNS
            .iter()
            .position(|candidate| *candidate == day.weekday);
        let start_new = match (weeks.last(), column) {
            (None, _) => true,
            // A new calendar week starts on Monday.
            (Some(_), Some(0)) => true,
            (Some(_), _) => false,
        };
        if start_new {
            weeks.push(MonthWeek {
                weekdays: vec![None; MONTH_COLUMNS.len()],
                weekend: Vec::new(),
            });
        }
        let week = weeks.last_mut().expect("a week was just pushed");
        match column {
            Some(index) => week.weekdays[index] = Some(day.clone()),
            None => week.weekend.push(day.clone()),
        }
    }
    weeks
}

/// The Month pane.
#[component]
pub fn MonthPanel(
    month: MonthView,
    label: String,
    /// Every enrolled boy. Month always shows exactly one of them, so with
    /// more than one enrolled the chip is a **required selector** (D-4) — not
    /// an optional filter, and never an "Everyone".
    boys: Vec<(i64, String)>,
    on_boy_filter: EventHandler<Option<i64>>,
    on_open_day: EventHandler<String>,
    on_step: EventHandler<i32>,
) -> Element {
    let user_id = month.user_id;
    let weeks = month_weeks(&month.days);

    rsx! {
        div { class: "flex flex-col gap-3", "data-month": "{month.year}-{month.month}",
            if boys.len() > 1 {
                BoyChips {
                    boys: boys.clone(),
                    selected: Some(user_id),
                    allow_everyone: false,
                    on_select: on_boy_filter,
                }
            }
            div { class: "flex items-center justify-between",
                button {
                    class: "rounded-xl bg-white px-3 py-2 text-sm font-bold text-sheffield-dark ring-1 ring-slate-200",
                    aria_label: "Previous month",
                    onclick: move |_| on_step.call(-1),
                    "←"
                }
                h3 { class: "text-lg font-bold text-sheffield-dark", "{label}" }
                button {
                    class: "rounded-xl bg-white px-3 py-2 text-sm font-bold text-sheffield-dark ring-1 ring-slate-200",
                    aria_label: "Next month",
                    onclick: move |_| on_step.call(1),
                    "→"
                }
            }

            div { class: "overflow-x-auto rounded-2xl bg-white p-2 shadow-sm ring-1 ring-slate-100",
                div { class: "flex min-w-[20rem] flex-col gap-1",
                    div { class: "flex gap-1",
                        for day in MONTH_COLUMNS {
                            div {
                                key: "{day.letter()}",
                                class: "flex-1 text-center text-xs font-bold text-slate-800",
                                "{day.letter()}"
                            }
                        }
                        div { class: "w-10 text-center text-xs font-bold text-slate-800", "SU" }
                    }

                    for (index , week) in weeks.into_iter().enumerate() {
                        div { key: "{index}", class: "flex items-stretch gap-1",
                            for (column , slot) in week.weekdays.clone().into_iter().enumerate() {
                                div { key: "{column}", class: "flex-1",
                                    if let Some(day) = slot {
                                        MonthCell { day, on_open_day }
                                    } else {
                                        div { class: "min-h-[44px] rounded-xl bg-slate-50" }
                                    }
                                }
                            }
                            div {
                                class: "flex w-10 flex-col gap-1",
                                "data-month-weekend": "true",
                                for day in week.weekend.clone() {
                                    WeekendCell { key: "{day.date}", day, on_open_day }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The day-of-month number, sliced off the `YYYY-MM-DD` date.
fn day_number(date: &str) -> &str {
    date.rsplit('-')
        .next()
        .unwrap_or(date)
        .trim_start_matches('0')
}

/// One school-day cell.
#[component]
fn MonthCell(day: MonthDay, on_open_day: EventHandler<String>) -> Element {
    let date = day.date.clone();
    rsx! {
        button {
            // The marker leads the element on purpose: a test slices the
            // rendered page from one `data-month-day` to the next, so a cell's
            // own classes must fall *inside* its own slice.
            "data-month-day": "{day.date}",
            class: if day.in_current_week { "flex min-h-[44px] w-full flex-col items-start rounded-xl bg-sheffield-light/25 p-1 text-left" } else { "flex min-h-[44px] w-full flex-col items-start rounded-xl bg-slate-50 p-1 text-left" },
            aria_label: "{day.date}",
            onclick: move |_| on_open_day.call(date.clone()),
            div { class: "flex w-full items-center justify-between",
                span { class: "text-xs font-bold text-slate-800", "{day_number(&day.date)}" }
                if day.extras > 0 {
                    span { class: "text-xs", aria_hidden: "true", "{glyphs::EXTRA_TASK_GLYPH}" }
                }
            }
            if let Some(total) = day.total {
                span { class: "text-xs font-semibold text-slate-600", "{day.done}/{total}" }
            } else if day.done > 0 {
                span { class: "text-xs font-semibold text-slate-600", "{day.done}" }
            }
            if day.weekday == Weekday::Mon {
                if let Some(week) = day.week {
                    span { class: "text-xs text-slate-600", "wk {week}" }
                }
            }
        }
    }
}

/// One day of the collapsed weekend strip.
#[component]
fn WeekendCell(day: MonthDay, on_open_day: EventHandler<String>) -> Element {
    let date = day.date.clone();
    rsx! {
        button {
            "data-month-day": "{day.date}",
            class: "flex min-h-[20px] w-full items-center justify-between rounded-md bg-slate-50 px-1 text-left",
            aria_label: "{day.date}",
            onclick: move |_| on_open_day.call(date.clone()),
            span { class: "text-xs text-slate-600", "{day_number(&day.date)}" }
            if day.extras > 0 {
                span { class: "text-xs", aria_hidden: "true", "{glyphs::EXTRA_TASK_GLYPH}" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(date: &str, weekday: Weekday) -> MonthDay {
        MonthDay {
            date: date.to_string(),
            weekday,
            is_school_day: !matches!(weekday, Weekday::Sat | Weekday::Sun),
            in_current_week: false,
            week: None,
            done: 0,
            total: None,
            extras: 0,
        }
    }

    /// September 2026 starts on a Tuesday and has 30 days.
    fn september_2026() -> Vec<MonthDay> {
        const ORDER: [Weekday; 7] = [
            Weekday::Tue,
            Weekday::Wed,
            Weekday::Thu,
            Weekday::Fri,
            Weekday::Sat,
            Weekday::Sun,
            Weekday::Mon,
        ];
        (1..=30)
            .map(|number| {
                day(
                    &format!("2026-09-{number:02}"),
                    ORDER[(number as usize - 1) % 7],
                )
            })
            .collect()
    }

    #[test]
    fn every_day_of_the_month_lands_in_exactly_one_cell() {
        let weeks = month_weeks(&september_2026());
        let cells: usize = weeks
            .iter()
            .map(|week| week.weekdays.iter().flatten().count() + week.weekend.len())
            .sum();
        assert_eq!(cells, 30, "every day of September 2026 gets one cell");
    }

    #[test]
    fn a_month_starting_mid_week_gets_leading_blanks_not_a_shifted_grid() {
        let weeks = month_weeks(&september_2026());
        assert_eq!(weeks.len(), 5, "September 2026 spans five calendar weeks");
        assert!(
            weeks[0].weekdays[0].is_none(),
            "the 1st is a Tuesday, so Monday's cell stays empty"
        );
        assert_eq!(
            weeks[0].weekdays[1].as_ref().map(|day| day.date.as_str()),
            Some("2026-09-01")
        );
        assert_eq!(weeks[0].weekend.len(), 2);
    }

    #[test]
    fn each_new_monday_opens_a_new_row() {
        let weeks = month_weeks(&september_2026());
        for week in weeks.iter().skip(1) {
            assert_eq!(
                week.weekdays[0].as_ref().map(|day| day.weekday),
                Some(Weekday::Mon),
                "every row after the first starts on a Monday"
            );
        }
    }

    #[test]
    fn the_day_number_loses_its_leading_zero_but_never_its_digits() {
        assert_eq!(day_number("2026-09-01"), "1");
        assert_eq!(day_number("2026-09-30"), "30");
        assert_eq!(day_number("2026-09-10"), "10");
    }

    #[test]
    fn an_empty_month_folds_to_no_weeks_at_all() {
        assert!(month_weeks(&[]).is_empty());
    }
}
