use dioxus::prelude::*;

use crate::client::app::use_app_state;
use crate::client::components::glyphs;
use crate::server::api::list_screensaver_images;
use crate::shared::types::MaximizedView;

/// Idle timeout before the ambient slideshow takes over the kiosk.
pub const IDLE_TIMEOUT_SECS: u64 = 10 * 60;
/// How often idleness is re-evaluated.
pub const IDLE_TICK_SECS: u64 = 15;
/// How long each screensaver photo stays on screen.
pub const SLIDE_SECS: u64 = 12;

/// The idle-timeout state machine, decoupled from any timer or platform so
/// it is unit testable without wasm or a browser (T2.7 acceptance (c): "the
/// idle-timeout state machine fires at 600 s in a unit test").
///
/// [`Screensaver`] drives this with one `tick_idle` call per
/// [`IDLE_TICK_SECS`] of silence and one `record_activity` call whenever the
/// activity counter it watches changes; the ticking and the activity watch
/// stay in the component (they need real time and, on `web`, real DOM
/// events), but the *decision* of when 600 s of silence becomes "idle" lives
/// here, where a test can drive it directly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IdleTracker {
    seconds_since_activity: u64,
    is_idle: bool,
}

impl IdleTracker {
    /// Record `elapsed_secs` of observed silence and return the (possibly
    /// newly) idle state.
    pub fn tick_idle(&mut self, elapsed_secs: u64) -> bool {
        self.seconds_since_activity = self.seconds_since_activity.saturating_add(elapsed_secs);
        if self.seconds_since_activity >= IDLE_TIMEOUT_SECS {
            self.is_idle = true;
        }
        self.is_idle
    }

    /// Activity was observed: reset the clock and clear idle state.
    pub fn record_activity(&mut self) {
        self.seconds_since_activity = 0;
        self.is_idle = false;
    }

    pub fn is_idle(&self) -> bool {
        self.is_idle
    }
}

/// QA round 1, Q1-14: whether an activity event, observed while the
/// **scheduled** overlay (`MaximizedView::Screensaver`, forced on by
/// `ScreensaverSchedule` regardless of idleness) is showing, should clear
/// it back to `MaximizedView::None`.
///
/// This is the remote's only way off a scheduled overlay: the two
/// `onclick`/`onpointerdown` handlers this component used to carry are
/// gone (the television has no pointer — `tests/tv_tests.rs`'s
/// `the_kiosk_never_reaches_for_a_pointer_event` now scans this file too),
/// so a remote key press reaching the window-level `keydown` listener
/// (`platform::watch_activity`) and bumping the `activity` signal is the
/// only remaining path. Pure and synchronous so it is unit testable without
/// wasm or a browser, the same way [`IdleTracker`] is.
pub fn view_after_activity(current_view: MaximizedView) -> MaximizedView {
    if current_view == MaximizedView::Screensaver {
        MaximizedView::None
    } else {
        current_view
    }
}

