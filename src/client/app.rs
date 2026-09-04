use dioxus::prelude::*;

use crate::client::components::mobile::MobileShell;
use crate::client::components::screensaver::Screensaver;
use crate::client::components::tv::style::TV_VIEWPORT_META;
use crate::client::components::tv::TvShell;
use crate::client::realtime::use_realtime_provider;
use crate::shared::types::MaximizedView;

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
        // Q1-01: served straight from the binary at `server::router::build_router`
        // (`GET /tailwind.css`, `include_str!`'d from `assets/tailwind.css`), not
        // through `asset!()`. The `family-hub.exe` service binary this project
        // ships (`cargo build --release --bin family-hub`) has no `dx`-rewritten
        // manganis manifest beside it, so `asset!("/assets/tailwind.css")` used to
        // SSR the un-rewritten placeholder path here — a 503 and an unstyled
        // kiosk. A stable, literal `href` always resolves, on that binary and on
        // the `dx build` bundle alike.
        document::Link { rel: "stylesheet", href: "/tailwind.css" }
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
        // The viewport meta is deliberately **not** here (B-1 / TV1). A single
        // global `width=device-width` is what made the kiosk paint at 2× and
        // clip on the Insignia, whose `wm size` override + density 320 give
        // the WebView a 960 CSS px `device-width` while every pixel of `/tv`
        // is designed for 1920. Each surface now declares its own — see
        // [`KioskDashboard`] and [`Mobile`].
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
///
/// # The viewport meta lives here (`docs/BACKLOG.md` B-1)
///
/// `document::Meta` is not tied to the root component: it is a hook that
/// calls `create_meta` on whichever `Document` provider is in context, and on
/// the server that provider **collects** every meta rendered in the first SSR
/// frame into the page's `<head>` — whichever component rendered it, as long
/// as it rendered outside a suspense boundary. `TvShell`'s resources do not
/// suspend, so a meta placed here lands in the head of `/tv` **and** `/`
/// (both routes render this component) and nowhere else. The Router renders
/// exactly one route, so a page carries exactly one Dioxus-emitted viewport
/// meta. Hydration is safe by construction: the server writes a marker for
/// every head element it rendered, and the wasm document skips re-creating
/// those — the same path the `apple-mobile-web-app-*` metas already take.
/// See `docs/design/PLAN_TV_VIEWPORT.md` §1.1.
///
/// **On the `dx build` bundle there will be two** (plan §1.2): the bundle's
/// own `public/index.html` template head carries
/// `width=device-width, initial-scale=1` ahead of everything Dioxus collects.
/// Chromium applies viewport metas in document order, each replacing the
/// previous description, so **the last one wins** — ours. `curl`ing the
/// production `/tv` and seeing two tags is expected; the one that counts is
/// the last. The integration tests boot the router without a template
/// (`IndexHtml::ssr_only()`, empty head), so there each route carries exactly
/// one — which is what `tests/router_tests.rs` asserts.
#[component]
fn KioskDashboard() -> Element {
    rsx! {
        document::Meta { name: "viewport", content: TV_VIEWPORT_META }
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
/// tab shell — six tabs since HS5: Routine · School · Calendar · Board ·
/// Remote · Settings (T2.2 shipped five; School is tab 2, "TV Remote" became
/// "Remote") — which still renders `Routine { compact: true }` as its first tab, so
/// `tests/http_tests.rs::http_mobile_serves_routine_only_view` (a protected
/// T0.3 assertion, `docs/HANDOFF.md` H-2) keeps holding.
///
/// # The phone's viewport meta lives here (B-1 / TV1)
///
/// Byte-identical to the global meta this replaced, so `/m` and `/mobile`
/// (both render this component) see no change at all: `device-width` because
/// a phone's own width *is* the design width, `viewport-fit=cover` because
/// `mobile/mod.rs` positions the tab bar with
/// `pb-[env(safe-area-inset-bottom)]` and that inset is zero without it. The
/// manifest `Link`, the `apple-*` metas and `theme-color` stay global in
/// [`App`] — only the viewport is route-scoped. See `KioskDashboard` above
/// for how Dioxus collects a route-scoped meta into the head, and
/// `docs/design/PLAN_TV_VIEWPORT.md` §1.1–§1.2.
#[component]
pub fn Mobile() -> Element {
    rsx! {
        document::Meta {
            name: "viewport",
            content: "width=device-width, initial-scale=1, viewport-fit=cover, user-scalable=no",
        }
        MobileShell {}
    }
}
