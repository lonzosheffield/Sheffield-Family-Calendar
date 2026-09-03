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
//! | `session.rs` | the parent session cookie — probe, sign in, first-run setup, sign out (Q2-02, closing `docs/HANDOFF.md` H-19/H-25) |
//! | `remote.rs` | the Remote tab — `SetView` / `SetActiveProfile` |
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
use crate::client::components::glyphs;
use crate::client::components::homeschool::School;
use crate::client::components::mobile::queue::{OfflineQueue, QueueToast};
use crate::client::components::mobile::remote::TvRemote;
use crate::client::components::mobile::session::SessionState;
use crate::client::components::mobile::settings::MobileSettings;
use crate::client::components::routine::Routine;
use crate::client::components::whiteboard::Whiteboard;
use crate::client::realtime::use_realtime;

/// The six bottom tabs, in bar order (`docs/homeschool/PLAN_HOMESCHOOL.md`
/// §2 H6: `Routine · School · Calendar · Board · Remote · Settings`).
///
/// **School sits second**, not last: the owner opens the phone in the morning
/// to see what the boys are doing today, and the second column is the one a
/// thumb reaches without looking. The sixth column is paid for by relabelling
/// *TV Remote* to **Remote** rather than by inventing a phone type size the
/// design system froze (`docs/design/DESIGN_DIRECTION.md` §2.1, review finding
/// R-15) — [`mobile_tab_budget_px`] is the arithmetic that proves it fits.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub enum MobileTab {
    #[default]
    Routine,
    /// The School ("house") tab — HS5.
    Homeschool,
    Calendar,
    Board,
    TvRemote,
    Settings,
}

impl MobileTab {
    /// Every tab, in the order they appear in the bar.
    pub const ALL: [MobileTab; 6] = [
        MobileTab::Routine,
        MobileTab::Homeschool,
        MobileTab::Calendar,
        MobileTab::Board,
        MobileTab::TvRemote,
        MobileTab::Settings,
    ];

    /// The label under the icon — and the accessible name of the button.
    pub fn label(self) -> &'static str {
        match self {
            MobileTab::Routine => "Routine",
            MobileTab::Homeschool => "School",
            MobileTab::Calendar => "Calendar",
            MobileTab::Board => "Board",
            // H6 / R-15: six columns at 360 px leave 60 px each, and the old
            // two-word label needs 70 — see `mobile_tab_budget_px`'s tests.
            MobileTab::TvRemote => "Remote",
            MobileTab::Settings => "Settings",
        }
    }

    /// Stable identifier, used for `key`s and test assertions.
    pub fn slug(self) -> &'static str {
        match self {
            MobileTab::Routine => "routine",
            MobileTab::Homeschool => "school",
            MobileTab::Calendar => "calendar",
            MobileTab::Board => "board",
            MobileTab::TvRemote => "tv-remote",
            MobileTab::Settings => "settings",
        }
    }

    /// A glyph rather than an icon font or an SVG sprite: the shared poster
    /// mapping (`glyphs::icon_glyph`'s panel/tab siblings, D4.2 /
    /// DESIGN_DIRECTION.md §2.5) beats a whole extra asset pipeline on a
    /// surface only two people use.
    pub fn glyph(self) -> &'static str {
        match self {
            MobileTab::Routine => glyphs::ROUTINE_GLYPH,
            MobileTab::Homeschool => glyphs::HOMESCHOOL_GLYPH,
            MobileTab::Calendar => glyphs::CALENDAR_GLYPH,
            MobileTab::Board => glyphs::WHITEBOARD_GLYPH,
            MobileTab::TvRemote => glyphs::TV_REMOTE_GLYPH,
            MobileTab::Settings => glyphs::SETTINGS_GLYPH,
        }
    }
}

// ---------------------------------------------------------------------------
// HS5 / R-15 — the tab bar's horizontal budget
// ---------------------------------------------------------------------------
//
// Nothing here paints anything. It is the *arithmetic* of the bar's classes,
// written down so that "do six labels still fit across the narrowest phone?"
// is a `cargo test` question rather than a screenshot question — the same
// trick `tv::style::tv_rail_budget_px()` plays for the kiosk's profile rail
// (`docs/design/DESIGN_DIRECTION.md` §2.7).
//
// The sixth tab was the one design risk the red team raised (R-15): the
// obvious fix, an 11 px type size, would have invented a phone size §2.1
// freezes. Relabelling *TV Remote* to *Remote* costs nothing and is what the
// numbers below show to be sufficient.

