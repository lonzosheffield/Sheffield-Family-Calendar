//! **Today is the tab** (H6): no segmented control, no "this week" landing
//! page — a parent opening the phone in the morning gets one list of exactly
//! what to do today, in reading order.
//!
//! ```text
//! Week 2 of 3 · Term 2 · 3 done · 1 skipped / 11      <- header chip -> settings
//! Week 2 done — start week 3?              [Finish week →]   <- nudge (H2)
//! Together            the shared read-alouds, once, for everyone
//! Isaiah              his own work            [Mark all done]
//! Nathaniel           …
//! This term           geography · poetry · free reads (collapsed)
//! ```
//!
//! Everything here is **presentational**: it takes a
//! [`HomeschoolTodayView`] and an [`SchoolAction`] sink and renders. The
//! fetching, the dates and the server calls live one level up in
//! [`super::School`], which is what lets the whole surface be rendered by
//! `dioxus::ssr::render_element` in a test with no server and no database
//! (HS5 accept (b), (c)) — the same split `RoutineRow` already uses.
//!
//! Section headings reuse the phone's existing
//! `h3 text-lg font-bold text-sheffield-dark` pattern; no new type size and no
//! new palette pair appear on this surface.

use std::collections::BTreeMap;

use dioxus::prelude::*;

use crate::client::components::glyphs;
use crate::client::components::homeschool::enroll::NoSchoolPlan;
use crate::client::components::homeschool::row::{ExtraRow, LessonRow};
use crate::client::components::homeschool::SchoolAction;
use crate::client::components::mobile::session;
use crate::shared::homeschool::{LogStatus, TermNoteKind};
use crate::shared::types::{
    BoyToday, DayItem, HomeschoolTodayView, LessonOccurrence, TogetherGroup, TogetherOccurrence,
};

/// The phone's section heading, unchanged from the Routine tab's own.
pub const SECTION_HEADING_CLASS: &str = "text-lg font-bold text-sheffield-dark";

/// A boy chip the surface is currently filtered to.
pub const CHIP_ON_CLASS: &str =
    "rounded-full bg-sheffield-dark px-3 py-2 text-sm font-bold text-white";
/// A boy chip that is merely on offer.
pub const CHIP_OFF_CLASS: &str =
    "rounded-full bg-white px-3 py-2 text-sm font-semibold text-sheffield-dark ring-1 ring-slate-200";

/// The boy chips (H6). **One chip strip, three panes.**
///
/// Today lets a parent stand back to **Everyone**; Year is filtered by the
/// same chip ("a boy chip filters Year exactly as it filters Today"), and
/// Month "always shows exactly one boy … the chip is a required selector"
/// (§4 default 17 / D-4). `allow_everyone` is the whole difference, which is
/// why the markup lives here once rather than three times — before QA round 1
/// (QH1-05) it lived only inside `TodayPanel`, so the second boy's Year and
/// Month were reachable only by going back to Today and toggling.
#[component]
pub fn BoyChips(
    boys: Vec<(i64, String)>,
    /// The boy the surface is on, or `None` for Everyone (Today only).
    selected: Option<i64>,
    allow_everyone: bool,
    on_select: EventHandler<Option<i64>>,
) -> Element {
    rsx! {
        div {
            class: "flex flex-wrap gap-2",
            role: "group",
            aria_label: "Whose school",
            "data-boy-chips": "true",
            if allow_everyone {
                button {
                    class: if selected.is_none() { CHIP_ON_CLASS } else { CHIP_OFF_CLASS },
                    "data-boy-chip": "everyone",
                    aria_pressed: if selected.is_none() { "true" } else { "false" },
                    onclick: move |_| on_select.call(None),
                    "Everyone"
                }
            }
            for (user_id , name) in boys.clone() {
                button {
                    key: "{user_id}",
                    class: if selected == Some(user_id) { CHIP_ON_CLASS } else { CHIP_OFF_CLASS },
                    "data-boy-chip": "{user_id}",
                    aria_pressed: if selected == Some(user_id) { "true" } else { "false" },
                    onclick: move |_| on_select.call(Some(user_id)),
                    "{name}"
                }
            }
        }
    }
}

/// Every day item of one group, oldest first — the order the occurrence rule
/// itself produces.
fn group_items(group: &TogetherGroup) -> Vec<&DayItem> {
    let mut items: Vec<&DayItem> = group
        .boys
        .iter()
        .flat_map(|boy| boy.done.iter().chain(&boy.catch_up).chain(&boy.due_today))
        .collect();
    items.sort_by(|a, b| item_date(a).cmp(item_date(b)));
    items
}

