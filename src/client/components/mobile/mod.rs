//! The phone PWA (PLAN v2 **D6**, task **T2.2**).
//!
//! Assumption A3 is what shapes this surface: **only the two parents have
//! phones.** The four boys drive the TV with the remote and never see this.
//! So the phone is a controller and an admin surface — richer than the
//! kiosk, allowed to be touch-only, and the one place that has to survive a
//! bad network, because a phone leaves the house and the television does not.
//!
//! What lives here:
//!
//! | file | what it is |
//! | --- | --- |
//! | `pwa.rs` | root manifest, `sw.js`, embedded icons, registration |
//! | `sw.js` | the service worker itself (declared in `docs/NON_RUST.md`) |
//! | `queue.rs` | the offline mutation queue — date + idempotency key, 48 h |
//! | `storage.rs` | `localStorage` shim, fallible everywhere |
//! | `session.rs` | the parent session token (`docs/HANDOFF.md` H-19) |
//! | `remote.rs` | the TV Remote tab — `SetView` / `SetActiveProfile` |
//! | `settings.rs` | parent sign-in, queue state, the offline promise |
//!
//! The per-platform offline promise this implements is written down for the
//! family in `docs/PWA.md`.

pub mod pwa;
pub mod queue;
pub mod remote;
pub mod session;
pub mod settings;
pub mod storage;

use dioxus::prelude::*;

use crate::client::components::calendar::CalendarPanel;
use crate::client::components::mobile::queue::{OfflineQueue, QueueToast};
use crate::client::components::mobile::remote::TvRemote;
use crate::client::components::mobile::settings::MobileSettings;
use crate::client::components::routine::Routine;
use crate::client::components::whiteboard::Whiteboard;
use crate::client::realtime::use_realtime;

/// The five bottom tabs, in bar order (PLAN v2 §3 T2.2).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub enum MobileTab {
    #[default]
    Routine,
    Calendar,
    Board,
    TvRemote,
    Settings,
}

impl MobileTab {
    /// Every tab, in the order they appear in the bar.
    pub const ALL: [MobileTab; 5] = [
        MobileTab::Routine,
        MobileTab::Calendar,
        MobileTab::Board,
        MobileTab::TvRemote,
        MobileTab::Settings,
    ];

    /// The label under the icon — and the accessible name of the button.
    pub fn label(self) -> &'static str {
        match self {
            MobileTab::Routine => "Routine",
            MobileTab::Calendar => "Calendar",
            MobileTab::Board => "Board",
            MobileTab::TvRemote => "TV Remote",
            MobileTab::Settings => "Settings",
        }
    }

    /// Stable identifier, used for `key`s and test assertions.
    pub fn slug(self) -> &'static str {
        match self {
            MobileTab::Routine => "routine",
            MobileTab::Calendar => "calendar",
            MobileTab::Board => "board",
            MobileTab::TvRemote => "tv-remote",
            MobileTab::Settings => "settings",
        }
    }

    /// A glyph rather than an icon font or an SVG sprite: five characters
    /// beat a whole extra asset pipeline on a surface only two people use.
    pub fn glyph(self) -> &'static str {
        match self {
            MobileTab::Routine => "✓",
            MobileTab::Calendar => "▦",
            MobileTab::Board => "✎",
            MobileTab::TvRemote => "▶",
            MobileTab::Settings => "⚙",
        }
    }
}