/// The narrowest viewport the phone surface is designed for, in CSS pixels —
/// an iPhone SE, and the floor of essentially every Android in the house.
pub const MOBILE_MIN_VIEWPORT_PX: u32 = 360;

/// Columns in the bar's `grid-cols-6`, straight from the tab list so the two
/// can never disagree.
pub const MOBILE_TAB_COLUMNS: u32 = MobileTab::ALL.len() as u32;

/// One step of Tailwind's spacing scale: `px-1` is 1 × 4 px a side.
pub const MOBILE_SPACING_STEP_PX: u32 = 4;

/// `px-1` on every tab button.
pub const MOBILE_TAB_PADDING_X_STEP: u32 = 1;

/// `text-xs` — the bar's type size, unchanged by the sixth tab.
pub const MOBILE_TAB_FONT_PX: u32 = 12;

/// Nunito Bold advance widths in thousandths of an em, for the characters the
/// six labels are made of.
///
/// Measured from the bundled `assets/fonts/nunito-800-latin.woff2` face at
/// its 1000-unit em, which is the face `font-display` + `font-bold` resolves
/// to in the bar. Anything not listed falls back to the widest capital, so an
/// unmeasured character can only make the budget *more* pessimistic.
const MOBILE_TAB_ADVANCES: [(char, u32); 22] = [
    (' ', 260),
    ('B', 660),
    ('C', 660),
    ('R', 660),
    ('S', 610),
    ('T', 610),
    ('V', 660),
    ('a', 550),
    ('c', 520),
    ('d', 590),
    ('e', 550),
    ('g', 570),
    ('h', 590),
    ('i', 270),
    ('l', 270),
    ('m', 880),
    ('n', 590),
    ('o', 590),
    ('r', 390),
    ('s', 470),
    ('t', 390),
    ('u', 590),
];

/// The widest capital in [`MOBILE_TAB_ADVANCES`] — what an unmeasured
/// character is charged.
const MOBILE_TAB_FALLBACK_ADVANCE: u32 = 660;

/// How wide `label` renders in the tab bar, in CSS pixels, rounded up.
pub fn mobile_label_width_px(label: &str) -> u32 {
    let em_thousandths: u32 = label
        .chars()
        .map(|c| {
            MOBILE_TAB_ADVANCES
                .iter()
                .find(|(candidate, _)| *candidate == c)
                .map_or(MOBILE_TAB_FALLBACK_ADVANCE, |(_, advance)| *advance)
        })
        .sum();
    (em_thousandths * MOBILE_TAB_FONT_PX).div_ceil(1000)
}

/// The pixels one column of the bar gets on the narrowest supported phone.
pub const fn mobile_tab_column_px() -> u32 {
    MOBILE_MIN_VIEWPORT_PX / MOBILE_TAB_COLUMNS
}

/// The pixels the **widest** tab label actually needs, padding included.
///
/// HS5 accept (a): this must stay at or under [`mobile_tab_column_px`] (60 px)
/// or a label wraps — or worse, is clipped — on a 360 px phone.
pub fn mobile_tab_budget_px() -> u32 {
    let padding = 2 * MOBILE_TAB_PADDING_X_STEP * MOBILE_SPACING_STEP_PX;
    MobileTab::ALL
        .iter()
        .map(|tab| mobile_label_width_px(tab.label()) + padding)
        .max()
        .unwrap_or(0)
}