fn item_date(item: &DayItem) -> &str {
    match item {
        DayItem::Lesson(lesson) => &lesson.scheduled_date,
        DayItem::Extra(extra) => &extra.scheduled_date,
    }
}

/// Recover each assignment row's `ordinal` from the occurrences on screen.
///
/// `LessonOccurrence` carries the row's **id**, not its ordinal, and
/// `upsert_assignment` is keyed on `(subject, week, ordinal)`. H3 rule 2 deals
/// a subject's rows out in ordinal order and `occurrences()` sorts by date, so
/// the rank of an assignment id in date order *is* its ordinal — for every row
/// the parent can actually see. A row further into the week that has not been
/// dealt out yet is simply not offered an edit here; the Year view's cell
/// sheet, which holds the whole week at once, edits it exactly.
fn assignment_ordinals(group: &TogetherGroup) -> BTreeMap<(i64, i64), i64> {
    let mut seen: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    let mut lessons: Vec<&LessonOccurrence> =
        group.together.iter().map(|slot| &slot.occurrence).collect();
    for item in group_items(group) {
        if let DayItem::Lesson(lesson) = item {
            lessons.push(lesson);
        }
    }
    lessons.sort_by(|a, b| a.scheduled_date.cmp(&b.scheduled_date));

    let mut out = BTreeMap::new();
    for lesson in lessons {
        let Some(assignment_id) = lesson.assignment_id else {
            continue;
        };
        let rows = seen.entry(lesson.subject_id).or_default();
        if !rows.contains(&assignment_id) {
            rows.push(assignment_id);
        }
        let ordinal = rows
            .iter()
            .position(|candidate| *candidate == assignment_id)
            .unwrap_or(0) as i64
            + 1;
        out.insert((lesson.subject_id, assignment_id), ordinal);
    }
    out
}

/// H2's nudge line, or `None` when the week needs no nudging.
pub fn nudge_line(group: &TogetherGroup) -> Option<String> {
    if group.paused || group.year_complete {
        return None;
    }
    if group.can_finish_week {
        return Some(format!(
            "Week {} done — start week {}?",
            group.week,
            group.week + 1
        ));
    }
    if group.days_on_week >= 14 {
        return Some(format!(
            "You've been on week {} for {} days",
            group.week, group.days_on_week
        ));
    }
    None
}

/// The header chip's words (H6 item 1).
pub fn header_chip_text(group: &TogetherGroup) -> String {
    let done: u32 = group.boys.iter().map(|boy| boy.done_count).sum();
    let skipped: u32 = group.boys.iter().map(|boy| boy.skipped_count).sum();
    let total: u32 = group.boys.iter().map(|boy| boy.total_count).sum();
    format!(
        "Week {} of {} · Term {} · {done} done · {skipped} skipped / {total}",
        group.week, group.weeks, group.term
    )
}

/// The Today pane.
#[component]
pub fn TodayPanel(
    view: HomeschoolTodayView,
    /// `Some(user_id)` when a boy chip is filtering the surface to one boy.
    boy_filter: Option<i64>,
    on_boy_filter: EventHandler<Option<i64>>,
    on_action: EventHandler<SchoolAction>,
) -> Element {
    if !view.anyone_enrolled {
        return rsx! {
            NoSchoolPlan { on_enroll: move |()| on_action.call(SchoolAction::OpenSettings) }
        };
    }

    let chips: Vec<(i64, String)> = view
        .groups
        .iter()
        .flat_map(|group| group.boys.iter())
        .map(|boy| (boy.user_id, boy.name.clone()))
        .collect();

    rsx! {
        div { class: "flex flex-col gap-4",
            if chips.len() > 1 {
                BoyChips {
                    boys: chips.clone(),
                    selected: boy_filter,
                    allow_everyone: true,
                    on_select: on_boy_filter,
                }
            }

            for group in view.groups.clone() {
                GroupBlock {
                    key: "{group.curriculum_id}-{group.week}",
                    group,
                    date: view.date.clone(),
                    is_school_day: view.is_school_day,
                    boy_filter,
                    on_action,
                }
            }
        }
    }
}

