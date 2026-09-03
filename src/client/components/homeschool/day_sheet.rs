//! The **day sheet** — one date, one boy (H8).
//!
//! Opened by tapping a cell in the Month view. It holds three things:
//!
//! 1. that date's curriculum items, **but only when the date lies inside the
//!    current week's span** — a future week has not been dealt out until the
//!    parent finishes the previous one, and the sheet says so in as many
//!    words rather than showing an empty list that looks like a bug;
//! 2. the boy's parent-added tasks for that date, which are independent of the
//!    curriculum pointer and therefore render on **any** date;
//! 3. the parent-only **Add task** form: boy · title · category · text.
//!
//! Extras can be ticked from here by anyone (§4 default 6 — a boy on the
//! television can tick a task his parent made) but only a parent can create or
//! delete one, which is exactly what the server enforces (H7).

use dioxus::prelude::*;

use crate::client::components::glyphs;
use crate::client::components::homeschool::row::{ExtraRow, LessonRow};
use crate::client::components::homeschool::settings::{SHEET_CARD_CLASS, SHEET_SCRIM_CLASS};
use crate::client::components::homeschool::SchoolAction;
use crate::client::components::mobile::session;
use crate::shared::homeschool::{Category, LogStatus};
use crate::shared::types::{profile_name, DayItem, FAMILY_PROFILE_COUNT};

/// The longest title `lesson_extras.title`'s CHECK will accept (H1).
pub const EXTRA_TITLE_MAX: usize = 80;

/// The three categories an extra may carry, with the glyph each renders
/// with — `free_read` is deliberately absent: it is reference material, never
/// an occurrence (§4 default 12).
pub const EXTRA_CATEGORIES: [(Category, &str); 3] = [
    (Category::Daily, "Daily"),
    (Category::Reading, "Reading"),
    (Category::Weekly, "Weekly"),
];

/// The line a date beyond the current week's span shows instead of a plan.
pub fn not_dealt_out_line(week: i64) -> String {
    format!("Not dealt out yet — finish week {week} first.")
}

#[component]
pub fn DaySheet(
    date: String,
    /// The boy's `current_week` — the week a parent must finish first.
    week: i64,
    in_current_week: bool,
    user_id: i64,
    items: Vec<DayItem>,
    on_action: EventHandler<SchoolAction>,
    on_close: EventHandler<()>,
) -> Element {
    let parent = session::is_parent();

    let lessons: Vec<DayItem> = if in_current_week {
        items
            .iter()
            .filter(|item| matches!(item, DayItem::Lesson(_)))
            .cloned()
            .collect()
    } else {
        Vec::new()
    };
    let extras: Vec<DayItem> = items
        .iter()
        .filter(|item| matches!(item, DayItem::Extra(_)))
        .cloned()
        .collect();

    rsx! {
        div { class: SHEET_SCRIM_CLASS, role: "dialog", aria_modal: "true", aria_label: "{date}",
            div { class: SHEET_CARD_CLASS, "data-day-sheet": "{date}",
                div { class: "flex items-center justify-between",
                    h2 { class: "text-xl font-bold text-sheffield-dark", "{date}" }
                    button {
                        class: "rounded-xl px-3 py-2 text-sm font-semibold text-slate-600",
                        onclick: move |_| on_close.call(()),
                        "Close"
                    }
                }

                if !in_current_week {
                    p { class: "mt-3 rounded-2xl bg-sheffield-sun/20 p-4 text-sm font-semibold text-slate-800",
                        "{not_dealt_out_line(week)}"
                    }
                }

                ul { class: "mt-3 flex flex-col gap-2",
                    for item in lessons.clone() {
                        if let DayItem::Lesson(occurrence) = item {
                            LessonRow {
                                key: "s{occurrence.subject_id}-a{occurrence.assignment_id.unwrap_or(0)}",
                                occurrence: occurrence.clone(),
                                tickable: true,
                                on_toggle: {
                                    let occurrence = occurrence.clone();
                                    move |completed: bool| {
                                        on_action
                                            .call(SchoolAction::ToggleLesson {
                                                user_id,
                                                week,
                                                subject_id: occurrence.subject_id,
                                                assignment_id: occurrence.assignment_id,
                                                scheduled_date: occurrence.scheduled_date.clone(),
                                                completed,
                                                status: LogStatus::Done,
                                                note: None,
                                            })
                                    }
                                },
                                on_skip: move |()| {},
                                on_edit: move |_: String| {},
                                on_note: move |_: String| {},
                            }
                        }
                    }
                    for item in extras.clone() {
                        if let DayItem::Extra(extra) = item {
                            ExtraRow {
                                key: "x{extra.id}",
                                extra: extra.clone(),
                                can_edit: parent,
                                on_toggle: {
                                    let extra_id = extra.id;
                                    move |completed: bool| {
                                        on_action
                                            .call(SchoolAction::ToggleExtra {
                                                user_id,
                                                extra_id,
                                                completed,
                                                status: LogStatus::Done,
                                            })
                                    }
                                },
                                on_delete: {
                                    let extra_id = extra.id;
                                    move |()| on_action.call(SchoolAction::DeleteExtra { extra_id })
                                },
                            }
                        }
                    }
                }

                if parent {
                    AddTaskForm { date: date.clone(), user_id, on_action }
                }
            }
        }
    }
}