/// The bottom tab bar, split out of [`MobileShell`] so it can be rendered on
/// its own in an SSR test (HS5 accept (a)) the way `RoutineRow` already is —
/// it takes plain props and reads no app context.
#[component]
pub fn MobileTabBar(active: MobileTab, on_select: EventHandler<MobileTab>) -> Element {
    rsx! {
        nav {
            class: "fixed inset-x-0 bottom-0 z-20 grid grid-cols-6 border-t border-slate-200 bg-white pb-[env(safe-area-inset-bottom)]",
            aria_label: "Sections",
            for entry in MobileTab::ALL {
                button {
                    key: "{entry.slug()}",
                    class: if entry == active { "flex flex-col items-center gap-0.5 px-1 py-2 text-xs font-bold text-sheffield-dark" } else { "flex flex-col items-center gap-0.5 px-1 py-2 text-xs font-semibold text-slate-600" },
                    aria_current: if entry == active { "page" } else { "false" },
                    "data-mobile-tab": "{entry.slug()}",
                    onclick: move |_| on_select.call(entry),
                    span { class: "text-xl leading-none", aria_hidden: "true", "{entry.glyph()}" }
                    span { "{entry.label()}" }
                }
            }
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

    // Q2-02: the parent session is an `HttpOnly` cookie the page can never
    // read, so "is this phone signed in, and has this hub ever been set up?"
    // is a question only the server can answer. One probe on mount, cached in
    // a context signal every tab reads through `session::state()` /
    // `session::is_parent()`, and re-written by the Settings tab after a
    // sign-in, a first-run setup or a sign-out.
    let mut session = use_context_provider(|| Signal::new(Option::<SessionState>::None));
    use_future(move || async move {
        if let Some(state) = session::probe().await {
            session.set(Some(state));
        }
    });

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
                // §3.5: "`☀️ Sheffield Hub` beside the tab label,
                // `font-poster`" — both spans inherit the header's own
                // `text-white` ink, so this adds no new palette pair.
                div { class: "flex items-baseline gap-2",
                    h1 { class: "font-poster text-lg font-bold",
                        span { aria_hidden: "true", "{glyphs::ROUTINE_GLYPH} " }
                        "Sheffield Hub"
                    }
                    span { class: "text-sm font-semibold", "{tab().label()}" }
                }
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
                    MobileTab::Homeschool => rsx! {
                        School {}
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

            MobileTabBar { active: tab(), on_select: move |entry| tab.set(entry) }
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
    fn the_bar_has_the_six_tabs_the_plan_names_in_order() {
        let labels: Vec<&str> = MobileTab::ALL.iter().map(|tab| tab.label()).collect();
        assert_eq!(
            labels,
            vec!["Routine", "School", "Calendar", "Board", "Remote", "Settings"]
        );
    }

    #[test]
    fn school_is_the_second_tab_and_wears_the_house_glyph() {
        assert_eq!(MobileTab::ALL[1], MobileTab::Homeschool);
        assert_eq!(MobileTab::Homeschool.glyph(), glyphs::HOMESCHOOL_GLYPH);
        assert_eq!(MobileTab::Homeschool.label(), "School");
    }

    #[test]
    fn the_widest_tab_label_fits_one_column_of_the_narrowest_phone() {
        assert_eq!(MobileTab::ALL.len(), 6);
        assert_eq!(mobile_tab_column_px(), 60);
        let budget = mobile_tab_budget_px();
        assert!(
            budget <= mobile_tab_column_px(),
            "the widest tab label needs {budget}px but a column is only {}px",
            mobile_tab_column_px()
        );
        // The clause HS5 (a) states in absolute terms, not merely relative to
        // the column arithmetic above.
        assert!(budget <= 60, "the tab budget is {budget}px, over 60");
    }

    #[test]
    fn the_old_two_word_remote_label_is_what_would_not_have_fitted() {
        // R-15's actual finding, kept as a live assertion: if someone renames
        // the tab back, this fails before a phone does.
        let padding = 2 * MOBILE_TAB_PADDING_X_STEP * MOBILE_SPACING_STEP_PX;
        let old = mobile_label_width_px("TV Remote") + padding;
        assert!(
            old > mobile_tab_column_px(),
            "`TV Remote` was supposed to be the label that overflows, and it needs {old}px"
        );
        assert!(mobile_label_width_px("Remote") < mobile_label_width_px("TV Remote"));
    }

    #[test]
    fn an_unmeasured_character_is_charged_the_widest_capital() {
        // "more pessimistic, never less" — an emoji or an accent in a future
        // label must not be able to shrink the budget below the truth.
        assert_eq!(
            mobile_label_width_px("W"),
            (MOBILE_TAB_FALLBACK_ADVANCE * MOBILE_TAB_FONT_PX).div_ceil(1000)
        );
        assert_eq!(mobile_label_width_px(""), 0);
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