/// One Together group — every boy on the same curriculum and the same week
/// (H4). With one boy enrolled this is simply his list.
#[component]
fn GroupBlock(
    group: TogetherGroup,
    /// The date the whole view is for — what makes a Together row "from Mon".
    date: String,
    is_school_day: bool,
    boy_filter: Option<i64>,
    on_action: EventHandler<SchoolAction>,
) -> Element {
    let parent = session::is_parent();
    let ordinals = assignment_ordinals(&group);
    let week = group.week;
    let curriculum_id = group.curriculum_id;
    let user_ids: Vec<i64> = group.boys.iter().map(|boy| boy.user_id).collect();
    let nudge = nudge_line(&group);

    let boys: Vec<BoyToday> = group
        .boys
        .iter()
        .filter(|boy| boy_filter.is_none_or(|wanted| boy.user_id == wanted))
        .cloned()
        .collect();
    let together: Vec<TogetherOccurrence> = group
        .together
        .iter()
        .filter(|slot| boy_filter.is_none_or(|wanted| slot.user_ids.contains(&wanted)))
        .cloned()
        .collect();

    rsx! {
        section { class: "flex flex-col gap-4", "data-school-group": "{curriculum_id}-{week}",
            button {
                class: "self-start rounded-full bg-sheffield-sun px-3 py-2 text-left text-sm font-bold text-slate-800",
                "data-school-chip": "true",
                onclick: move |_| on_action.call(SchoolAction::OpenSettings),
                "{header_chip_text(&group)}"
            }

            if group.paused {
                StateCard {
                    glyph: "⚽",
                    title: "School's out".to_string(),
                    body: "No school today — school is paused until a parent turns it back on."
                        .to_string(),
                }
            } else if group.year_complete {
                StateCard {
                    glyph: glyphs::YEAR_COMPLETE_GLYPH,
                    title: "Year complete".to_string(),
                    body: "Every week of the plan is finished.".to_string(),
                }
            } else {
                if let Some(nudge) = nudge.clone() {
                    div { class: "flex flex-wrap items-center gap-3 rounded-2xl bg-sheffield-sun/30 p-4 text-sm font-semibold text-slate-800",
                        role: "status",
                        span { class: "flex-1", "{nudge}" }
                        if parent {
                            if group.can_finish_week {
                                button {
                                    class: "rounded-xl bg-sheffield-dark px-3 py-2 text-sm font-bold text-white",
                                    onclick: {
                                        let user_ids = user_ids.clone();
                                        move |_| {
                                            on_action
                                                .call(SchoolAction::SetWeek {
                                                    user_ids: user_ids.clone(),
                                                    week: week + 1,
                                                })
                                        }
                                    },
                                    "Finish week →"
                                }
                            }
                            if week > 1 {
                                button {
                                    class: "rounded-xl bg-white px-3 py-2 text-sm font-bold text-sheffield-dark ring-1 ring-slate-200",
                                    onclick: {
                                        let user_ids = user_ids.clone();
                                        move |_| {
                                            on_action
                                                .call(SchoolAction::SetWeek {
                                                    user_ids: user_ids.clone(),
                                                    week: week - 1,
                                                })
                                        }
                                    },
                                    "Back a week"
                                }
                            }
                        }
                    }
                } else if parent && week > 1 {
                    button {
                        class: "self-start text-sm font-semibold text-sheffield-dark",
                        onclick: {
                            let user_ids = user_ids.clone();
                            move |_| {
                                on_action
                                    .call(SchoolAction::SetWeek {
                                        user_ids: user_ids.clone(),
                                        week: week - 1,
                                    })
                            }
                        },
                        "Back a week"
                    }
                }

                if !is_school_day {
                    p { class: "rounded-2xl bg-sheffield-light/25 p-4 text-sm font-semibold text-slate-800",
                        "Not a school day — anything below is catch-up."
                    }
                }

                if !together.is_empty() {
                    div { class: "flex flex-col gap-2",
                        h3 { class: SECTION_HEADING_CLASS, "Together" }
                        ul { class: "flex flex-col gap-2",
                            for slot in together.clone() {
                                TogetherRow {
                                    key: "s{slot.occurrence.subject_id}-a{slot.occurrence.assignment_id.unwrap_or(0)}-{slot.occurrence.scheduled_date}",
                                    slot,
                                    curriculum_id,
                                    week,
                                    date: date.clone(),
                                    parent,
                                    ordinals: ordinals.clone(),
                                    on_action,
                                }
                            }
                        }
                    }
                }

                for boy in boys.clone() {
                    BoyBlock {
                        key: "{boy.user_id}",
                        boy,
                        week,
                        parent,
                        ordinals: ordinals.clone(),
                        on_action,
                    }
                }

                TermCard { group: group.clone() }
            }
        }
    }
}

