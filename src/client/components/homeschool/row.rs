//! One row of the School tab — the anatomy H6 item 3 spells out.
//!
//! `category glyph → checkbox → subject (title register) → the week's text on
//! its own line`, with three annotations that only ever appear when they mean
//! something:
//!
//! * **`(then tell it back)`** on every `reading` row. Narration is the point
//!   of the reading, not an extra step, so it is printed on the row rather
//!   than left to a parent's memory (review finding W-5). It is a prompt, not
//!   state: there is nothing to tick.
//! * **`part 1 of 2` / `continue · 2 of 2`** when the occurrence rule split
//!   one week's reading over several days (H3 rule 5). The second and later
//!   parts say *continue* because "part 2 of 2" on its own reads like a
//!   second, separate reading.
//! * **a catch-up chip** naming the day the row was actually due — `from Mon`.
//!
//! Every colour here is a pair already in
//! [`crate::client::components::palette::PALETTE_PAIRS`]: the catch-up chip is
//! the warm hue as a **ground** under `slate-800` ink, which is the one way a
//! 3:1-ish hue can carry words at AA (review finding R-12, and QA round 1's
//! Q1-15 before it). No new pair, and no new type size.

use dioxus::prelude::*;

use crate::client::components::glyphs;
use crate::client::components::homeschool::day_sheet::EXTRA_TITLE_MAX;
use crate::shared::homeschool::{Category, LogStatus, Weekday};
use crate::shared::types::{ExtraTask, LessonOccurrence};

/// The chip that tags a row which was due earlier in the week.
///
/// The warm hue as a ground under dark ink — 4.62:1, an existing declared
/// pair. HS5 accept (g) pins both halves of it.
pub const CATCH_UP_CHIP_CLASS: &str =
    "rounded-full bg-sheffield-accent px-2 py-0.5 text-xs font-bold text-slate-800";

/// The three-letter weekday a catch-up chip names.
pub fn weekday_short(day: Weekday) -> &'static str {
    match day {
        Weekday::Mon => "Mon",
        Weekday::Tue => "Tue",
        Weekday::Wed => "Wed",
        Weekday::Thu => "Thu",
        Weekday::Fri => "Fri",
        Weekday::Sat => "Sat",
        Weekday::Sun => "Sun",
    }
}

/// H3 rule 5's `part` as the words the row prints, or `None` when the reading
/// was not split at all.
pub fn part_label(part: Option<(u32, u32)>) -> Option<String> {
    match part {
        None => None,
        Some((_, total)) if total <= 1 => None,
        Some((1, total)) => Some(format!("part 1 of {total}")),
        Some((index, total)) => Some(format!("continue · {index} of {total}")),
    }
}

/// The glyph a lesson row leads with — the subject's own where it has earned
/// one, else its category's (`glyphs::subject_glyph`, Boss pre-wave-A).
fn lesson_glyph(occurrence: &LessonOccurrence) -> &'static str {
    glyphs::subject_glyph(
        occurrence.icon_name.as_deref(),
        occurrence.category.as_str(),
    )
}

/// The checkbox, shared by lesson and extra rows.
///
/// `.stamp-check` is the rubber-stamp transition D4.4 owns in `input.css`
/// (§2.4) — this module only ever names the class.
#[component]
fn RowCheckbox(status: Option<LogStatus>, on_toggle: EventHandler<bool>) -> Element {
    let done = matches!(status, Some(LogStatus::Done));
    let skipped = matches!(status, Some(LogStatus::Skipped));
    rsx! {
        button {
            class: if done {
                "stamp-check mt-1 flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-sheffield-dark text-white"
            } else {
                "mt-1 flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border-2 border-sheffield-light"
            },
            "data-lesson-check": "true",
            aria_pressed: if status.is_some() { "true" } else { "false" },
            onclick: move |event| {
                event.stop_propagation();
                on_toggle.call(status.is_none());
            },
            if done {
                "✓"
            } else if skipped {
                "–"
            }
        }
    }
}