#[component]
pub fn Screensaver() -> Element {
    let mut is_idle = use_signal(|| false);
    let activity = use_signal(|| 0_u64);
    let mut slide = use_signal(|| 0_usize);
    // T2.7: the optional schedule (off by default, PURPLE §P5.5 default 20)
    // forces the overlay on regardless of idleness by broadcasting
    // `ServerMessage::SetView { view: MaximizedView::Screensaver }`, which
    // `tv::shell`'s WS handler already threads into `AppState::current_view`
    // the same way it does every other `SetView`.
    let state = use_app_state();
    let scheduled_on = (state.current_view)() == MaximizedView::Screensaver;

    let images =
        use_resource(|| async move { list_screensaver_images().await.unwrap_or_default() });
    let images = match &*images.read_unchecked() {
        Some(list) => list.clone(),
        None => Vec::new(),
    };

    // Any touch, click or key press anywhere counts as activity.
    use_hook(|| platform::watch_activity(activity));

    use_future(move || async move {
        let mut last_seen = activity.peek().to_owned();
        let mut tracker = IdleTracker::default();
        loop {
            platform::sleep(IDLE_TICK_SECS).await;
            let current = activity.peek().to_owned();
            if current == last_seen {
                let now_idle = tracker.tick_idle(IDLE_TICK_SECS);
                if now_idle != is_idle() {
                    is_idle.set(now_idle);
                }
            } else {
                last_seen = current;
                tracker.record_activity();
                if is_idle() {
                    is_idle.set(false);
                }
                // QA round 1, Q1-14: real activity (a remote key press,
                // fed through the window-level `keydown` listener in
                // `platform::watch_activity`) also dismisses a
                // schedule-forced overlay, which `is_idle` alone does not
                // control (`active = is_idle() || scheduled_on` below).
                // Without this, a family that opted into the schedule had
                // no way to clear it except another `SetView` from a phone.
                let mut current_view = state.current_view;
                let before = current_view.peek().to_owned();
                let after = view_after_activity(before);
                if after != before {
                    current_view.set(after);
                }
            }
        }
    });

    use_future(move || async move {
        loop {
            platform::sleep(SLIDE_SECS).await;
            if is_idle() {
                let next = slide().wrapping_add(1);
                slide.set(next);
            }
        }
    });

    let active = is_idle() || scheduled_on;
    if !active {
        return rsx! {};
    }

    // D4.2 / §3.3: the caption chip renders as soon as the overlay is
    // active, independent of whether any photo has loaded yet — the family
    // should still see whose hub this is on a bare black screen, not only
    // once a `list_screensaver_images` fetch resolves. `current` only
    // matters when there is something to crossfade.
    let current = if images.is_empty() {
        0
    } else {
        slide() % images.len()
    };

    rsx! {
        div {
            // QA round 1, Q1-14: the two `onclick`/`onpointerdown` dismiss
            // handlers this div used to carry are gone — the television has
            // no pointer, and they were the only way `active` (`is_idle() ||
            // scheduled_on`) could be true with no route back for a
            // schedule-forced overlay from the remote. Dismissal now runs
            // entirely off the `activity` signal both idle detection and
            // [`view_after_activity`] already watch: any `keydown` (a
            // remote press) already resets `is_idle` above, and clears a
            // scheduled overlay via `state.current_view` in the same tick.
            class: "fixed inset-0 z-50 bg-black",
            for (index, image) in images.iter().enumerate() {
                img {
                    key: "{image}",
                    class: if index == current {
                        "absolute inset-0 h-full w-full object-cover opacity-100 transition-opacity duration-[2000ms]"
                    } else {
                        "absolute inset-0 h-full w-full object-cover opacity-0 transition-opacity duration-[2000ms]"
                    },
                    src: "{image}",
                    alt: "",
                }
            }
            // §3.3: bottom-left, inside the 5% overscan band, a *solid*
            // ground (never translucent over a photo) — `text-white` on
            // `bg-slate-800` is already a declared, passing palette pair
            // (`palette::PALETTE_PAIRS`, "the key-code overlay's headings"),
            // so this chip adds no new colour.
            div {
                class: "absolute bottom-[5%] left-[5%] flex items-center gap-2 rounded-full bg-slate-800 px-6 py-2 font-poster text-3xl text-white",
                span { aria_hidden: "true", "{glyphs::ROUTINE_GLYPH}" }
                "Sheffield Family Hub"
            }
        }
    }
}

#[cfg(all(feature = "web", target_arch = "wasm32"))]
mod platform {
    use dioxus::prelude::*;
    use wasm_bindgen::prelude::Closure;
    use wasm_bindgen::JsCast;

    pub async fn sleep(seconds: u64) {
        gloo_timers::future::TimeoutFuture::new((seconds * 1000) as u32).await;
    }

