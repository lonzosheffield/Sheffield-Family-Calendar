use dioxus::prelude::*;

use crate::client::components::dashboard::Dashboard;
use crate::client::components::routine::Routine;
use crate::client::components::screensaver::Screensaver;
use crate::client::realtime::use_realtime_provider;
use crate::shared::types::MaximizedView;

const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");
const MANIFEST: Asset = asset!("/assets/manifest.json");

/// Global UI state shared by every panel.
#[derive(Clone, Copy)]
pub struct AppState {
    /// Which panel, if any, currently owns the whole screen.
    pub current_view: Signal<MaximizedView>,
    /// The family profile whose routine is being displayed (1..=4).
    pub active_user_id: Signal<u32>,
}

pub fn use_app_state() -> AppState {
    use_context::<AppState>()
}

#[derive(Routable, Clone, PartialEq)]
pub enum Route {
    /// Fire OS kiosk dashboard.
    #[route("/")]
    Home {},
    /// Companion phone view: just the routine, full width.
    #[route("/mobile")]
    Mobile {},
    /// Short phone URL used by the split-origin deployment (PLAN v2 D3′).
    /// Renders exactly the same view as `/mobile`.
    #[route("/m")]
    MobileShort {},
}

#[component]
pub fn App() -> Element {
    use_realtime_provider();
    use_context_provider(|| AppState {
        current_view: Signal::new(MaximizedView::None),
        active_user_id: Signal::new(1),
    });

    rsx! {
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        document::Link { rel: "manifest", href: MANIFEST }
        document::Meta {
            name: "viewport",
            content: "width=device-width, initial-scale=1, viewport-fit=cover, user-scalable=no",
        }
        document::Meta { name: "theme-color", content: "#2672B3" }
        Router::<Route> {}
    }
}

#[component]
pub fn Home() -> Element {
    rsx! {
        div { class: "relative h-full w-full bg-sheffield-paper font-display text-slate-800",
            Dashboard {}
            Screensaver {}
        }
    }
}

#[component]
pub fn MobileShort() -> Element {
    rsx! {
        Mobile {}
    }
}

#[component]
pub fn Mobile() -> Element {
    rsx! {
        div { class: "min-h-screen w-full bg-sheffield-paper p-4 font-display text-slate-800",
            Routine { compact: true }
        }
    }
}
