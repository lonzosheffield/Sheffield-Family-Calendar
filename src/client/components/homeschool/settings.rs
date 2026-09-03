//! **School settings** — the sheet behind the header chip (H6).
//!
//! Four things live here, in the order a parent needs them: who is enrolled
//! and on what, **Pause school** for the summer (H2's `paused`, which empties
//! every surface without touching a single log row), the per-subject days and
//! `shared` toggles, and the enrollment form itself.
//!
//! Every control on this sheet is a parent action the server enforces with
//! the session cookie (H7), so the sheet renders its controls only for a
//! parent and says why when it does not.

use dioxus::prelude::*;

use crate::client::components::homeschool::enroll::EnrollForm;
use crate::client::components::homeschool::SchoolAction;
use crate::client::components::mobile::session;
use crate::shared::types::{
    profile_name, CurriculumSummary, EnrollmentView, SubjectSetting, FAMILY_PROFILE_COUNT,
};

/// The bottom sheet the whole School tab opens its dialogs in.
///
/// A sheet rather than a route: the phone's tab bar is fixed to the bottom of
/// the viewport and a full-page navigation would take the parent out of Today
/// to change one subject's days.
pub const SHEET_SCRIM_CLASS: &str = "fixed inset-0 z-40 flex items-end justify-center bg-black/50";
/// The sheet's own card.
pub const SHEET_CARD_CLASS: &str =
    "max-h-[85vh] w-full max-w-md overflow-y-auto rounded-t-3xl bg-white p-5 shadow-2xl";

#[component]
pub fn SchoolSettingsSheet(
    enrollments: Vec<EnrollmentView>,
    curricula: Vec<CurriculumSummary>,
    subjects: Vec<SubjectSetting>,
    on_action: EventHandler<SchoolAction>,
    on_close: EventHandler<()>,
) -> Element {
    let parent = session::is_parent();

    rsx! {
        div { class: SHEET_SCRIM_CLASS, role: "dialog", aria_modal: "true",
            aria_label: "School settings",
            div { class: SHEET_CARD_CLASS, "data-school-settings": "true",
                div { class: "flex items-center justify-between",
                    h2 { class: "text-xl font-bold text-sheffield-dark", "School settings" }
                    button {
                        class: "rounded-xl px-3 py-2 text-sm font-semibold text-slate-600",
                        onclick: move |_| on_close.call(()),
                        "Close"
                    }
                }

                div { class: "mt-4 flex flex-col gap-3",
                    h3 { class: "text-lg font-bold text-sheffield-dark", "Who is enrolled" }
                    if enrollments.iter().all(|row| !row.enrolled) {
                        p { class: "text-sm text-slate-600", "Nobody is enrolled yet." }
                    }
                    for row in enrollments.clone().into_iter().filter(|row| row.enrolled) {
                        EnrollmentCard { key: "{row.user_id}", row, parent, on_action }
                    }
                }

                div { class: "mt-6",
                    EnrollForm {
                        curricula: curricula.clone(),
                        enrollments: enrollments.clone(),
                        on_action,
                    }
                }

                if !subjects.is_empty() {
                    div { class: "mt-6 flex flex-col gap-3",
                        h3 { class: "text-lg font-bold text-sheffield-dark", "Subjects" }
                        for subject in subjects.clone() {
                            SubjectCard { key: "{subject.subject_id}", subject, parent, on_action }
                        }
                    }
                }
            }
        }
    }
}