/// One shared read-aloud, rendered **once** with the boys it covers (H4).
///
/// Ticking it is parent-only (§4 default 18): the fan-out needs the group
/// membership only the server holds, which is also why it is the one mutation
/// the offline queue refuses to carry.
#[component]
fn TogetherRow(
    slot: TogetherOccurrence,
    curriculum_id: i64,
    week: i64,
    date: String,
    parent: bool,
    ordinals: BTreeMap<(i64, i64), i64>,
    on_action: EventHandler<SchoolAction>,
) -> Element {
    let occurrence = slot.occurrence.clone();
    let subject_id = occurrence.subject_id;
    let assignment_id = occurrence.assignment_id;
    let scheduled_date = occurrence.scheduled_date.clone();
    // A shared row still on screen after its own day is the group's catch-up.
    let catch_up = occurrence.scheduled_date.as_str() < date.as_str();
    // H4: a partly-done shared row shows "2 of 3" rather than a tick.
    let covering = (slot.user_ids.len() > 1 && slot.done_user_ids.len() < slot.user_ids.len())
        .then(|| format!("{} of {}", slot.done_user_ids.len(), slot.user_ids.len()));
    let edit_ordinal = edit_ordinal_for(&ordinals, subject_id, assignment_id);

    rsx! {
        LessonRow {
            occurrence: occurrence.clone(),
            catch_up,
            tickable: parent,
            can_edit: parent && edit_ordinal.is_some(),
            covering,
            on_toggle: move |completed: bool| {
                on_action
                    .call(SchoolAction::ToggleTogether {
                        curriculum_id,
                        week,
                        subject_id,
                        assignment_id,
                        scheduled_date: scheduled_date.clone(),
                        completed,
                    })
            },
            on_skip: move |()| {},
            on_edit: move |text: String| {
                if let Some(ordinal) = edit_ordinal {
                    on_action
                        .call(SchoolAction::EditAssignment {
                            subject_id,
                            week,
                            ordinal,
                            text,
                        });
                }
            },
            on_note: move |_: String| {},
        }
    }
}

fn edit_ordinal_for(
    ordinals: &BTreeMap<(i64, i64), i64>,
    subject_id: i64,
    assignment_id: Option<i64>,
) -> Option<i64> {
    match assignment_id {
        // H6 item 6's named case: a daily subject with no row this week —
        // "Math: lesson 14" — where the edit *creates* ordinal 1.
        None => Some(1),
        Some(assignment_id) => ordinals.get(&(subject_id, assignment_id)).copied(),
    }
}

/// One boy's own (non-shared) work, under his name.
#[component]
fn BoyBlock(
    boy: BoyToday,
    week: i64,
    parent: bool,
    ordinals: BTreeMap<(i64, i64), i64>,
    on_action: EventHandler<SchoolAction>,
) -> Element {
    let user_id = boy.user_id;
    // H4: a `shared` occurrence belongs to **Together**, where it renders once
    // for everyone it covers. `today_view` hands back every occurrence the boy
    // has — shared and not — so the split happens here rather than being
    // baked into the pure scheduling core, which has no idea what a section
    // heading is. Extras are never shared and always render under the boy.
    let due_today = own_work(&boy.due_today);
    let catch_up = own_work(&boy.catch_up);

    rsx! {
        div { class: "flex flex-col gap-2", "data-school-boy": "{user_id}",
            div { class: "flex items-center justify-between gap-3",
                h3 { class: SECTION_HEADING_CLASS, "{boy.name}" }
                if parent {
                    button {
                        class: "rounded-xl bg-white px-3 py-2 text-sm font-bold text-sheffield-dark ring-1 ring-slate-200",
                        onclick: move |_| on_action.call(SchoolAction::MarkAllDone { user_id, week }),
                        "Mark all done"
                    }
                }
            }
            if due_today.is_empty() && catch_up.is_empty() {
                p { class: "rounded-2xl bg-white p-4 text-sm text-slate-600 ring-1 ring-slate-100",
                    "Nothing left for {boy.name} today."
                }
            } else {
                ul { class: "flex flex-col gap-2",
                    for item in due_today.clone() {
                        DayItemRow {
                            key: "due-{item_key(&item)}",
                            item,
                            user_id,
                            week,
                            parent,
                            catch_up: false,
                            ordinals: ordinals.clone(),
                            on_action,
                        }
                    }
                    for item in catch_up.clone() {
                        DayItemRow {
                            key: "late-{item_key(&item)}",
                            item,
                            user_id,
                            week,
                            parent,
                            catch_up: true,
                            ordinals: ordinals.clone(),
                            on_action,
                        }
                    }
                }
            }
        }
    }
}