    pub fn watch_activity(activity: Signal<u64>) {
        let Some(window) = web_sys::window() else {
            return;
        };

        for event in ["pointerdown", "keydown", "touchstart", "wheel"] {
            let mut activity = activity;
            let closure = Closure::<dyn FnMut()>::new(move || {
                let next = activity.peek().wrapping_add(1);
                activity.set(next);
            });
            let _ =
                window.add_event_listener_with_callback(event, closure.as_ref().unchecked_ref());
            closure.forget();
        }
    }
}

#[cfg(not(all(feature = "web", target_arch = "wasm32")))]
mod platform {
    use dioxus::prelude::*;

    /// On the server the screensaver never activates, so this future parks.
    pub async fn sleep(_seconds: u64) {
        std::future::pending::<()>().await
    }

    pub fn watch_activity(_activity: Signal<u64>) {}
}

#[cfg(test)]
mod idle_tracker_tests {
    use super::*;

    /// T2.7 acceptance (c): "the idle-timeout state machine fires at 600 s
    /// in a unit test." [`Screensaver`] ticks in [`IDLE_TICK_SECS`] (15 s)
    /// increments, so this drives the tracker the same way: not idle before
    /// the last tick that crosses 600 s, idle exactly on it.
    #[test]
    fn idle_tracker_fires_at_exactly_600_seconds() {
        assert_eq!(
            IDLE_TIMEOUT_SECS, 600,
            "this test's step count assumes the 600 s timeout named in the acceptance test"
        );
        let ticks_to_timeout = IDLE_TIMEOUT_SECS / IDLE_TICK_SECS;
        assert_eq!(
            ticks_to_timeout * IDLE_TICK_SECS,
            IDLE_TIMEOUT_SECS,
            "IDLE_TICK_SECS must evenly divide IDLE_TIMEOUT_SECS for this test to land on 600s exactly"
        );

        let mut tracker = IdleTracker::default();
        assert!(!tracker.is_idle());

        for tick in 1..ticks_to_timeout {
            assert!(
                !tracker.tick_idle(IDLE_TICK_SECS),
                "must not be idle yet at {} s (tick {tick})",
                tick * IDLE_TICK_SECS
            );
        }
        assert!(
            tracker.tick_idle(IDLE_TICK_SECS),
            "must be idle once {IDLE_TIMEOUT_SECS} s of silence have elapsed"
        );
        assert!(tracker.is_idle());
    }

    #[test]
    fn a_single_tick_past_the_timeout_still_fires() {
        let mut tracker = IdleTracker::default();
        assert!(tracker.tick_idle(IDLE_TIMEOUT_SECS + 1));
    }

    #[test]
    fn activity_resets_the_idle_clock() {
        let mut tracker = IdleTracker::default();
        assert!(tracker.tick_idle(IDLE_TIMEOUT_SECS));
        assert!(tracker.is_idle());

        tracker.record_activity();
        assert!(!tracker.is_idle());

        // A fresh 600 s must elapse again from the reset point.
        assert!(!tracker.tick_idle(IDLE_TIMEOUT_SECS - IDLE_TICK_SECS));
        assert!(tracker.tick_idle(IDLE_TICK_SECS));
    }
}

/// QA round 1, Q1-14: "the remote cannot clear [a scheduled overlay]" —
/// these cover [`view_after_activity`], the pure function that now does,
/// wired into [`Screensaver`]'s activity-tracking `use_future`.
#[cfg(test)]
mod view_after_activity_tests {
    use super::*;

    #[test]
    fn activity_clears_a_scheduled_overlay() {
        assert_eq!(
            view_after_activity(MaximizedView::Screensaver),
            MaximizedView::None
        );
    }

    #[test]
    fn activity_leaves_every_other_view_untouched() {
        for view in [
            MaximizedView::None,
            MaximizedView::Routine,
            MaximizedView::Calendar,
            MaximizedView::Whiteboard,
            // HS6: the School panel is a view a phone can push, so activity
            // must leave it alone exactly as it leaves the other three.
            MaximizedView::Homeschool,
        ] {
            assert_eq!(
                view_after_activity(view),
                view,
                "activity must not disturb a view the schedule did not set"
            );
        }
    }
}
