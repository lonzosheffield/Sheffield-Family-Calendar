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

use crate::client::components::qr::qr_svg;
use crate::shared::types::{routine_progress, CalendarEvent, CustomTaskView, RoutineItemView};

use super::keymap::TV_KEYS;
use super::model::{current_focus, FocusId, TvModel, TvOverlay, TvPanel, TvProfile};
use super::palette::{best_ink_on, Rgb, SHEFFIELD_DARK};
use super::staleness::status_line;
use super::style::{
    focus_class, TV_BODY_LARGE, TV_BODY_TEXT, TV_HEADING, TV_HEADING_LARGE, TV_OVERSCAN_CLASS,
};

/// Every full-screen container on the kiosk: paper ground, display face,
/// 30 px base text, 5 % overscan.
fn screen_class(extra: &str) -> String {
    format!(
        "relative flex h-full min-h-screen w-full flex-col bg-sheffield-paper font-display \
         text-slate-800 {TV_BODY_TEXT} {TV_OVERSCAN_CLASS} {extra}"
    )
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
            class: screen_class("gap-8"),
            "data-tv-surface": "1",
            "data-tv-panel": model.state.panel.slug(),
            "data-tv-profile": model.active_profile().map(|p| p.id.to_string()).unwrap_or_default(),

            {header(&model)}

            div { class: "flex min-h-0 flex-1 gap-10",
                {profile_rail(&model, focused.as_ref())}
                main { class: "flex min-h-0 flex-1 flex-col gap-6",
                    {panel_body(&model, focused.as_ref(), children)}
                }
            }

            {panel_hints(&model)}

            if model.keys_debug {
                {keys_overlay(&model)}
            }
        }
    }
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

    rsx! {
        header { class: "flex shrink-0 items-baseline justify-between gap-10",
            div { class: "flex items-baseline gap-6",
                h1 { class: "{TV_HEADING_LARGE} font-bold text-sheffield-dark",
                    "{model.state.panel.title()}"
                }
                if model.state.panel == TvPanel::Routine {
                    p { class: "{TV_HEADING} text-slate-600", "{profile}" }
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

// ---------------------------------------------------------------------------
// The profile rail
// ---------------------------------------------------------------------------

fn profile_rail(model: &TvModel, focused: Option<&FocusId>) -> Element {
    let profiles: Vec<TvProfile> = model.profiles.clone();
    let active = model.active_profile().map(|p| p.id);

    rsx! {
        nav {
            class: "flex w-[26rem] shrink-0 flex-col gap-5 overflow-hidden",
            "aria-label": "Family profiles",
            for profile in profiles.into_iter() {
                {profile_button(&profile, active == Some(profile.id), focused == Some(&FocusId::Profile(profile.id)))}
            }
            {join_qr_button(focused == Some(&FocusId::JoinQr))}
        }
    }
}

fn profile_button(profile: &TvProfile, active: bool, focused: bool) -> Element {
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
            class: "{ring} {fill} flex items-center gap-6 px-8 py-6 shadow-lg",
            span {
                class: "{TV_HEADING} flex h-24 w-24 shrink-0 items-center justify-center rounded-full font-bold {disc_ink}",
                style: "background-color: {disc_hex}",
                "{initial}"
            }
            span { class: "{TV_BODY_LARGE} truncate font-bold", "{profile.name}" }
        }
    }
}

fn join_qr_button(focused: bool) -> Element {
    let ring = focus_class(focused);
    rsx! {
        button {
            id: "tv-join-qr",
            "data-tv-focus": "join-qr",
            class: "{ring} mt-auto bg-white px-8 py-6 shadow-lg",
            span { class: "{TV_BODY_LARGE} font-bold text-sheffield-dark", "Add a phone" }
            span { class: "{TV_BODY_TEXT} block text-slate-600", "Play/Pause shows the code" }
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
    }
}

fn routine_panel(model: &TvModel, focused: Option<&FocusId>) -> Element {
    let items: Vec<RoutineItemView> = model.routine.clone();
    let tasks: Vec<CustomTaskView> = model.tasks.clone();
    let progress = routine_progress(&model.routine);
    let done = model.routine.iter().filter(|i| i.completed).count();
    let total = model.routine.len();

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
                class: "{TV_HEADING} shrink-0 rounded-full bg-sheffield-accent px-8 py-1 font-bold text-slate-800",
                "{done} / {total}"
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

fn routine_row(item: &RoutineItemView, focused: bool) -> Element {
    let id = FocusId::RoutineItem(item.template_id).dom_id();
    let ring = focus_class(focused);
    let fill = if item.completed {
        "bg-sheffield-light/25 text-slate-800"
    } else {
        "bg-white text-slate-800"
    };
    let mark = if item.completed { "✓" } else { "" };
    let box_class = if item.completed {
        "bg-sheffield-dark text-white"
    } else {
        "bg-white text-sheffield-dark ring-4 ring-sheffield-light"
    };

    rsx! {
        button {
            id: "{id}",
            "data-tv-focus": "routine-item",
            "aria-pressed": if item.completed { "true" } else { "false" },
            class: "{ring} {fill} flex items-center gap-8 px-8 py-6 shadow-lg",
            span {
                class: "{TV_HEADING} flex h-20 w-20 shrink-0 items-center justify-center rounded-2xl font-bold {box_class}",
                "{mark}"
            }
            span { class: "min-w-0 flex-1",
                span { class: "{TV_BODY_LARGE} block font-bold", "{item.title}" }
                span { class: "{TV_BODY_TEXT} block text-slate-600", "{item.description}" }
            }
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
            if let Some(path) = task.photo_path.clone() {
                img { class: "h-20 w-20 shrink-0 rounded-2xl object-cover", src: "{path}", alt: "" }
            }
            span { class: "{TV_BODY_LARGE} min-w-0 flex-1 font-bold", "{task.title}" }
        }
    }
}

fn calendar_panel(model: &TvModel, focused: Option<&FocusId>) -> Element {
    let events: Vec<CalendarEvent> = model.events.clone();

    if events.is_empty() {
        return rsx! {
            p { class: "{TV_HEADING} text-slate-600", "Nothing on the calendar today." }
        };
    }

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
            div { class: "min-h-0 flex-1 overflow-hidden rounded-3xl bg-white shadow-lg", {board} }
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
    let svg = url.as_deref().and_then(|url| qr_svg(url, 520).ok());

    rsx! {
        div {
            id: "tv-overlay",
            class: screen_class("items-center justify-center gap-10"),
            "data-tv-surface": "1",
            "data-tv-panel": model.state.panel.slug(),
            "data-tv-overlay": "join-qr",

            h1 { class: "{TV_HEADING_LARGE} font-bold text-sheffield-dark", "Add a phone" }
            p { class: "{TV_HEADING} text-slate-600",
                "Scan this with the phone's camera, on the home Wi‑Fi."
            }
            if let Some(svg) = svg {
                div { class: "rounded-3xl bg-white p-10 shadow-lg", dangerous_inner_html: "{svg}" }
            }
            if let Some(url) = url {
                p { class: "{TV_HEADING} font-bold tracking-wide text-slate-800", "{url}" }
            } else {
                p { class: "{TV_HEADING} text-slate-600", "Waiting for the hub's address…" }
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
