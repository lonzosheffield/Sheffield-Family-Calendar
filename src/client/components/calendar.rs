//! The calendar surface — Today and Week, with explicit
//! `Loading`/`Empty`/`Error` states (T2.4, W3).
//!
//! v1 rendered `events.is_empty()` and nothing else, which meant three very
//! different situations all looked identical: the first paint before the
//! server had answered, a real failure, and a genuinely empty day. Worse, the
//! server's cache could never shrink back to nothing, so a day that had been
//! populated *stayed* populated — deleting the last event left the stale one
//! on the wall (W3/G10). [`CalendarState`] replaces that fallback: it is a
//! total function over "what the resource currently holds", every arm renders
//! something different, and it is unit-tested here rather than inferred from
//! the markup.
//!
//! Everything shown is **server-local** time (`docs/reviews/PURPLE_TEAM.md`
//! §P5.5 default 14): the week is built by the hub
//! ([`crate::server::api::calendar::get_calendar_week`]) and handed over
//! ready-made, because a phone in another timezone — or with a wrong clock —
//! must not be able to disagree with the television about what day it is.
//!
//! Editing is parent-gated and phone-shaped (§P5.5 default 35: "calendar
//! editing … phone-only"). The composer only appears when this device holds a
//! parent session; the hub checks the session again server-side regardless.

use dioxus::prelude::*;

use crate::client::components::mobile::session;
use crate::client::realtime::use_realtime;
use crate::server::api::calendar::{
    create_local_event, delete_local_event, get_calendar_week, get_events_for_day, LocalEventInput,
    WeekView,
};
use crate::shared::types::CalendarEvent;

// ---------------------------------------------------------------------------
// State machine (W3)
// ---------------------------------------------------------------------------

/// What the panel currently knows. Replaces v1's `is_empty()` fallback.
#[derive(Clone, PartialEq, Debug)]
pub enum CalendarState<T> {
    /// The request is in flight and nothing has ever been rendered.
    Loading,
    /// The hub answered with a failure; the message is shown, with a retry.
    Error(String),
    /// The hub answered, and there is genuinely nothing to show.
    Empty,
    /// The hub answered with content.
    Ready(T),
}

impl<T> CalendarState<T> {
    /// Fold "what the resource holds" into exactly one of the four states.
    ///
    /// `None` is *only* the first paint. An error is never silently swallowed
    /// into `Empty`, and an empty answer is never left looking like a pending
    /// one — those two confusions are precisely W3.
    pub fn resolve(value: Option<Result<T, String>>, is_empty: impl Fn(&T) -> bool) -> Self {
        match value {
            None => CalendarState::Loading,
            Some(Err(message)) => CalendarState::Error(message),
            Some(Ok(payload)) if is_empty(&payload) => CalendarState::Empty,
            Some(Ok(payload)) => CalendarState::Ready(payload),
        }
    }

    /// The state's name, used by the tests and by the `data-calendar-state`
    /// attribute the surfaces carry so a failure is diagnosable from the DOM.
    pub fn name(&self) -> &'static str {
        match self {
            CalendarState::Loading => "loading",
            CalendarState::Error(_) => "error",
            CalendarState::Empty => "empty",
            CalendarState::Ready(_) => "ready",
        }
    }
}

/// Which view the panel is showing.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum CalendarMode {
    #[default]
    Today,
    Week,
}

/// What one fetch returned, so both views can share one resource.
#[derive(Clone, PartialEq, Debug)]
pub enum CalendarPayload {
    Day(Vec<CalendarEvent>),
    Week(Box<WeekView>),
}

impl CalendarPayload {
    pub fn is_empty(&self) -> bool {
        match self {
            CalendarPayload::Day(events) => events.is_empty(),
            CalendarPayload::Week(week) => week.is_empty(),
        }
    }
}

/// The row id behind a wire event id, when the event is **local** and so may
/// be edited or deleted from a phone.
///
/// Wire ids are `{source}:{row id}@{occurrence start}`
/// ([`crate::server::calendar::Occurrence::to_calendar_event`]). A Google
/// event returns `None`: deleting one locally would only last until the next
/// poll replaced the window.
pub fn local_event_id(wire_id: &str) -> Option<i64> {
    wire_id
        .strip_prefix("local:")?
        .split('@')
        .next()?
        .parse()
        .ok()
}

/// `HH:MM – HH:MM`, or `All day`. Times arrive already in server-local form,
/// so this slices and never converts.
pub fn format_window(event: &CalendarEvent) -> String {
    if event.all_day {
        return "All day".to_string();
    }
    format!("{} – {}", clock(&event.start), clock(&event.end))
}

