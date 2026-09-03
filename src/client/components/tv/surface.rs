//! The kiosk, drawn.
//!
//! [`TvSurface`] is a **pure** component: everything it draws comes from the
//! [`TvModel`] prop, it opens no socket, calls no server function and holds
//! no hook. That is what lets `tests/tv_tests.rs` render it into HTML and
//! assert, on the real markup, that the focus order matches the golden file,
//! that every focusable wears the ring, that no text is under 28 px and that
//! every full-screen container carries the 5 % overscan (PURPLE §P3 T2.1
//! (a), (b), (c), (f)). [`super::shell::TvShell`] is the impure half: it
//! fetches, listens and feeds this one a model.
//!
//! Every focusable element carries `data-tv-focus`, an `id` from
//! [`FocusId::dom_id`], and — on exactly one of them — the live focus ring.
//! Rendering walks [`focus_order`]'s output, so the DOM order and the order
//! the remote walks are the same list, not two lists that agree today.
//!
//! The whiteboard panel renders `children` in its frame. The live shell
//! passes T2.3's `Whiteboard` component; the tests pass nothing and get the
//! "drawing happens on the phone" placeholder, which keeps this task's
//! typography assertions scoped to this task's markup.

use dioxus::prelude::*;

use crate::client::components::calendar::CalendarState;
use crate::client::components::qr::qr_svg;
use crate::shared::homeschool::{Category, LogStatus};
use crate::shared::types::{
    routine_progress, CalendarEvent, CustomTaskView, DayItem, ExtraTask, LessonOccurrence,
    RoutineItemView,
};

use super::keymap::TV_KEYS;
use super::model::{
    current_focus, day_item_focus, lesson_key, school, FocusId, TvModel, TvOverlay, TvPanel,
    TvProfile, TvSchoolState,
};
use super::staleness::status_line;
use super::style::{
    focus_class, TV_BODY_LARGE, TV_BODY_TEXT, TV_CELEBRATION_SPIN_CLASS, TV_EYEBROW_CLASS,
    TV_FRAME_CLASS, TV_HEADING, TV_HEADING_LARGE, TV_JOIN_PILL_CLASS, TV_OVERSCAN_CLASS,
    TV_PANEL_HEADING_CLASS, TV_POSTER_CARD_CLASS, TV_PROFILE_BUTTON_CLASS, TV_PROFILE_DISC_CLASS,
    TV_PROFILE_RAIL_CLASS, TV_STAMP_CLASS, TV_WORDMARK_DISPLAY_CLASS, TV_WORDMARK_QUIET_CLASS,
};
use crate::client::components::glyphs::{
    ball_glyph, category_glyph, icon_glyph, subject_glyph, ADD_PHONE_GLYPH, EXTRA_TASK_GLYPH,
    HOMESCHOOL_GLYPH, ROUTINE_GLYPH, YEAR_COMPLETE_GLYPH,
};
use crate::client::components::palette::{best_ink_on, Rgb, SHEFFIELD_DARK};

/// Every full-screen container on the kiosk: **the poster's blue frame**, the
/// display face, 30 px base text, 5 % overscan.
///
/// D4.3 turned the overscan band into the frame (§3.1): the ground is
/// `sheffield-light` and carries no ink of its own, because everything the
/// kiosk says lives on the white poster card
/// ([`TV_POSTER_CARD_CLASS`](super::style::TV_POSTER_CARD_CLASS)) inside it.
fn screen_class(extra: &str) -> String {
    format!(
        "relative flex h-full min-h-screen w-full flex-col {TV_FRAME_CLASS} font-display \
         {TV_BODY_TEXT} {TV_OVERSCAN_CLASS} {extra}"
    )
}

/// The two aria-hidden sports balls that anchor the bottom corners of the
/// frame — four balls for four boys, straight off the poster (§2.8, §3.1.6).
///
/// They sit on the blue band, never over text, and never anywhere else.
fn corner_balls() -> Element {
    let left = format!("{}{}", ball_glyph(1), ball_glyph(2));
    let right = format!("{}{}", ball_glyph(3), ball_glyph(4));
    rsx! {
        span {
            class: "{TV_HEADING} pointer-events-none absolute bottom-[1.6%] left-[1.6%] select-none",
            "aria-hidden": "true",
            "{left}"
        }
        span {
            class: "{TV_HEADING} pointer-events-none absolute bottom-[1.6%] right-[1.6%] select-none",
            "aria-hidden": "true",
            "{right}"
        }
    }
}

