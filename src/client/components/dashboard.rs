use dioxus::prelude::*;

use crate::client::app::use_app_state;
use crate::client::components::calendar::CalendarPanel;
use crate::client::components::routine::Routine;
use crate::client::components::whiteboard::Whiteboard;
use crate::shared::types::MaximizedView;

#[component]
pub fn Dashboard() -> Element {
    let state = use_app_state();
    let view = (state.current_view)();

    match view {
        MaximizedView::None => rsx! {
            div { class: "grid h-full w-full grid-cols-1 gap-4 p-4 lg:grid-cols-3",
                Panel { view: MaximizedView::Routine, title: "Morning Routine", Routine { compact: false } }
                Panel { view: MaximizedView::Calendar, title: "Today", CalendarPanel {} }
                Panel { view: MaximizedView::Whiteboard, title: "Whiteboard", Whiteboard {} }
            }
        },
        maximized => rsx! {
            div { class: "flex h-full w-full flex-col gap-3 p-4",
                div { class: "flex items-center justify-between",
                    h1 { class: "text-3xl font-bold text-sheffield-dark", "{panel_title(maximized)}" }
                    RestoreButton {}
                }
                div { class: "h-full w-full overflow-auto rounded-3xl bg-white p-4 shadow-lg",
                    match maximized {
                        MaximizedView::Routine => rsx! { Routine { compact: false } },
                        MaximizedView::Calendar => rsx! { CalendarPanel {} },
                        MaximizedView::Whiteboard => rsx! { Whiteboard {} },
                        // T2.7: this dashboard is desktop/dead code (unreached
                        // by `/tv` or `/m`, `docs/HANDOFF.md` "T2.2 close");
                        // nothing here ever sets this variant, but the match
                        // must stay exhaustive.
                        MaximizedView::None | MaximizedView::Screensaver => rsx! {},
                    }
                }
            }
        },
    }
}

fn panel_title(view: MaximizedView) -> &'static str {
    match view {
        MaximizedView::Routine => "Morning Routine",
        MaximizedView::Calendar => "Today",
        MaximizedView::Whiteboard => "Whiteboard",
        MaximizedView::None | MaximizedView::Screensaver => "Sheffield Family Hub",
    }
}

#[component]
fn Panel(view: MaximizedView, title: String, children: Element) -> Element {
    let mut state = use_app_state();

    rsx! {
        section { class: "flex h-full min-h-0 flex-col overflow-hidden rounded-3xl bg-white shadow-lg",
            header { class: "flex items-center justify-between bg-sheffield-dark px-4 py-3 text-white",
                h2 { class: "text-xl font-bold", "{title}" }
                button {
                    class: "rounded-full bg-sheffield-light/40 px-3 py-1 text-sm font-semibold hover:bg-sheffield-light/70",
                    aria_label: "Maximize {title}",
                    onclick: move |_| state.current_view.set(view),
                    "Maximize"
                }
            }
            div { class: "min-h-0 flex-1 overflow-auto p-4", {children} }
        }
    }
}

#[component]
fn RestoreButton() -> Element {
    let mut state = use_app_state();

    rsx! {
        button {
            class: "rounded-full bg-sheffield-accent px-6 py-3 text-lg font-bold text-white shadow-md hover:brightness-110",
            onclick: move |_| state.current_view.set(MaximizedView::None),
            "Restore"
        }
    }
}