/// A boy's own work: everything except the shared occurrences **Together**
/// already renders once for the whole group (H4).
pub fn own_work(items: &[DayItem]) -> Vec<DayItem> {
    items
        .iter()
        .filter(|item| match item {
            DayItem::Lesson(lesson) => !lesson.shared,
            DayItem::Extra(_) => true,
        })
        .cloned()
        .collect()
}

fn item_key(item: &DayItem) -> String {
    match item {
        DayItem::Lesson(lesson) => format!(
            "s{}-a{}-{}",
            lesson.subject_id,
            lesson.assignment_id.unwrap_or(0),
            lesson.scheduled_date
        ),
        DayItem::Extra(extra) => format!("x{}", extra.id),
    }
}

/// One row of a boy's day: a curriculum occurrence or one of his parent-added
/// tasks (D-2 — an extra is never a `LessonOccurrence`).
#[component]
pub fn DayItemRow(
    item: DayItem,
    user_id: i64,
    week: i64,
    parent: bool,
    catch_up: bool,
    ordinals: BTreeMap<(i64, i64), i64>,
    on_action: EventHandler<SchoolAction>,
) -> Element {
    match item {
        DayItem::Lesson(occurrence) => {
            let subject_id = occurrence.subject_id;
            let assignment_id = occurrence.assignment_id;
            let scheduled_date = occurrence.scheduled_date.clone();
            let skip_date = scheduled_date.clone();
            let note_date = scheduled_date.clone();
            let note_status = occurrence.status.unwrap_or(LogStatus::Done);
            let edit_ordinal = edit_ordinal_for(&ordinals, subject_id, assignment_id);
            rsx! {
                LessonRow {
                    occurrence: occurrence.clone(),
                    catch_up,
                    tickable: true,
                    can_edit: parent && edit_ordinal.is_some(),
                    on_toggle: move |completed: bool| {
                        on_action
                            .call(SchoolAction::ToggleLesson {
                                user_id,
                                week,
                                subject_id,
                                assignment_id,
                                scheduled_date: scheduled_date.clone(),
                                completed,
                                status: LogStatus::Done,
                                note: None,
                            })
                    },
                    on_skip: move |()| {
                        on_action
                            .call(SchoolAction::ToggleLesson {
                                user_id,
                                week,
                                subject_id,
                                assignment_id,
                                scheduled_date: skip_date.clone(),
                                completed: true,
                                status: LogStatus::Skipped,
                                note: None,
                            })
                    },
                    on_edit: move |text: String| {
                        if let Some(ordinal) = edit_ordinal {
                            on_action
                                .call(SchoolAction::EditAssignment {
                                    subject_id,
                                    week,
                                    ordinal,
                                    text,
                                });
                        }
                    },
                    on_note: move |note: String| {
                        // A note lives on the log row, so writing one is the
                        // same mutation as ticking — and it keeps whatever
                        // status the row already had rather than quietly
                        // un-skipping it.
                        on_action
                            .call(SchoolAction::ToggleLesson {
                                user_id,
                                week,
                                subject_id,
                                assignment_id,
                                scheduled_date: note_date.clone(),
                                completed: true,
                                status: note_status,
                                note: (!note.trim().is_empty()).then(|| note.trim().to_string()),
                            })
                    },
                }
            }
        }
        DayItem::Extra(extra) => {
            let extra_id = extra.id;
            rsx! {
                ExtraRow {
                    extra: extra.clone(),
                    catch_up,
                    can_edit: parent,
                    on_toggle: move |completed: bool| {
                        on_action
                            .call(SchoolAction::ToggleExtra {
                                user_id,
                                extra_id,
                                completed,
                                status: LogStatus::Done,
                            })
                    },
                    on_delete: move |()| on_action.call(SchoolAction::DeleteExtra { extra_id }),
                }
            }
        }
    }
}