#[component]
pub fn TvSurface(model: TvModel, children: Element) -> Element {
    let focused = current_focus(&model);

    if model.state.overlay == TvOverlay::JoinQr {
        return join_overlay(&model, focused.as_ref());
    }

    rsx! {
        div {
            id: "tv-root",
            class: screen_class(""),
            "data-tv-surface": "1",
            "data-tv-panel": model.state.panel.slug(),
            "data-tv-profile": model.active_profile().map(|p| p.id.to_string()).unwrap_or_default(),

            // The poster card: one white page with a thin dark border, and
            // the only element on the kiosk that wears one (§2.3).
            div { class: "{TV_POSTER_CARD_CLASS}",

                {header(&model)}

                div { class: "flex min-h-0 flex-1 gap-10",
                    {profile_rail(&model, focused.as_ref())}
                    main { class: "flex min-h-0 flex-1 flex-col gap-6",
                        {panel_body(&model, focused.as_ref(), children)}
                    }
                }

                {panel_hints(&model)}
            }

            {corner_balls()}

            if model.keys_debug {
                {keys_overlay(&model)}
            }
        }
    }
}

/// Is every routine item ticked? The 8/8 state (§2.4) — the count chip turns
/// sun-yellow and the wordmark's two suns start to turn.
fn routine_complete(model: &TvModel) -> bool {
    !model.routine.is_empty() && model.routine.iter().all(|item| item.completed)
}

// ---------------------------------------------------------------------------
// Header: panel title, the permanent "updated HH:MM", the badge
// ---------------------------------------------------------------------------

fn header(model: &TvModel) -> Element {
    let badge_lit = !model.connected || model.stale;
    let profile = model
        .active_profile()
        .map(|p| p.name.clone())
        .unwrap_or_else(|| "Sheffield Family".to_string());
    let routine = model.state.panel == TvPanel::Routine;
    let celebrating = routine && routine_complete(model);
    // HS6 / H6: the School panel's own header is `🏠 School · Week 3`. The
    // week comes from the boy's enrollment, so a boy with none (or a hub that
    // has not answered yet) gets the bare panel title rather than `Week 0`.
    let school_glyph = (model.state.panel == TvPanel::Homeschool).then_some(HOMESCHOOL_GLYPH);
    let heading = match (model.state.panel, school(model).week) {
        (TvPanel::Homeschool, Some(week)) => {
            format!("{} · Week {week}", TvPanel::Homeschool.title())
        }
        (panel, _) => panel.title().to_string(),
    };

    rsx! {
        header { class: "flex shrink-0 items-end justify-between gap-10",
            if routine {
                {wordmark(model.state.panel.title(), &profile, celebrating)}
            } else {
                h1 { class: "{TV_HEADING_LARGE} {TV_PANEL_HEADING_CLASS} flex min-w-0 items-baseline gap-5",
                    if let Some(glyph) = school_glyph {
                        span { class: "shrink-0 select-none", "aria-hidden": "true", "{glyph}" }
                    }
                    span { class: "truncate", "{heading}" }
                }
            }
            div { class: "flex shrink-0 items-center gap-6",
                p {
                    id: "tv-updated-at",
                    class: "{TV_BODY_TEXT} font-semibold text-slate-600",
                    "{status_line(model.updated_at.as_deref())}"
                }
                if badge_lit {
                    p {
                        id: "tv-disconnected-badge",
                        class: "{TV_BODY_TEXT} rounded-full bg-sheffield-accent px-6 py-2 font-bold text-slate-800",
                        role: "status",
                        "Disconnected"
                    }
                }
            }
        }
    }
}

