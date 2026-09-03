//! The empty state and the enrollment form (H6, "School settings").
//!
//! Enrollment is the one School action with no sensible default: which boy,
//! which curriculum, which week he is starting on and which days count as
//! school days are all facts only a parent holds. So the empty state is a
//! poster card with one button rather than a silent blank tab, and the form
//! itself lives inside the settings sheet, behind the parent session the
//! server enforces anyway (H7).

use dioxus::prelude::*;

use crate::client::components::glyphs;
use crate::client::components::homeschool::SchoolAction;
use crate::client::components::mobile::session;
use crate::shared::types::{profile_name, CurriculumSummary, EnrollmentView, FAMILY_PROFILE_COUNT};

/// The default school week (§4 default 2) — Monday to Friday.
pub const DEFAULT_SCHOOL_DAYS: &str = "MTWRF";

/// Nobody is enrolled: the whole tab is this card (H6, "Empty state").
#[component]
pub fn NoSchoolPlan(on_enroll: EventHandler<()>) -> Element {
    rsx! {
        div { class: "flex flex-col items-center gap-3 rounded-2xl bg-white p-6 text-center shadow-sm ring-1 ring-slate-100",
            p { class: "text-4xl", aria_hidden: "true", "{glyphs::HOMESCHOOL_GLYPH}" }
            h3 { class: "text-lg font-bold text-sheffield-dark", "No school plan yet" }
            p { class: "text-sm text-slate-600",
                "Pick a boy and a curriculum and the day's work will be waiting here every morning."
            }
            button {
                class: "rounded-2xl bg-sheffield-dark px-4 py-3 text-base font-bold text-white shadow",
                onclick: move |_| on_enroll.call(()),
                "Enroll a boy"
            }
        }
    }
}

/// Boy → curriculum → starting week → school days, then **Enroll**.
///
/// Rendered only for a parent: the form is hidden for a signed-out phone
/// rather than shown and rejected, because a control that always refuses is
/// worse than no control. The server checks the cookie regardless (H7).
#[component]
pub fn EnrollForm(
    curricula: Vec<CurriculumSummary>,
    enrollments: Vec<EnrollmentView>,
    on_action: EventHandler<SchoolAction>,
) -> Element {
    let mut user_id = use_signal(|| 1i64);
    let mut curriculum_id = use_signal(|| curricula.first().map(|row| row.id).unwrap_or_default());
    let mut week = use_signal(|| 1i64);
    let mut school_days = use_signal(|| DEFAULT_SCHOOL_DAYS.to_string());

    if !session::is_parent() {
        return rsx! {
            p { class: "rounded-2xl bg-sheffield-sun/20 p-4 text-sm font-semibold text-slate-800",
                "Sign in with the parent PIN under Settings to change who is enrolled."
            }
        };
    }

    rsx! {
        div { class: "flex flex-col gap-3", "data-school-enroll": "true",
            h3 { class: "text-lg font-bold text-sheffield-dark", "Enroll a boy" }

            label { class: "block text-sm font-semibold text-slate-600",
                "Boy"
                select {
                    class: "mt-1 w-full rounded-xl border border-slate-200 bg-white p-3 text-base text-slate-800",
                    value: "{user_id}",
                    onchange: move |event| {
                        if let Ok(parsed) = event.value().parse::<i64>() {
                            user_id.set(parsed);
                        }
                    },
                    for candidate in 1..=FAMILY_PROFILE_COUNT {
                        option { key: "{candidate}", value: "{candidate}", "{profile_name(candidate)}" }
                    }
                }
            }

            label { class: "block text-sm font-semibold text-slate-600",
                "Curriculum"
                select {
                    class: "mt-1 w-full rounded-xl border border-slate-200 bg-white p-3 text-base text-slate-800",
                    value: "{curriculum_id}",
                    onchange: move |event| {
                        if let Ok(parsed) = event.value().parse::<i64>() {
                            curriculum_id.set(parsed);
                        }
                    },
                    for row in curricula.clone() {
                        option { key: "{row.id}", value: "{row.id}",
                            "{row.name} · {row.weeks} weeks"
                        }
                    }
                }
            }

            label { class: "block text-sm font-semibold text-slate-600",
                "Starting week"
                input {
                    class: "mt-1 w-full rounded-xl border border-slate-200 bg-white p-3 text-base text-slate-800",
                    r#type: "number",
                    min: "1",
                    value: "{week}",
                    oninput: move |event| {
                        if let Ok(parsed) = event.value().parse::<i64>() {
                            week.set(parsed.max(1));
                        }
                    },
                }
            }

            label { class: "block text-sm font-semibold text-slate-600",
                "School days"
                input {
                    class: "mt-1 w-full rounded-xl border border-slate-200 bg-white p-3 text-base text-slate-800",
                    r#type: "text",
                    value: "{school_days}",
                    oninput: move |event| school_days.set(event.value().to_uppercase()),
                }
                span { class: "mt-1 block text-xs text-slate-600",
                    "Letters M T W R F S U — R is Thursday, U is Sunday."
                }
            }

            button {
                class: "rounded-2xl bg-sheffield-dark px-4 py-3 text-base font-bold text-white shadow disabled:opacity-50",
                disabled: curriculum_id() == 0 || school_days().trim().is_empty(),
                onclick: move |_| {
                    on_action
                        .call(SchoolAction::Enroll {
                            user_id: user_id(),
                            curriculum_id: curriculum_id(),
                            week: week(),
                            school_days: school_days(),
                        })
                },
                "Enroll"
            }

            if !enrollments.iter().any(|row| row.enrolled) {
                p { class: "text-sm text-slate-600", "Nobody is enrolled yet." }
            }
        }
    }
}
