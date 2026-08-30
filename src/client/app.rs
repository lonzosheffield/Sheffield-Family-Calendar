use dioxus::prelude::*;

use crate::client::components::mobile::MobileShell;
use crate::client::components::screensaver::Screensaver;
use crate::client::components::tv::TvShell;
use crate::client::realtime::use_realtime_provider;
use crate::shared::types::MaximizedView;

const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

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
    /// Fire OS kiosk dashboard. `GET /` itself is a 308 redirect to `/tv`
    /// registered directly on the axum router (`server::router::build_router`,
    /// PLAN v2 T0.6 / D3′) before it ever reaches this SPA router; `Home`
    /// stays reachable for in-app client-side navigation.
    #[route("/")]
    Home {},
    /// The TV kiosk's URL of record (PLAN v2 D3′): `http://<ip>:8080/tv`.
    /// Renders exactly the same view as `Home`.
    #[route("/tv")]
    Tv {},
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
        // T2.2 / G6 / R-16: the manifest is linked at its **root** URL, not
        // through `asset!()`. A hashed `/assets/<hash>-manifest.json` puts
        // `start_url: "/m"` outside the manifest's own scope, and the install
        // prompt never appears however many icons it lists.
        document::Link {
            rel: "manifest",
            href: crate::client::components::mobile::pwa::MANIFEST_PATH,
        }
        document::Link {
            rel: "apple-touch-icon",
            href: "/icons/icon-192.png",
        }
        document::Meta { name: "apple-mobile-web-app-capable", content: "yes" }
        document::Meta {
            name: "apple-mobile-web-app-status-bar-style",
            content: "black-translucent",
        }
        document::Meta { name: "apple-mobile-web-app-title", content: "Family Hub" }
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
        KioskDashboard {}
    }
}

/// The TV kiosk's URL of record (PLAN v2 D3′ / T0.6): `/tv`. Renders exactly
/// the same view as [`Home`].
#[component]
pub fn Tv() -> Element {
    rsx! {
        KioskDashboard {}
    }
}

/// The kiosk (T2.1): the 10-foot, D-pad-only surface of PLAN v2 D8, plus the
/// ambient screensaver layered over it.
///
/// The old three-up `Dashboard` was a desktop layout — pointer-driven,
/// `hover:`-styled and 14 px in places — so `/tv` renders
/// [`TvShell`](crate::client::components::tv::TvShell) instead.
/// The old `components::dashboard` was deleted at the wave 2-b close once
/// neither `/tv` nor `/m` (T2.2's `MobileShell`) rendered it.
#[component]
fn KioskDashboard() -> Element {
    rsx! {
        div { class: "relative h-full w-full bg-sheffield-paper font-display text-slate-800",
            TvShell {}
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

/// The phone PWA. T2.2 replaced the v1 routine-only page (G9) with the full
/// five-tab shell — Routine · Calendar · Board · TV Remote · Settings —
/// which still renders `Routine { compact: true }` as its first tab, so
/// `tests/http_tests.rs::http_mobile_serves_routine_only_view` (a protected
/// T0.3 assertion, `docs/HANDOFF.md` H-2) keeps holding.
#[component]
pub fn Mobile() -> Element {
    rsx! {
        MobileShell {}
    }
}