/// The poster's headline, rebuilt as the routine panel's wordmark (§2.6).
///
/// ```text
/// SHEFFIELD                     <- eyebrow: 30 px, bold, tracking-[0.35em]
/// ☀️ Morning Routine ☀️  Boy 1   <- 60 px Baloo 2 800; "Morning" is the
///                                  outlined display red, the rest is ink
/// ```
///
/// The loud word is the panel title's *first* word, so the lockup is built
/// from the same `TvPanel::title()` every other surface reads rather than
/// from a second, drifting copy of the words. The two suns are aria-hidden
/// emoji flanking the line, exactly two, never behind text (§2.8).
fn wordmark(title: &str, profile: &str, celebrating: bool) -> Element {
    let (loud, quiet) = title.split_once(' ').unwrap_or((title, ""));
    let sun = if celebrating {
        TV_CELEBRATION_SPIN_CLASS
    } else {
        ""
    };

    rsx! {
        div { class: "flex min-w-0 flex-col gap-1",
            span { class: "{TV_BODY_TEXT} {TV_EYEBROW_CLASS}", "SHEFFIELD" }
            div { class: "flex min-w-0 items-baseline gap-6",
                h1 { class: "{TV_HEADING_LARGE} flex shrink-0 items-baseline gap-5",
                    span { class: "select-none {sun}", "aria-hidden": "true", "{ROUTINE_GLYPH}" }
                    span { class: "{TV_WORDMARK_DISPLAY_CLASS}", "{loud}" }
                    if !quiet.is_empty() {
                        span { class: "{TV_WORDMARK_QUIET_CLASS}", "{quiet}" }
                    }
                    span { class: "select-none {sun}", "aria-hidden": "true", "{ROUTINE_GLYPH}" }
                }
                p { class: "{TV_HEADING} truncate text-slate-600", "{profile}" }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The profile rail
// ---------------------------------------------------------------------------

fn profile_rail(model: &TvModel, focused: Option<&FocusId>) -> Element {
    let profiles: Vec<TvProfile> = model.profiles.clone();
    let active = model.active_profile().map(|p| p.id);

    rsx! {
        nav {
            class: "{TV_PROFILE_RAIL_CLASS}",
            "aria-label": "Family profiles",
            for (index, profile) in profiles.into_iter().enumerate() {
                {profile_button(&profile, index as u32 + 1, active == Some(profile.id), focused == Some(&FocusId::Profile(profile.id)))}
            }
            {join_qr_button(focused == Some(&FocusId::JoinQr))}
        }
    }
}

/// One boy in the rail. `rail_index` is his 1-based position, which is what
/// picks his sports ball off the poster's bottom corners (§2.5): ⚽ 🏈 ⚾ 🏀,
/// cycling for a fifth profile.
fn profile_button(profile: &TvProfile, rail_index: u32, active: bool, focused: bool) -> Element {
    let id = FocusId::Profile(profile.id).dom_id();
    let ring = focus_class(focused);
    let fill = if active {
        "bg-sheffield-dark text-white"
    } else {
        "bg-white text-slate-800"
    };
    let initial = profile
        .name
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string());
    // The disc's colour comes from the `profiles` row, so the ink on it has
    // to be chosen rather than assumed: a white initial on Boy 4's
    // `#F4D03F` is 1.5:1, and on his brother's `#2672B3` it is 5.1:1
    // (T3.4 / `palette::best_ink_on`). A row with an unreadable colour falls
    // back to the hub's primary blue rather than to no disc at all.
    let disc = Rgb::parse(&profile.color).unwrap_or(SHEFFIELD_DARK);
    let disc_hex = disc.to_hex();
    let disc_ink = best_ink_on(disc);

    rsx! {
        button {
            id: "{id}",
            key: "{id}",
            "data-tv-focus": "profile",
            "aria-current": if active { "true" } else { "false" },
            class: "{ring} {fill} {TV_PROFILE_BUTTON_CLASS}",
            span {
                class: "{TV_HEADING} {TV_PROFILE_DISC_CLASS} {disc_ink}",
                style: "background-color: {disc_hex}",
                "{initial}"
            }
            span { class: "{TV_BODY_LARGE} min-w-0 flex-1 truncate font-bold", "{profile.name}" }
            span {
                class: "{TV_BODY_LARGE} shrink-0 select-none",
                "aria-hidden": "true",
                "{ball_glyph(rail_index)}"
            }
        }
    }
}

/// The rail's last entry.
///
/// QD-02 folded it down to one line: the second line ("Play/Pause shows the
/// code") cost 36 px the rail did not have, and the shortcut it advertised is
/// not the only way in — `Enter` on this very button opens the same overlay,
/// which is the route a child on a D-pad actually takes.
fn join_qr_button(focused: bool) -> Element {
    let ring = focus_class(focused);
    rsx! {
        button {
            id: "tv-join-qr",
            "data-tv-focus": "join-qr",
            class: "{ring} {TV_JOIN_PILL_CLASS}",
            span { class: "{TV_BODY_LARGE} block truncate font-bold text-sheffield-dark",
                span { class: "select-none", "aria-hidden": "true", "{ADD_PHONE_GLYPH} " }
                "Add a phone"
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Panels
// ---------------------------------------------------------------------------

fn panel_body(model: &TvModel, focused: Option<&FocusId>, board: Element) -> Element {
    match model.state.panel {
        TvPanel::Routine => routine_panel(model, focused),
        TvPanel::Calendar => calendar_panel(model, focused),
        TvPanel::Whiteboard => whiteboard_panel(board),
        TvPanel::Homeschool => homeschool_panel(model, focused),
    }
}

fn routine_panel(model: &TvModel, focused: Option<&FocusId>) -> Element {
    let items: Vec<RoutineItemView> = model.routine.clone();
    let tasks: Vec<CustomTaskView> = model.tasks.clone();
    let progress = routine_progress(&model.routine);
    let done = model.routine.iter().filter(|i| i.completed).count();
    let total = model.routine.len();
    // §2.4 — all 8 done: the chip leaves the accent red for the poster's sun
    // yellow and gains a sun of its own. Both grounds are declared pairs
    // under `slate-800`; only the ground changes.
    let complete = routine_complete(model);
    let chip_ground = if complete {
        "bg-sheffield-sun"
    } else {
        "bg-sheffield-accent"
    };

    if items.is_empty() && tasks.is_empty() {
        return rsx! {
            p { class: "{TV_HEADING} text-slate-600", "Loading today's routine…" }
        };
    }

    rsx! {
        div { class: "flex shrink-0 items-center gap-8",
            div { class: "h-8 flex-1 overflow-hidden rounded-full bg-sheffield-light/30",
                div {
                    class: "h-full rounded-full bg-sheffield-dark",
                    style: "width: {progress}%",
                }
            }
            p {
                id: "tv-routine-count",
                class: "{TV_HEADING} shrink-0 rounded-full {chip_ground} px-8 py-1 font-poster font-bold text-slate-800",
                "{done} / {total}"
                if complete {
                    span { class: "select-none", "aria-hidden": "true", " {ROUTINE_GLYPH}" }
                }
            }
        }
        ul { class: "flex min-h-0 flex-1 flex-col gap-5 overflow-auto",
            for item in items.into_iter() {
                li { key: "tv-routine-{item.template_id}",
                    {routine_row(&item, focused == Some(&FocusId::RoutineItem(item.template_id)))}
                }
            }
            for task in tasks.into_iter() {
                li { key: "tv-task-{task.id}",
                    {task_row(&task, focused == Some(&FocusId::CustomTask(task.id)))}
                }
            }
        }
    }
}

/// The poster's row, in the poster's order: **icon → empty square → the
/// instruction, with the *why* in parentheses after it** (§1.3, §3.1.3).
///
/// The parenthetical why is the seeded `description` column — the app kept
/// the poster's voice from the first migration; this puts back its shape.
fn routine_row(item: &RoutineItemView, focused: bool) -> Element {
    let id = FocusId::RoutineItem(item.template_id).dom_id();
    let ring = focus_class(focused);
    let fill = if item.completed {
        "bg-sheffield-light/25 text-slate-800"
    } else {
        "bg-white text-slate-800"
    };
    let why = item.description.trim().to_string();

    rsx! {
        button {
            id: "{id}",
            "data-tv-focus": "routine-item",
            "aria-pressed": if item.completed { "true" } else { "false" },
            class: "{ring} {fill} flex items-center gap-8 px-8 py-6 shadow-lg",
            {row_glyph(icon_glyph(&item.icon_name))}
            {checkbox(item.completed)}
            span { class: "min-w-0 flex-1",
                span { class: "{TV_BODY_LARGE} font-bold", "{item.title}" }
                if !why.is_empty() {
                    span { class: "{TV_BODY_TEXT} text-slate-600", " ({why})" }
                }
            }
        }
    }
}

/// A row's leading glyph — the poster's full-colour clip-art, at 48 px.
///
/// Always `aria-hidden` beside real text and never wrapped in a `text-*`
/// colour class: emoji bring their own colour (§2.5).
fn row_glyph(glyph: &str) -> Element {
    rsx! {
        span {
            class: "{TV_HEADING} w-20 shrink-0 select-none text-center",
            "aria-hidden": "true",
            "{glyph}"
        }
    }
}

/// The poster's empty rounded square, and the stamp that lands in it (§2.4).
fn checkbox(checked: bool) -> Element {
    let mark = if checked { "✓" } else { "" };
    let state = if checked {
        format!("bg-sheffield-dark text-white {TV_STAMP_CLASS}")
    } else {
        "bg-white text-sheffield-dark ring-4 ring-sheffield-light".to_string()
    };

    rsx! {
        span {
            class: "{TV_HEADING} flex h-20 w-20 shrink-0 items-center justify-center rounded-2xl font-bold {state}",
            "{mark}"
        }
    }
}

fn task_row(task: &CustomTaskView, focused: bool) -> Element {
    let id = FocusId::CustomTask(task.id).dom_id();
    let ring = focus_class(focused);
    let fill = if task.is_completed {
        "bg-sheffield-light/25 text-slate-800"
    } else {
        "bg-white text-slate-800"
    };

    rsx! {
        button {
            id: "{id}",
            "data-tv-focus": "custom-task",
            "aria-pressed": if task.is_completed { "true" } else { "false" },
            class: "{ring} {fill} flex items-center gap-8 px-8 py-6 shadow-lg",
            // §3.1.4 — the photo thumbnail plays the glyph's role when there
            // is one; otherwise the row leads with `icon_glyph`'s fallback,
            // so a custom task still reads as a poster row.
            if let Some(path) = task.photo_path.clone() {
                img { class: "h-20 w-20 shrink-0 rounded-2xl object-cover", src: "{path}", alt: "" }
            } else {
                {row_glyph(icon_glyph("custom-task"))}
            }
            {checkbox(task.is_completed)}
            span { class: "{TV_BODY_LARGE} min-w-0 flex-1 font-bold", "{task.title}" }
        }
    }
}

/// The four calendar states, each rendered as itself (W3).
///
/// The three non-`Ready` arms are one unfocusable sentence apiece, so the
/// golden focus order is unchanged by them: a hub that cannot be reached
/// says so, and never borrows the empty day's words.
fn calendar_panel(model: &TvModel, focused: Option<&FocusId>) -> Element {
    let events: Vec<CalendarEvent> = match &model.events {
        CalendarState::Loading => {
            return rsx! {
                p { class: "{TV_HEADING} text-slate-600", "Loading the calendar…" }
            };
        }
        CalendarState::Error(_) => {
            return rsx! {
                p {
                    class: "{TV_HEADING} text-slate-600",
                    "Can't reach the hub's calendar — check the hub"
                }
            };
        }
        CalendarState::Empty => {
            return rsx! {
                p { class: "{TV_HEADING} text-slate-600", "Nothing on the calendar today." }
            };
        }
        CalendarState::Ready(events) => events.clone(),
    };

    rsx! {
        ul { class: "flex min-h-0 flex-1 flex-col gap-5 overflow-auto",
            for event in events.into_iter() {
                li { key: "{event.id}",
                    {event_row(&event, focused == Some(&FocusId::Event(event.id.clone())))}
                }
            }
        }
    }
}

fn event_row(event: &CalendarEvent, focused: bool) -> Element {
    let id = FocusId::Event(event.id.clone()).dom_id();
    let ring = focus_class(focused);
    let window = format_window(event);

    rsx! {
        button {
            id: "{id}",
            "data-tv-focus": "event",
            class: "{ring} flex items-center gap-8 border-l-8 border-sheffield-sun bg-white px-8 py-6 shadow-lg",
            span { class: "{TV_BODY_LARGE} w-64 shrink-0 font-bold text-sheffield-dark", "{window}" }
            span { class: "{TV_BODY_LARGE} min-w-0 flex-1 font-bold", "{event.summary}" }
        }
    }
}

/// `HH:MM – HH:MM`, or `All day`. Times arrive already in server-local form
/// (PURPLE §P5.5 default 14), so this only has to slice, never convert.
pub fn format_window(event: &CalendarEvent) -> String {
    if event.all_day {
        return "All day".to_string();
    }
    format!("{} – {}", clock(&event.start), clock(&event.end))
}

fn clock(timestamp: &str) -> String {
    timestamp
        .split('T')
        .nth(1)
        .and_then(|time| time.get(0..5))
        .unwrap_or(timestamp)
        .to_string()
}

fn whiteboard_panel(board: Element) -> Element {
    rsx! {
        div { class: "flex min-h-0 flex-1 flex-col gap-6",
            p { class: "{TV_BODY_TEXT} shrink-0 text-slate-600",
                "Drawing happens on a phone — the board shows here."
            }
            // On the white poster card the board needs an edge of its own:
            // the same light-blue ring the empty checkboxes wear, so the
            // drawing surface reads as a surface rather than as more page.
            div {
                class: "min-h-0 flex-1 overflow-hidden rounded-3xl bg-white ring-4 ring-sheffield-light",
                {board}
            }
        }
    }
}

// ---------------------------------------------------------------------------
// HS6 — the School panel (`docs/homeschool/PLAN_HOMESCHOOL.md` H6)
// ---------------------------------------------------------------------------

/// A section label in tracked caps — the one place on the kiosk besides the
/// wordmark's eyebrow where capitals and tracking are allowed (§2.6), and no
/// new type size (H6: "`TV_BODY_TEXT font-bold tracking-[0.35em] uppercase
/// text-slate-800`, no new size").
///
/// Written as a `<p>`, not an `<h2>`: 30 px is under the 44 px every heading
/// on the kiosk must clear, and a label that is not a heading should not
/// claim to be one.
fn section_label(text: &str) -> Element {
    rsx! {
        li { class: "shrink-0 list-none",
            p { class: "{TV_BODY_TEXT} font-bold tracking-[0.35em] uppercase text-slate-800", "{text}" }
        }
    }
}

/// One sentence, centred in the panel — the shape the calendar's three
/// non-`Ready` arms already use, so a state card never becomes focusable.
fn school_card(text: String) -> Element {
    rsx! {
        p { id: "tv-school-state", class: "{TV_HEADING} text-slate-600", "{text}" }
    }
}

fn homeschool_panel(model: &TvModel, focused: Option<&FocusId>) -> Element {
    match school(model).state {
        TvSchoolState::Loading => school_card("Loading today's school…".to_string()),
        // H6: "not enrolled → `No school plan for Simeon`".
        TvSchoolState::NotEnrolled(name) => school_card(format!("No school plan for {name}")),
        // H2: paused, or a day outside his school days.
        TvSchoolState::NoSchoolToday => school_card(format!("No school today {}", ball_glyph(1))),
        TvSchoolState::YearComplete => school_card(format!("Year complete {YEAR_COMPLETE_GLYPH}")),
        // The routine's 8/8 chip, in the School panel's words (§2.4).
        TvSchoolState::AllDone { done, total } => rsx! {
            div { class: "flex shrink-0 items-center gap-8",
                p {
                    id: "tv-school-count",
                    class: "{TV_HEADING} shrink-0 rounded-full bg-sheffield-sun px-8 py-1 font-poster font-bold text-slate-800",
                    "{done} / {total}"
                    span { class: "select-none", "aria-hidden": "true", " {ROUTINE_GLYPH}" }
                }
                p { class: "{TV_HEADING} text-slate-800", "School work all done!" }
            }
        },
        TvSchoolState::Day {
            due_today,
            catch_up,
            ..
        } => rsx! {
            ul { class: "flex min-h-0 flex-1 flex-col gap-5 overflow-auto",
                if !due_today.is_empty() {
                    {section_label("TODAY")}
                }
                for item in due_today.into_iter() {
                    li { key: "{day_item_focus(&item).dom_id()}",
                        {day_row(&item, focused)}
                    }
                }
                if !catch_up.is_empty() {
                    {section_label("STILL TO FINISH")}
                }
                for item in catch_up.into_iter() {
                    li { key: "{day_item_focus(&item).dom_id()}",
                        {day_row(&item, focused)}
                    }
                }
            }
        },
    }
}

fn day_row(item: &DayItem, focused: Option<&FocusId>) -> Element {
    let is_focused = focused == Some(&day_item_focus(item));
    match item {
        DayItem::Lesson(lesson) => lesson_row(lesson, is_focused),
        DayItem::Extra(extra) => extra_row(extra, is_focused),
    }
}

/// The row's second line: the week's text, the narration prompt every reading
/// carries (W-5), and which part of a split this is (H3 rule 5).
fn lesson_detail(lesson: &LessonOccurrence) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(text) = lesson
        .text
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        parts.push(text.to_string());
    }
    if let Some((k, n)) = lesson.part {
        if n > 1 {
            parts.push(if k <= 1 {
                format!("part {k} of {n}")
            } else {
                format!("continue · {k} of {n}")
            });
        }
    }
    // Narration is a prompt, never state (§1): every reading ends with the
    // child telling it back.
    if lesson.category == Category::Reading {
        parts.push("(then tell it back)".to_string());
    }
    parts.join(" · ")
}

/// Ground and ink for a row that has been logged, in the routine row's own
/// two declared pairs.
fn row_fill(logged: bool) -> &'static str {
    if logged {
        "bg-sheffield-light/25 text-slate-800"
    } else {
        "bg-white text-slate-800"
    }
}

/// A skipped occurrence is not a done one: the box stays empty and the row
/// says why, on the same accent chip the phone's catch-up tag uses.
fn skipped_chip() -> Element {
    rsx! {
        span {
            class: "{TV_BODY_TEXT} shrink-0 rounded-full bg-sheffield-accent px-6 py-2 font-bold text-slate-800",
            "Skipped"
        }
    }
}

fn lesson_row(lesson: &LessonOccurrence, focused: bool) -> Element {
    let id = FocusId::Lesson(lesson_key(lesson)).dom_id();
    let ring = focus_class(focused);
    let done = lesson.status == Some(LogStatus::Done);
    let fill = row_fill(lesson.status.is_some());
    let glyph = subject_glyph(lesson.icon_name.as_deref(), lesson.category.as_str());
    let detail = lesson_detail(lesson);

    rsx! {
        button {
            id: "{id}",
            "data-tv-focus": "lesson",
            "aria-pressed": if done { "true" } else { "false" },
            class: "{ring} {fill} flex items-center gap-8 px-8 py-6 shadow-lg",
            {row_glyph(glyph)}
            {checkbox(done)}
            span { class: "min-w-0 flex-1",
                span { class: "{TV_BODY_LARGE} font-bold", "{lesson.title}" }
                if !detail.is_empty() {
                    span { class: "{TV_BODY_TEXT} text-slate-600", " {detail}" }
                }
            }
            if lesson.status == Some(LogStatus::Skipped) {
                {skipped_chip()}
            }
        }
    }
}

/// A parent-added task, in the same list and with the same anatomy — the pin
/// rides in front of the category glyph so the boy can see at a glance which
/// rows his mother added (H8, §4 default 6: he may tick it, never make one).
fn extra_row(extra: &ExtraTask, focused: bool) -> Element {
    let id = FocusId::Extra(extra.id).dom_id();
    let ring = focus_class(focused);
    let done = extra.status == Some(LogStatus::Done);
    let fill = row_fill(extra.status.is_some());
    let glyph = format!(
        "{EXTRA_TASK_GLYPH}{}",
        category_glyph(extra.category.as_str())
    );
    let text = extra
        .text
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .unwrap_or_default()
        .to_string();

    rsx! {
        button {
            id: "{id}",
            "data-tv-focus": "extra",
            "aria-pressed": if done { "true" } else { "false" },
            class: "{ring} {fill} flex items-center gap-8 px-8 py-6 shadow-lg",
            {row_glyph(&glyph)}
            {checkbox(done)}
            span { class: "min-w-0 flex-1",
                span { class: "{TV_BODY_LARGE} font-bold", "{extra.title}" }
                if !text.is_empty() {
                    span { class: "{TV_BODY_TEXT} text-slate-600", " {text}" }
                }
            }
            if extra.status == Some(LogStatus::Skipped) {
                {skipped_chip()}
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Footer: where Left/Right goes next
// ---------------------------------------------------------------------------

fn panel_hints(model: &TvModel) -> Element {
    let current = model.state.panel;
    let hints: Vec<(TvPanel, String)> = TvPanel::ALL
        .into_iter()
        .map(|panel| {
            let fill = if panel == current {
                "bg-sheffield-dark text-white"
            } else {
                "bg-white text-slate-600"
            };
            (
                panel,
                format!("{TV_BODY_TEXT} rounded-full px-8 py-2 font-bold {fill}"),
            )
        })
        .collect();

    rsx! {
        footer { class: "flex shrink-0 items-center justify-center gap-10",
            for (panel, class) in hints.into_iter() {
                span { key: "{panel.slug()}", class: "{class}", "{panel.title()}" }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The phone-join QR overlay
// ---------------------------------------------------------------------------

fn join_overlay(model: &TvModel, focused: Option<&FocusId>) -> Element {
    let url = model.join_url.clone();
    let ring = focus_class(focused == Some(&FocusId::OverlayClose));
    // 320 px, not the old 520: the overlay now lives inside the poster card
    // rather than on the whole page, and a code that overflowed the card
    // would spill onto the frame. Measured against the 1080-line kiosk, the
    // whole stack (heading, sentence, code, URL, Back) fits the card's
    // 800 px interior with room to spare. A phone is held a hand's width
    // from the screen to scan, so 320 px on a 1920-wide panel still reads
    // instantly.
    let svg = url.as_deref().and_then(|url| qr_svg(url, 320).ok());

    rsx! {
        div {
            id: "tv-overlay",
            class: screen_class(""),
            "data-tv-surface": "1",
            "data-tv-panel": model.state.panel.slug(),
            "data-tv-overlay": "join-qr",

            div { class: "{TV_POSTER_CARD_CLASS} items-center justify-center gap-6",
                h1 { class: "{TV_HEADING_LARGE} flex items-baseline gap-5 {TV_PANEL_HEADING_CLASS}",
                    span { class: "select-none", "aria-hidden": "true", "{ADD_PHONE_GLYPH}" }
                    "Add a phone"
                }
                p { class: "{TV_BODY_LARGE} text-slate-600",
                    "Scan this with the phone's camera, on the home Wi‑Fi."
                }
                // A smaller poster on the poster: the code sits on the paper
                // tint so it reads as its own card against the white page
                // (§3.4).
                if let Some(svg) = svg {
                    div {
                        class: "rounded-3xl bg-sheffield-paper p-8 shadow-lg",
                        dangerous_inner_html: "{svg}",
                    }
                }
                if let Some(url) = url {
                    p { class: "{TV_BODY_LARGE} font-bold tracking-wide text-slate-800", "{url}" }
                } else {
                    p { class: "{TV_BODY_LARGE} text-slate-600", "Waiting for the hub's address…" }
                }
                button {
                    id: "tv-overlay-close",
                    "data-tv-focus": "overlay-close",
                    class: "{ring} w-auto bg-sheffield-dark px-12 py-6 shadow-lg",
                    span { class: "{TV_BODY_LARGE} font-bold text-white", "Back" }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// `?keys=1` — the key-code debug overlay (D8 / R-11)
// ---------------------------------------------------------------------------

/// A corner HUD listing the last presses exactly as the browser reported
/// them, plus the map the kiosk is using.
///
/// This exists so the owner can point the *real* remote at the *real* Fire OS
/// WebView and read back the `key`/`code` it actually emits (Appendix A step
/// A5) — the one thing about the remote that cannot be established from this
/// PC. It is not focusable: a debug HUD that captured the D-pad would defeat
/// its own purpose.
fn keys_overlay(model: &TvModel) -> Element {
    let log = model.key_log.clone();
    rsx! {
        aside {
            id: "tv-keys-overlay",
            class: "{TV_BODY_TEXT} absolute right-[5%] top-[5%] w-[38rem] rounded-3xl bg-slate-800 p-8 text-white shadow-lg",
            "aria-live": "polite",
            h2 { class: "{TV_HEADING} font-bold", "Key codes" }
            if log.is_empty() {
                p { class: "text-slate-200", "Press any button on the remote." }
            }
            ul { class: "flex flex-col gap-2",
                for (index, entry) in log.iter().enumerate().rev() {
                    li { key: "{index}", class: "flex items-baseline justify-between gap-6",
                        span { class: "font-bold", "{entry.key}" }
                        span { class: "text-slate-200", "{entry.code}" }
                        span { class: "text-slate-200", "{entry.action()}" }
                    }
                }
            }
            h2 { class: "{TV_HEADING} font-bold", "Map" }
            ul { class: "flex flex-col gap-2",
                for key in TV_KEYS.into_iter() {
                    li { key: "{key.canonical_key_name()}", class: "flex items-baseline justify-between gap-6",
                        span { class: "font-bold", "{key.canonical_key_name()}" }
                        span { class: "text-slate-200", "{key.describe()}" }
                    }
                }
            }
            p { class: "text-slate-200", "No Escape key: Fire TV remotes do not have one." }
        }
    }
}