/// The collapsed **This term** card: the geography concept, the term's poetry
/// book and the free reads (§4 default 12 — reference only, never an
/// occurrence).
#[component]
fn TermCard(group: TogetherGroup) -> Element {
    rsx! {
        if !group.term_notes.is_empty() {
            details { class: "rounded-2xl bg-white p-4 shadow-sm ring-1 ring-slate-100",
                summary { class: SECTION_HEADING_CLASS, "This term" }
                ul { class: "mt-2 flex flex-col gap-1 text-sm text-slate-600",
                    for note in group.term_notes.clone() {
                        li { key: "{note.id}",
                            span { class: "mr-2 font-semibold text-slate-800",
                                "{term_note_label(note.kind)}"
                            }
                            "{note.text}"
                        }
                    }
                }
            }
        }
    }
}

fn term_note_label(kind: TermNoteKind) -> &'static str {
    match kind {
        TermNoteKind::Geography => "Geography",
        TermNoteKind::FreeRead => "Free read",
        TermNoteKind::Poetry => "Poetry",
    }
}

/// The three whole-surface states H2 names: paused, year complete, and the
/// empty state `enroll.rs` owns.
#[component]
fn StateCard(glyph: String, title: String, body: String) -> Element {
    rsx! {
        div { class: "rounded-2xl bg-white p-6 text-center shadow-sm ring-1 ring-slate-100",
            p { class: "text-4xl", aria_hidden: "true", "{glyph}" }
            p { class: "mt-2 text-lg font-bold text-sheffield-dark", "{title}" }
            p { class: "mt-1 text-sm text-slate-600", "{body}" }
        }
    }
}

/// The tab's own glyph, re-exported so the shell header can lead with it
/// without importing the glyph module twice.
pub const TAB_GLYPH: &str = glyphs::HOMESCHOOL_GLYPH;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::homeschool::TermNote;

    fn group(week: i64, can_finish_week: bool, days_on_week: u32) -> TogetherGroup {
        TogetherGroup {
            curriculum_id: 1,
            curriculum_name: "Sample Year".into(),
            week,
            weeks: 3,
            term: week,
            week_started_on: "2026-09-07".into(),
            paused: false,
            year_complete: false,
            can_finish_week,
            days_on_week,
            together: Vec::new(),
            boys: vec![BoyToday {
                user_id: 1,
                name: "Isaiah".into(),
                due_today: Vec::new(),
                catch_up: Vec::new(),
                done: Vec::new(),
                done_count: 3,
                skipped_count: 1,
                total_count: 11,
            }],
            term_notes: Vec::new(),
        }
    }

    #[test]
    fn the_header_chip_reads_the_way_the_plan_writes_it() {
        assert_eq!(
            header_chip_text(&group(2, false, 0)),
            "Week 2 of 3 · Term 2 · 3 done · 1 skipped / 11"
        );
    }

    #[test]
    fn a_complete_week_nudges_towards_the_next_one() {
        assert_eq!(
            nudge_line(&group(2, true, 3)).as_deref(),
            Some("Week 2 done — start week 3?")
        );
    }

    #[test]
    fn a_fortnight_on_one_week_nudges_by_elapsed_days_instead() {
        assert_eq!(
            nudge_line(&group(3, false, 15)).as_deref(),
            Some("You've been on week 3 for 15 days")
        );
        // Thirteen days is not yet a nudge: H2's threshold is 14.
        assert_eq!(nudge_line(&group(3, false, 13)), None);
    }

    #[test]
    fn a_paused_or_finished_year_never_nudges() {
        let mut paused = group(2, true, 20);
        paused.paused = true;
        assert_eq!(nudge_line(&paused), None);

        let mut complete = group(4, true, 20);
        complete.year_complete = true;
        assert_eq!(nudge_line(&complete), None);
    }

    #[test]
    fn a_term_note_kind_never_renders_as_its_wire_string() {
        assert_eq!(term_note_label(TermNoteKind::Geography), "Geography");
        assert_eq!(term_note_label(TermNoteKind::FreeRead), "Free read");
        assert_eq!(term_note_label(TermNoteKind::Poetry), "Poetry");
        let note = TermNote {
            id: 1,
            term: 1,
            kind: TermNoteKind::FreeRead,
            text: "The Otter's Almanac".into(),
            sort_order: 0,
        };
        assert_ne!(term_note_label(note.kind), note.kind.as_str());
    }
}