/// One boy's enrollment, with **Pause school** and **Unenroll**.
#[component]
fn EnrollmentCard(
    row: EnrollmentView,
    parent: bool,
    on_action: EventHandler<SchoolAction>,
) -> Element {
    let user_id = row.user_id;
    let paused = row.paused;
    let name = profile_name(clamp_profile(user_id));

    rsx! {
        div { class: "rounded-2xl bg-white p-4 shadow-sm ring-1 ring-slate-100",
            "data-school-enrollment": "{user_id}",
            p { class: "font-semibold text-slate-800", "{name} · {row.curriculum_name}" }
            p { class: "text-sm text-slate-600",
                "Week {row.current_week} of {row.weeks} · school days {row.school_days} · started {row.week_started_on}"
            }
            if parent {
                div { class: "mt-3 flex flex-wrap gap-2",
                    button {
                        class: "rounded-xl bg-white px-3 py-2 text-sm font-bold text-sheffield-dark ring-1 ring-slate-200",
                        onclick: move |_| {
                            on_action.call(SchoolAction::SetPaused { user_id, paused: !paused })
                        },
                        if paused {
                            "Start school again"
                        } else {
                            "Pause school"
                        }
                    }
                    button {
                        class: "rounded-xl px-3 py-2 text-sm font-bold text-red-700",
                        onclick: move |_| on_action.call(SchoolAction::Unenroll { user_id }),
                        "Unenroll"
                    }
                }
            }
        }
    }
}

/// `profile_name` takes the rail's 1-based `u32`; an enrollment carries the
/// database id. They are the same numbers for the family's four boys, but a
/// row that somehow names a wider id must not panic the settings sheet.
fn clamp_profile(user_id: i64) -> u32 {
    u32::try_from(user_id)
        .ok()
        .filter(|candidate| (1..=FAMILY_PROFILE_COUNT).contains(candidate))
        .unwrap_or(1)
}

/// One subject's days and `shared` flag (H6, "per-subject days + shared
/// toggles").
#[component]
fn SubjectCard(
    subject: SubjectSetting,
    parent: bool,
    on_action: EventHandler<SchoolAction>,
) -> Element {
    let subject_id = subject.subject_id;
    let shared = subject.shared;
    let mut days = use_signal(|| subject.days.clone());

    rsx! {
        div { class: "rounded-2xl bg-white p-4 shadow-sm ring-1 ring-slate-100",
            "data-school-subject": "{subject_id}",
            p { class: "font-semibold text-slate-800", "{subject.name}" }
            p { class: "text-sm text-slate-600", "{subject.category.as_str()}" }
            if parent {
                div { class: "mt-2 flex flex-wrap items-center gap-2",
                    input {
                        class: "w-28 rounded-xl border border-slate-200 bg-white p-2 text-sm text-slate-800",
                        r#type: "text",
                        aria_label: "School days for {subject.name}",
                        value: "{days}",
                        oninput: move |event| days.set(event.value().to_uppercase()),
                    }
                    button {
                        class: "rounded-xl bg-sheffield-dark px-3 py-2 text-sm font-bold text-white",
                        onclick: move |_| {
                            on_action
                                .call(SchoolAction::SetSubjectSchedule {
                                    subject_id,
                                    days: days(),
                                    shared,
                                })
                        },
                        "Save days"
                    }
                    button {
                        class: "rounded-xl bg-white px-3 py-2 text-sm font-bold text-sheffield-dark ring-1 ring-slate-200",
                        onclick: move |_| {
                            on_action
                                .call(SchoolAction::SetSubjectSchedule {
                                    subject_id,
                                    days: days(),
                                    shared: !shared,
                                })
                        },
                        if shared {
                            "Read together"
                        } else {
                            "Each boy alone"
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
    fn a_profile_id_outside_the_rail_never_panics_the_sheet() {
        assert_eq!(clamp_profile(1), 1);
        assert_eq!(
            clamp_profile(i64::from(FAMILY_PROFILE_COUNT)),
            FAMILY_PROFILE_COUNT
        );
        assert_eq!(clamp_profile(0), 1);
        assert_eq!(clamp_profile(-3), 1);
        assert_eq!(clamp_profile(i64::MAX), 1);
    }

    #[test]
    fn the_sheet_is_a_modal_over_a_scrim_the_palette_already_declares() {
        assert!(SHEET_SCRIM_CLASS.contains("bg-black/50"));
        assert!(SHEET_CARD_CLASS.contains("bg-white"));
    }
}
