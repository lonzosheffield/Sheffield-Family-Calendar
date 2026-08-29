use dioxus::prelude::*;

use crate::server::api::list_screensaver_images;

/// Idle timeout before the ambient slideshow takes over the kiosk.
pub const IDLE_TIMEOUT_SECS: u64 = 10 * 60;
/// How often idleness is re-evaluated.
pub const IDLE_TICK_SECS: u64 = 15;
/// How long each screensaver photo stays on screen.
pub const SLIDE_SECS: u64 = 12;

#[component]
pub fn Screensaver() -> Element {
    let mut is_idle = use_signal(|| false);
    let activity = use_signal(|| 0_u64);
    let mut slide = use_signal(|| 0_usize);

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
        let mut idle_ticks = 0_u64;
        loop {
            platform::sleep(IDLE_TICK_SECS).await;
            let current = activity.peek().to_owned();
            if current == last_seen {
                idle_ticks += IDLE_TICK_SECS;
                if idle_ticks >= IDLE_TIMEOUT_SECS && !is_idle() {
                    is_idle.set(true);
                }
            } else {
                last_seen = current;
                idle_ticks = 0;
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

    if !is_idle() || images.is_empty() {
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