/// Boy · title · category · optional text (H8).
#[component]
fn AddTaskForm(date: String, user_id: i64, on_action: EventHandler<SchoolAction>) -> Element {
    let mut boy = use_signal(|| user_id);
    let mut title = use_signal(String::new);
    let mut text = use_signal(String::new);
    let mut category = use_signal(|| Category::Daily);

    rsx! {
        div { class: "mt-6 flex flex-col gap-3", "data-add-task": "true",
            h3 { class: "text-lg font-bold text-sheffield-dark", "Add task" }

            label { class: "block text-sm font-semibold text-slate-600",
                "Boy"
                select {
                    class: "mt-1 w-full rounded-xl border border-slate-200 bg-white p-3 text-base text-slate-800",
                    value: "{boy}",
                    onchange: move |event| {
                        if let Ok(parsed) = event.value().parse::<i64>() {
                            boy.set(parsed);
                        }
                    },
                    for candidate in 1..=FAMILY_PROFILE_COUNT {
                        option { key: "{candidate}", value: "{candidate}", "{profile_name(candidate)}" }
                    }
                }
            }

            label { class: "block text-sm font-semibold text-slate-600",
                "What needs doing?"
                input {
                    class: "mt-1 w-full rounded-xl border border-slate-200 bg-white p-3 text-base text-slate-800",
                    r#type: "text",
                    maxlength: "{EXTRA_TITLE_MAX}",
                    value: "{title}",
                    oninput: move |event| title.set(event.value()),
                }
            }

            div { class: "flex gap-2", role: "group", aria_label: "Kind of task",
                for (candidate , label) in EXTRA_CATEGORIES {
                    button {
                        key: "{candidate.as_str()}",
                        class: if category() == candidate { "flex-1 rounded-xl bg-sheffield-dark px-3 py-2 text-sm font-bold text-white" } else { "flex-1 rounded-xl bg-white px-3 py-2 text-sm font-semibold text-sheffield-dark ring-1 ring-slate-200" },
                        "data-extra-category": "{candidate.as_str()}",
                        aria_pressed: if category() == candidate { "true" } else { "false" },
                        onclick: move |_| category.set(candidate),
                        span { class: "mr-1", aria_hidden: "true",
                            "{glyphs::category_glyph(candidate.as_str())}"
                        }
                        "{label}"
                    }
                }
            }

            label { class: "block text-sm font-semibold text-slate-600",
                "Notes (optional)"
                input {
                    class: "mt-1 w-full rounded-xl border border-slate-200 bg-white p-3 text-base text-slate-800",
                    r#type: "text",
                    value: "{text}",
                    oninput: move |event| text.set(event.value()),
                }
            }

            button {
                class: "rounded-2xl bg-sheffield-dark px-4 py-3 text-base font-bold text-white shadow disabled:opacity-50",
                disabled: title().trim().is_empty(),
                onclick: {
                    let date = date.clone();
                    move |_| {
                        let body = text();
                        on_action
                            .call(SchoolAction::AddExtra {
                                user_id: boy(),
                                scheduled_date: date.clone(),
                                title: title().trim().chars().take(EXTRA_TITLE_MAX).collect(),
                                category: category(),
                                text: (!body.trim().is_empty()).then(|| body.trim().to_string()),
                            });
                        title.set(String::new());
                        text.set(String::new());
                    }
                },
                "Add"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_not_dealt_out_line_is_exactly_what_the_plan_writes() {
        assert_eq!(
            not_dealt_out_line(2),
            "Not dealt out yet — finish week 2 first."
        );
    }

    #[test]
    fn an_extra_may_be_daily_reading_or_weekly_but_never_a_free_read() {
        let kinds: Vec<&str> = EXTRA_CATEGORIES
            .iter()
            .map(|(category, _)| category.as_str())
            .collect();
        assert_eq!(kinds, vec!["daily", "reading", "weekly"]);
        assert!(!kinds.contains(&Category::FreeRead.as_str()));
    }

    #[test]
    fn every_category_option_has_its_own_glyph() {
        let mut glyphs: Vec<&str> = EXTRA_CATEGORIES
            .iter()
            .map(|(category, _)| glyphs::category_glyph(category.as_str()))
            .collect();
        let before = glyphs.len();
        glyphs.sort_unstable();
        glyphs.dedup();
        assert_eq!(before, glyphs.len(), "three kinds, three distinct glyphs");
        assert!(glyphs.iter().all(|glyph| *glyph != "✅"));
    }

    #[test]
    fn the_title_cap_matches_the_databases_own_check() {
        // `lesson_extras.title` is `CHECK (length(title) BETWEEN 1 AND 80)`.
        assert_eq!(EXTRA_TITLE_MAX, 80);
    }
}