/// The phone surface: a header, one tab's content, and the bottom tab bar.
///
/// `pb-[env(safe-area-inset-bottom)]` and the matching `viewport-fit=cover`
/// meta in `client::app` are what keep the tab bar above the iPhone home
/// indicator once the PWA is installed and running without browser chrome.
#[component]
pub fn MobileShell() -> Element {
    let (bus, _sender) = use_realtime();
    let mut tab = use_signal(MobileTab::default);
    let mut toast = use_signal(|| Option::<String>::None);
    let mut queue_version = use_signal(|| 0u64);

    // Register the service worker once the shell is on screen. Idempotent,
    // and a silent no-op on an insecure origin (see `pwa.rs`).
    use_effect(pwa::register_service_worker);

    // Replay whatever is queued as soon as the socket is up — which is both
    // "the phone reconnected" (Android) and "the app was reopened" (iOS,
    // which has no Background Sync). One code path, two platforms.
    let connected = (bus.connected)();
    use_effect(move || {
        // Read inside the closure, not outside: that subscription is what
        // makes this effect re-run when the socket comes back, which is the
        // whole trigger for a replay.
        if !(bus.connected)() {
            return;
        }
        spawn(async move {
            let messages = flush_queue().await;
            queue_version += 1;
            if let Some(message) = messages {
                toast.set(Some(message));
            }
        });
    });

    rsx! {
        div { class: "flex min-h-screen w-full flex-col bg-sheffield-paper font-display text-slate-800",
            header { class: "sticky top-0 z-10 flex items-center justify-between bg-sheffield-dark px-4 py-3 text-white",
                h1 { class: "text-lg font-bold", "{tab().label()}" }
                ConnectionDot { connected }
            }

            if let Some(message) = toast() {
                div {
                    class: "mx-4 mt-3 rounded-2xl bg-sheffield-sun/30 px-4 py-3 text-sm font-semibold text-slate-800",
                    role: "status",
                    aria_live: "polite",
                    "{message}"
                    button {
                        class: "ml-3 font-bold text-slate-800",
                        aria_label: "Dismiss",
                        onclick: move |_| toast.set(None),
                        "×"
                    }
                }
            }

            main { class: "flex-1 overflow-y-auto p-4 pb-28",
                match tab() {
                    MobileTab::Routine => rsx! {
                        Routine { compact: true }
                    },
                    MobileTab::Calendar => rsx! {
                        CalendarPanel {}
                    },
                    MobileTab::Board => rsx! {
                        div { class: "h-[70vh] w-full overflow-hidden rounded-2xl bg-white shadow",
                            Whiteboard {}
                        }
                    },
                    MobileTab::TvRemote => rsx! {
                        TvRemote {}
                    },
                    MobileTab::Settings => rsx! {
                        MobileSettings {
                            queue_version: queue_version(),
                            on_retry: move |()| {
                                spawn(async move {
                                    let messages = flush_queue().await;
                                    queue_version += 1;
                                    toast
                                        .set(
                                            Some(
                                                messages
                                                    .unwrap_or_else(|| "Nothing was waiting to send.".into()),
                                            ),
                                        );
                                });
                            },
                        }
                    },
                }
            }

            nav {
                class: "fixed inset-x-0 bottom-0 z-20 grid grid-cols-5 border-t border-slate-200 bg-white pb-[env(safe-area-inset-bottom)]",
                aria_label: "Sections",
                for entry in MobileTab::ALL {
                    button {
                        key: "{entry.slug()}",
                        class: if entry == tab() { "flex flex-col items-center gap-0.5 px-1 py-2 text-xs font-bold text-sheffield-dark" } else { "flex flex-col items-center gap-0.5 px-1 py-2 text-xs font-semibold text-slate-600" },
                        aria_current: if entry == tab() { "page" } else { "false" },
                        onclick: move |_| tab.set(entry),
                        span { class: "text-xl leading-none", aria_hidden: "true", "{entry.glyph()}" }
                        span { "{entry.label()}" }
                    }
                }
            }
        }
    }
}

/// Load, expire and replay the offline queue, persist what is left, and
/// return the toast text if there is anything worth saying.
///
/// Split out of the component so the whole flow is one place: the component
/// only decides *when* to call it.
async fn flush_queue() -> Option<String> {
    let mut stored = OfflineQueue::load();
    if stored.is_empty() {
        return None;
    }
    let report = stored.replay(queue::now_ms(), queue::send_to_server).await;
    stored.save();

    // Expiry is the message that matters most — it is the only one that
    // reports a change the family made and the hub will never see.
    let expiry = report.toasts.iter().find(|toast| toast.is_expiry());
    let chosen: Option<&QueueToast> = expiry.or_else(|| report.toasts.first());
    chosen.map(QueueToast::message)
}

/// The connection state, as a **chip** rather than as coloured text.
///
/// T3.4: `sheffield-sun` on the `sheffield-dark` header was 3.4:1 and
/// `sheffield-accent` on it was 1.6:1 — the "Offline" word, the one word on
/// this surface that has to be legible when everything else has failed, was
/// the least legible thing on the screen. Flipping the hue to the *ground*
/// and putting `slate-800` on it gives 9.7:1 connected and 4.7:1 offline,
/// with the hue still doing the signalling (`palette::PALETTE_PAIRS`).
#[component]
fn ConnectionDot(connected: bool) -> Element {
    rsx! {
        span {
            class: if connected { "flex items-center gap-2 rounded-full bg-sheffield-sun px-3 py-1 text-xs font-bold text-slate-800" } else { "flex items-center gap-2 rounded-full bg-sheffield-accent px-3 py-1 text-xs font-bold text-slate-800" },
            role: "status",
            span {
                class: "h-2.5 w-2.5 rounded-full bg-slate-800",
                aria_hidden: "true",
            }
            if connected {
                "Connected"
            } else {
                "Offline"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bar_has_the_five_tabs_the_plan_names_in_order() {
        let labels: Vec<&str> = MobileTab::ALL.iter().map(|tab| tab.label()).collect();
        assert_eq!(
            labels,
            vec!["Routine", "Calendar", "Board", "TV Remote", "Settings"]
        );
    }

    #[test]
    fn every_tab_has_a_distinct_slug() {
        let mut slugs: Vec<&str> = MobileTab::ALL.iter().map(|tab| tab.slug()).collect();
        slugs.sort_unstable();
        let before = slugs.len();
        slugs.dedup();
        assert_eq!(before, slugs.len(), "tab slugs must be unique");
    }

    #[test]
    fn the_default_tab_is_the_routine() {
        assert_eq!(MobileTab::default(), MobileTab::Routine);
    }
}