/// One curriculum occurrence, on the phone.
///
/// `tickable` is `false` wherever the surface is showing a week that has not
/// been dealt out (§4 default 17 — the Year view's non-current weeks): no
/// checkbox is rendered at all, because a control that always refuses is
/// worse than no control.
#[component]
pub fn LessonRow(
    occurrence: LessonOccurrence,
    #[props(default = false)] catch_up: bool,
    #[props(default = true)] tickable: bool,
    #[props(default = false)] can_edit: bool,
    /// `"2 of 3"` on a partly-done Together row (H4).
    covering: Option<String>,
    on_toggle: EventHandler<bool>,
    on_skip: EventHandler<()>,
    on_edit: EventHandler<String>,
    on_note: EventHandler<String>,
) -> Element {
    let mut editing = use_signal(|| false);
    let mut noting = use_signal(|| false);
    let mut draft = use_signal(|| occurrence.text.clone().unwrap_or_default());
    let mut note_draft = use_signal(|| occurrence.note.clone().unwrap_or_default());

    let status = occurrence.status;
    let done = matches!(status, Some(LogStatus::Done));
    let glyph = lesson_glyph(&occurrence);
    let part = part_label(occurrence.part);
    let is_reading = occurrence.category == Category::Reading;

    rsx! {
        li {
            class: if done {
                "flex w-full items-start gap-3 rounded-2xl bg-sheffield-light/25 p-4 text-left"
            } else {
                "flex w-full items-start gap-3 rounded-2xl bg-white p-4 text-left shadow-sm ring-1 ring-slate-100"
            },
            "data-lesson-row": "s{occurrence.subject_id}-a{occurrence.assignment_id.unwrap_or(0)}-{occurrence.scheduled_date}",
            span { class: "mt-1 shrink-0 text-2xl leading-none", aria_hidden: "true", "{glyph}" }

            if tickable {
                RowCheckbox { status, on_toggle }
            }

            div { class: "min-w-0 flex-1",
                div { class: "flex flex-wrap items-center gap-2",
                    span {
                        class: if done { "font-semibold text-slate-600 line-through" } else { "font-semibold" },
                        "{occurrence.title}"
                    }
                    if catch_up {
                        span { class: CATCH_UP_CHIP_CLASS,
                            "from {weekday_short(occurrence.weekday)}"
                        }
                    }
                    if let Some(covering) = covering.clone() {
                        span { class: "rounded-full bg-sheffield-sun px-2 py-0.5 text-xs font-bold text-slate-800",
                            "{covering}"
                        }
                    }
                }

                if editing() {
                    div { class: "mt-2 flex items-center gap-2",
                        input {
                            class: "w-full rounded-xl border border-slate-200 bg-white p-2 text-sm text-slate-800",
                            r#type: "text",
                            aria_label: "This week's text for {occurrence.title}",
                            value: "{draft}",
                            oninput: move |event| draft.set(event.value()),
                        }
                        button {
                            class: "shrink-0 rounded-xl bg-sheffield-dark px-3 py-2 text-sm font-bold text-white",
                            onclick: move |_| {
                                editing.set(false);
                                on_edit.call(draft());
                            },
                            "Save"
                        }
                    }
                } else if let Some(text) = occurrence.text.clone() {
                    span { class: "mt-0.5 block text-sm text-slate-600", "{text}" }
                }

                if let Some(detail) = occurrence.detail.clone() {
                    span { class: "block text-sm text-slate-600", "{detail}" }
                }

                div { class: "mt-1 flex flex-wrap items-center gap-2 text-xs text-slate-600",
                    if let Some(part) = part {
                        span { "{part}" }
                    }
                    if is_reading {
                        span { "(then tell it back)" }
                    }
                    if let Some(note) = occurrence.note.clone() {
                        span { "note: {note}" }
                    }
                }

                if can_edit {
                    div { class: "mt-2 flex gap-3 text-xs font-semibold text-sheffield-dark",
                        button {
                            onclick: move |_| {
                                let next = !editing();
                                editing.set(next);
                            },
                            "Edit text"
                        }
                        if tickable {
                            button { onclick: move |_| on_skip.call(()), "Skip" }
                            button {
                                onclick: move |_| {
                                    let next = !noting();
                                    noting.set(next);
                                },
                                "Note"
                            }
                        }
                    }
                    if noting() {
                        div { class: "mt-2 flex items-center gap-2",
                            input {
                                class: "w-full rounded-xl border border-slate-200 bg-white p-2 text-sm text-slate-800",
                                r#type: "text",
                                aria_label: "Note about {occurrence.title}",
                                value: "{note_draft}",
                                oninput: move |event| note_draft.set(event.value()),
                            }
                            button {
                                class: "shrink-0 rounded-xl bg-sheffield-dark px-3 py-2 text-sm font-bold text-white",
                                onclick: move |_| {
                                    noting.set(false);
                                    on_note.call(note_draft());
                                },
                                "Save note"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// One parent-added task (H8) on a boy's day — a `lesson_extras` row.
///
/// It leads with 📌 rather than its category glyph so a boy on the television
/// and a parent on the phone both see at a glance which rows came from the
/// curriculum and which came from mum.
///
/// H6's Month view promises "extras can be edited, deleted, ticked or skipped
/// from the same sheet". Before QA round 2 (QH2-02) the row offered a checkbox
/// and **Delete** only, so the two halves of that sentence the server had
/// already implemented — `update_extra` and `toggle_extra(status)` — had no way
/// in from the phone at all.
#[component]
pub fn ExtraRow(
    extra: ExtraTask,
    #[props(default = false)] catch_up: bool,
    #[props(default = false)] can_edit: bool,
    on_toggle: EventHandler<bool>,
    on_skip: EventHandler<()>,
    on_edit: EventHandler<String>,
    on_delete: EventHandler<()>,
) -> Element {
    let mut editing = use_signal(|| false);
    let mut draft = use_signal(|| extra.title.clone());

    let status = extra.status;
    let done = matches!(status, Some(LogStatus::Done));

    rsx! {
        li {
            class: if done {
                "flex w-full items-start gap-3 rounded-2xl bg-sheffield-light/25 p-4 text-left"
            } else {
                "flex w-full items-start gap-3 rounded-2xl bg-white p-4 text-left shadow-sm ring-1 ring-slate-100"
            },
            "data-extra-row": "{extra.id}",
            span { class: "mt-1 shrink-0 text-2xl leading-none", aria_hidden: "true",
                "{glyphs::EXTRA_TASK_GLYPH}"
            }
            RowCheckbox { status, on_toggle }
            div { class: "min-w-0 flex-1",
                div { class: "flex flex-wrap items-center gap-2",
                    span {
                        class: if done { "font-semibold text-slate-600 line-through" } else { "font-semibold" },
                        "{extra.title}"
                    }
                    if catch_up {
                        span { class: CATCH_UP_CHIP_CLASS, "from {extra.scheduled_date}" }
                    }
                }
                if let Some(text) = extra.text.clone() {
                    span { class: "mt-0.5 block text-sm text-slate-600", "{text}" }
                }
                if can_edit {
                    div { class: "mt-2 flex gap-3 text-xs font-semibold text-sheffield-dark",
                        button {
                            onclick: move |_| {
                                let next = !editing();
                                editing.set(next);
                            },
                            "Edit title"
                        }
                        button { onclick: move |_| on_skip.call(()), "Skip" }
                        button {
                            class: "text-red-700",
                            aria_label: "Delete {extra.title}",
                            onclick: move |_| on_delete.call(()),
                            "Delete"
                        }
                    }
                    if editing() {
                        div { class: "mt-2 flex items-center gap-2",
                            input {
                                class: "w-full rounded-xl border border-slate-200 bg-white p-2 text-sm text-slate-800",
                                r#type: "text",
                                maxlength: "{EXTRA_TITLE_MAX}",
                                aria_label: "Title of {extra.title}",
                                value: "{draft}",
                                oninput: move |event| draft.set(event.value()),
                            }
                            button {
                                class: "shrink-0 rounded-xl bg-sheffield-dark px-3 py-2 text-sm font-bold text-white",
                                onclick: move |_| {
                                    editing.set(false);
                                    on_edit.call(draft());
                                },
                                "Save"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_split_reading_says_part_one_then_continue() {
        assert_eq!(part_label(Some((1, 2))).as_deref(), Some("part 1 of 2"));
        assert_eq!(
            part_label(Some((2, 2))).as_deref(),
            Some("continue · 2 of 2")
        );
        assert_eq!(
            part_label(Some((3, 3))).as_deref(),
            Some("continue · 3 of 3")
        );
    }

    #[test]
    fn an_unsplit_reading_prints_no_part_label_at_all() {
        // H3 rule 5 leaves `part` at `None` when a row owns its day outright,
        // and a one-day "group" must not print `part 1 of 1` either.
        assert_eq!(part_label(None), None);
        assert_eq!(part_label(Some((1, 1))), None);
    }

    #[test]
    fn the_catch_up_chip_is_the_warm_hue_as_a_ground_under_dark_ink() {
        // HS5 accept (g): both halves of the declared pair, never the hue as
        // ink (3.11:1 on paper — QA round 1, Q1-15).
        assert!(CATCH_UP_CHIP_CLASS.contains("bg-sheffield-accent"));
        assert!(CATCH_UP_CHIP_CLASS.contains("text-slate-800"));
    }

    #[test]
    fn every_weekday_has_a_three_letter_name() {
        for day in Weekday::ORDER {
            assert_eq!(weekday_short(day).len(), 3, "{day:?}");
        }
    }
}
