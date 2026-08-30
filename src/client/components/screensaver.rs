use dioxus::prelude::*;

use crate::client::app::use_app_state;
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
    if !active || images.is_empty() {
        return rsx! {};
    }

    let current = slide() % images.len();

    rsx! {
        div {
            class: "fixed inset-0 z-50 bg-black",
            onclick: move |_| is_idle.set(false),
            onpointerdown: move |_| is_idle.set(false),
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