/// Pull `HH:MM` out of an RFC3339 timestamp without a date dependency
/// (`chrono` is server-only; this file also compiles to wasm).
fn clock(timestamp: &str) -> String {
    timestamp
        .split('T')
        .nth(1)
        .and_then(|time| time.get(0..5))
        .unwrap_or(timestamp)
        .to_string()
}

/// The repeat rules the composer offers. Anything more exotic is an `RRULE`
/// the owner can type; these four cover the family's actual calendar.
const REPEAT_CHOICES: [(&str, &str); 4] = [
    ("", "Does not repeat"),
    ("FREQ=DAILY", "Every day"),
    ("FREQ=WEEKLY", "Every week"),
    ("FREQ=MONTHLY", "Every month"),
];

// ---------------------------------------------------------------------------
// The panel
// ---------------------------------------------------------------------------

#[component]
pub fn CalendarPanel() -> Element {
    let (bus, _sender) = use_realtime();

    let mut mode = use_signal(CalendarMode::default);
    let mut status = use_signal(|| Option::<String>::None);

    // Protocol v2 (T1.2): `CalendarUpdated` and the midnight `DayRolled` both
    // bump `calendar_version`, so reading it here is what makes a broadcast a
    // reactive dependency of this fetch. The payload is never pushed — the
    // panel always refetches, which is what makes the message unspoofable.
    let mut calendar = use_resource(move || async move {
        let _version = (bus.calendar_version)();
        let today = (bus.today)();
        match mode() {
            CalendarMode::Today => get_events_for_day(today.unwrap_or_default())
                .await
                .map(CalendarPayload::Day)
                .map_err(|err| err.to_string()),
            CalendarMode::Week => get_calendar_week(None)
                .await
                .map(|week| CalendarPayload::Week(Box::new(week)))
                .map_err(|err| err.to_string()),
        }
    });

    let state =
        CalendarState::resolve(calendar.read_unchecked().clone(), CalendarPayload::is_empty);
    let state_name = state.name();
    let is_parent = session::is_parent();

    rsx! {
        div {
            class: "flex h-full min-h-0 flex-col gap-3",
            "data-calendar-state": "{state_name}",

            div { class: "flex shrink-0 items-center gap-2",
                for (choice , label) in [(CalendarMode::Today, "Today"), (CalendarMode::Week, "Week")] {
                    button {
                        key: "{label}",
                        r#type: "button",
                        class: if mode() == choice { "rounded-2xl bg-sheffield-dark px-4 py-2 text-sm font-semibold text-white shadow" } else { "rounded-2xl bg-white px-4 py-2 text-sm font-semibold text-sheffield-dark shadow ring-1 ring-slate-200" },
                        "aria-pressed": if mode() == choice { "true" } else { "false" },
                        onclick: move |_| {
                            mode.set(choice);
                            status.set(None);
                        },
                        "{label}"
                    }
                }
            }

            match &state {
                CalendarState::Loading => rsx! {
                    p { class: "text-slate-400", "Loading the calendar…" }
                },
                CalendarState::Error(message) => rsx! {
                    div { class: "rounded-2xl border-l-4 border-red-500 bg-red-50 p-3",
                        p { class: "font-semibold text-red-700", "The calendar could not be loaded." }
                        p { class: "text-sm text-red-600", "{message}" }
                        button {
                            r#type: "button",
                            class: "mt-2 rounded-2xl bg-red-600 px-4 py-2 text-sm font-semibold text-white shadow",
                            onclick: move |_| calendar.restart(),
                            "Try again"
                        }
                    }
                },
                CalendarState::Empty => rsx! {
                    p { class: "text-slate-400",
                        if mode() == CalendarMode::Week {
                            "Nothing on the calendar this week."
                        } else {
                            "Nothing on the calendar today."
                        }
                    }
                },
                CalendarState::Ready(CalendarPayload::Day(events)) => rsx! {
                    ul { class: "min-h-0 space-y-2 overflow-auto",
                        for event in events.iter() {
                            EventRow {
                                key: "{event.id}",
                                event: event.clone(),
                                deletable: is_parent,
                                on_deleted: move |message: String| {
                                    status.set(Some(message));
                                    calendar.restart();
                                },
                            }
                        }
                    }
                },
                CalendarState::Ready(CalendarPayload::Week(week)) => rsx! {
                    div { class: "grid min-h-0 grid-cols-1 gap-2 overflow-auto sm:grid-cols-7",
                        for day in week.days.iter() {
                            div {
                                key: "{day.date}",
                                class: if day.is_today { "rounded-2xl bg-white p-2 shadow-sm ring-2 ring-sheffield-sun" } else { "rounded-2xl bg-white p-2 shadow-sm" },
                                "data-day": "{day.date}",
                                p { class: "text-xs font-semibold uppercase tracking-wide text-slate-500",
                                    "{day.weekday} {day.day_of_month}"
                                }
                                if day.events.is_empty() {
                                    p { class: "text-xs text-slate-300", "—" }
                                }
                                ul { class: "space-y-1",
                                    for event in day.events.iter() {
                                        li {
                                            key: "{event.id}",
                                            class: "rounded-xl border-l-4 border-sheffield-sun bg-slate-50 px-2 py-1",
                                            p { class: "text-sm font-semibold", "{event.summary}" }
                                            p { class: "text-xs text-slate-500", "{format_window(event)}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
            }

            if let Some(message) = status() {
                p { class: "shrink-0 text-sm font-semibold text-slate-600", "{message}" }
            }

            if is_parent {
                EventComposer {
                    on_saved: move |message: String| {
                        status.set(Some(message));
                        calendar.restart();
                    },
                }
            } else {
                p { class: "shrink-0 text-xs text-slate-400",
                    "Sign in as a parent on the Settings tab to add or remove events."
                }
            }
        }
    }
}

#[component]
fn EventRow(event: CalendarEvent, deletable: bool, on_deleted: EventHandler<String>) -> Element {
    let row_id = local_event_id(&event.id);

    rsx! {
        li { class: "flex items-start gap-3 rounded-2xl border-l-4 border-sheffield-sun bg-white p-3 shadow-sm",
            div { class: "min-w-0 flex-1",
                p { class: "font-semibold", "{event.summary}" }
                p { class: "text-sm text-slate-500", "{format_window(&event)}" }
            }
            if let Some(id) = row_id {
                if deletable {
                    button {
                        r#type: "button",
                        class: "shrink-0 rounded-2xl px-3 py-2 text-sm font-semibold text-red-600 ring-1 ring-red-200",
                        "aria-label": "Delete {event.summary}",
                        onclick: move |_| async move {
                            let message = match delete_local_event(id, session::token()).await {
                                Ok(()) => "Event deleted.".to_string(),
                                Err(err) => format!("Could not delete that event: {err}"),
                            };
                            on_deleted.call(message);
                        },
                        "Delete"
                    }
                }
            }
        }
    }
}

#[component]
fn EventComposer(on_saved: EventHandler<String>) -> Element {
    let mut title = use_signal(String::new);
    let mut starts_at = use_signal(String::new);
    let mut ends_at = use_signal(String::new);
    let mut all_day = use_signal(|| false);
    let mut repeat = use_signal(String::new);

    rsx! {
        form {
            class: "shrink-0 rounded-2xl bg-white p-3 shadow-sm ring-1 ring-slate-200",
            onsubmit: move |event| async move {
                // Dioxus 0.7 submits the form by default (D7' break #7).
                event.prevent_default();
                if title().trim().is_empty() || starts_at().trim().is_empty() {
                    on_saved.call("An event needs a title and a start time.".into());
                    return;
                }
                let input = LocalEventInput {
                    title: title(),
                    starts_at: starts_at(),
                    ends_at: Some(ends_at()).filter(|value| !value.trim().is_empty()),
                    all_day: all_day(),
                    rrule: Some(repeat()).filter(|value| !value.trim().is_empty()),
                    ..LocalEventInput::default()
                };
                let message = match create_local_event(input, session::token()).await {
                    Ok(_) => {
                        title.set(String::new());
                        starts_at.set(String::new());
                        ends_at.set(String::new());
                        repeat.set(String::new());
                        "Event added.".to_string()
                    }
                    Err(err) => format!("Could not add that event: {err}"),
                };
                on_saved.call(message);
            },
            p { class: "mb-2 text-sm font-bold text-sheffield-dark", "Add an event" }
            div { class: "flex flex-col gap-2",
                input {
                    class: "rounded-xl bg-slate-50 px-3 py-2 text-base ring-1 ring-slate-200",
                    r#type: "text",
                    "aria-label": "Event title",
                    placeholder: "What is happening?",
                    value: "{title}",
                    oninput: move |event| title.set(event.value()),
                }
                input {
                    class: "rounded-xl bg-slate-50 px-3 py-2 text-base ring-1 ring-slate-200",
                    r#type: "datetime-local",
                    "aria-label": "Starts at",
                    value: "{starts_at}",
                    oninput: move |event| starts_at.set(event.value()),
                }
                input {
                    class: "rounded-xl bg-slate-50 px-3 py-2 text-base ring-1 ring-slate-200",
                    r#type: "datetime-local",
                    "aria-label": "Ends at",
                    value: "{ends_at}",
                    oninput: move |event| ends_at.set(event.value()),
                }
                label { class: "flex items-center gap-2 text-sm text-slate-600",
                    input {
                        r#type: "checkbox",
                        checked: all_day(),
                        oninput: move |event| all_day.set(event.checked()),
                    }
                    "All day"
                }
                select {
                    class: "rounded-xl bg-slate-50 px-3 py-2 text-base ring-1 ring-slate-200",
                    "aria-label": "Repeat",
                    value: "{repeat}",
                    oninput: move |event| repeat.set(event.value()),
                    for (value , label) in REPEAT_CHOICES {
                        option { key: "{label}", value: "{value}", "{label}" }
                    }
                }
                button {
                    class: "rounded-2xl bg-sheffield-dark px-5 py-3 text-base font-semibold text-white shadow",
                    r#type: "submit",
                    "Add event"
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::api::calendar::{CalendarDay, WeekView};

    fn event(id: &str) -> CalendarEvent {
        CalendarEvent {
            id: id.to_string(),
            summary: "Swimming".into(),
            start: "2026-08-29T17:00:00+01:00".into(),
            end: "2026-08-29T18:00:00+01:00".into(),
            all_day: false,
        }
    }

    /// **T2.4 (e)**, at the state-machine level: the four situations v1
    /// collapsed into one must be four distinct states, and an answer of "no
    /// events" must be `Empty` — never `Loading`, and never the previous
    /// content.
    #[test]
    fn the_four_states_are_distinct_and_an_empty_answer_is_empty() {
        let loading: CalendarState<CalendarPayload> =
            CalendarState::resolve(None, CalendarPayload::is_empty);
        assert_eq!(loading.name(), "loading");

        let failed: CalendarState<CalendarPayload> =
            CalendarState::resolve(Some(Err("pool closed".into())), CalendarPayload::is_empty);
        assert_eq!(failed.name(), "error");
        assert!(matches!(failed, CalendarState::Error(message) if message == "pool closed"));

        let ready = CalendarState::resolve(
            Some(Ok(CalendarPayload::Day(vec![event(
                "local:1@20260829T170000",
            )]))),
            CalendarPayload::is_empty,
        );
        assert_eq!(ready.name(), "ready");

        // The last event of the day is deleted: the panel is Empty, not the
        // stale event and not a spinner (W3).
        let emptied = CalendarState::resolve(
            Some(Ok(CalendarPayload::Day(Vec::new()))),
            CalendarPayload::is_empty,
        );
        assert_eq!(emptied.name(), "empty");
        assert!(matches!(emptied, CalendarState::Empty));
    }

    #[test]
    fn a_week_with_no_events_at_all_is_empty_and_one_event_is_ready() {
        let day = |date: &str, events: Vec<CalendarEvent>| CalendarDay {
            date: date.into(),
            weekday: "Sunday".into(),
            day_of_month: 8,
            is_today: false,
            events,
        };
        let mut week = WeekView {
            start: "2026-03-08".into(),
            end: "2026-03-14".into(),
            today: "2026-03-08".into(),
            days: (0..7).map(|_| day("2026-03-08", Vec::new())).collect(),
        };
        assert!(week.is_empty());
        assert_eq!(
            CalendarState::resolve(
                Some(Ok(CalendarPayload::Week(Box::new(week.clone())))),
                CalendarPayload::is_empty
            )
            .name(),
            "empty"
        );

        week.days[3].events.push(event("google:9@20260311T090000"));
        assert!(!week.is_empty());
        assert_eq!(
            CalendarState::resolve(
                Some(Ok(CalendarPayload::Week(Box::new(week)))),
                CalendarPayload::is_empty
            )
            .name(),
            "ready"
        );
    }

    #[test]
    fn only_local_events_expose_a_row_id_to_delete() {
        assert_eq!(local_event_id("local:12@20260829T170000"), Some(12));
        assert_eq!(local_event_id("google:12@20260829T170000"), None);
        assert_eq!(local_event_id("nonsense"), None);
        assert_eq!(local_event_id("local:@20260829T170000"), None);
    }

    #[test]
    fn the_window_label_slices_local_time_and_never_converts_it() {
        assert_eq!(format_window(&event("local:1@x")), "17:00 – 18:00");
        let all_day = CalendarEvent {
            all_day: true,
            start: "2026-10-26".into(),
            end: "2026-10-31".into(),
            ..event("google:2@x")
        };
        assert_eq!(format_window(&all_day), "All day");
    }
}
