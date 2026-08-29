use dioxus::prelude::*;

use crate::client::realtime::use_realtime;
use crate::server::api::get_today_events;
use crate::shared::types::CalendarEvent;

#[component]
pub fn CalendarPanel() -> Element {
    let (bus, _sender) = use_realtime();

    // Protocol v2 (T1.2): `CalendarUpdated` carries only the affected date, so
    // the panel refetches instead of trusting a pushed payload. Reading
    // `calendar_version` makes the broadcast a reactive dependency of the
    // resource. T2.4 replaces this with the `Loading`/`Empty`/`Error` state
    // machine (W3).
    let events_resource = use_resource(move || async move {
        let _version = (bus.calendar_version)();
        get_today_events().await
    });

    let events: Vec<CalendarEvent> = match &*events_resource.read_unchecked() {
        Some(Ok(events)) => events.clone(),
        _ => Vec::new(),
    };

    rsx! {
        div { class: "flex h-full flex-col gap-3",
            if events.is_empty() {
                p { class: "text-slate-400", "Nothing on the calendar today." }
            }
            ul { class: "space-y-2 overflow-auto",
                for event in events.iter() {
                    li {
                        key: "{event.id}",
                        class: "rounded-2xl border-l-4 border-sheffield-sun bg-white p-3 shadow-sm",
                        p { class: "font-semibold", "{event.summary}" }
                        p { class: "text-sm text-slate-500", "{format_window(event)}" }
                    }
                }
            }
        }
    }
}

fn format_window(event: &CalendarEvent) -> String {
    if event.all_day {
        return "All day".to_string();
    }
    format!("{} – {}", clock(&event.start), clock(&event.end))
}

/// Pull `HH:MM` out of an RFC3339 timestamp without a date dependency.
fn clock(timestamp: &str) -> String {
    timestamp
        .split('T')
        .nth(1)
        .and_then(|time| time.get(0..5))
        .unwrap_or(timestamp)
        .to_string()
}
