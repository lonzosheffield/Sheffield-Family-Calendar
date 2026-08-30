//! The phone surface's `localStorage` shim.
//!
//! One thing outlives a page load on a phone through this shim: the offline
//! mutation queue ([`super::queue`]). It is a small string, it must survive
//! the PWA being swiped away, and it may never reach the TV origin — which
//! is exactly `localStorage`'s shape. (The parent session used to live here
//! too; QA round 2's Q2-02 moved it to the server's `HttpOnly` cookie, which
//! outlives the page load on its own and is out of reach of any script —
//! see [`super::session`].)
//!
//! **Why hand-written `wasm_bindgen` imports rather than `web_sys::Storage`:**
//! `Storage` is not in `Cargo.toml`'s `web-sys` feature list and `Cargo.toml`
//! is not a T2.2-owned file (`docs/reviews/PURPLE_TEAM.md` §P4 — a crate or
//! feature addition is a Boss micro-commit between waves). These four
//! `extern "C"` declarations are ordinary Rust source using the already-
//! declared `wasm-bindgen` glue exception in `docs/NON_RUST.md`; a request to
//! swap them for `web-sys`'s `Storage` is filed in `docs/HANDOFF.md`.
//!
//! Every accessor is fallible on purpose. Safari in private mode makes
//! `window.localStorage` *throw* on access, and a phone with site data
//! disabled has no storage at all — a hub that panicked there would be a hub
//! that cannot be used by one of the two parents.

/// Read `key`, or `None` if it is unset or storage is unavailable.
pub fn get(key: &str) -> Option<String> {
    imp::get(key)
}

/// Write `key`. Silently does nothing when storage is unavailable — the
/// caller's in-memory state stays authoritative for the life of the tab.
pub fn set(key: &str, value: &str) {
    imp::set(key, value);
}

/// Remove `key`. Silent on unavailable storage, like [`set`].
pub fn remove(key: &str) {
    imp::remove(key);
}

#[cfg(all(feature = "web", target_arch = "wasm32"))]
mod imp {
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = ["window", "localStorage"], js_name = getItem, catch)]
        fn get_item(key: &str) -> Result<Option<String>, JsValue>;

        #[wasm_bindgen(js_namespace = ["window", "localStorage"], js_name = setItem, catch)]
        fn set_item(key: &str, value: &str) -> Result<(), JsValue>;

        #[wasm_bindgen(js_namespace = ["window", "localStorage"], js_name = removeItem, catch)]
        fn remove_item(key: &str) -> Result<(), JsValue>;
    }

    pub fn get(key: &str) -> Option<String> {
        get_item(key).ok().flatten()
    }

    pub fn set(key: &str, value: &str) {
        let _ = set_item(key, value);
    }

    pub fn remove(key: &str) {
        let _ = remove_item(key);
    }
}

/// Server-side rendering and every non-wasm build (including `cargo test`)
/// get a process-local map instead of a browser's storage.
///
/// It is not a stub that throws away writes: SSR renders the phone shell
/// once before hydration takes over, and a queue that read `None` there and
/// something else a moment later would flash the wrong badge count. A plain
/// map keeps the two passes consistent, and keeps [`super::queue`]'s
/// persistence testable without a browser.
#[cfg(not(all(feature = "web", target_arch = "wasm32")))]
mod imp {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    fn store() -> &'static Mutex<HashMap<String, String>> {
        static STORE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
        STORE.get_or_init(|| Mutex::new(HashMap::new()))
    }

    pub fn get(key: &str) -> Option<String> {
        store()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .get(key)
            .cloned()
    }

    pub fn set(key: &str, value: &str) {
        store()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .insert(key.to_string(), value.to_string());
    }

    pub fn remove(key: &str) {
        store()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_round_trips_and_can_be_removed() {
        const KEY: &str = "familyhub.storage.round-trip-test";
        assert_eq!(get(KEY), None);
        set(KEY, "hello");
        assert_eq!(get(KEY).as_deref(), Some("hello"));
        remove(KEY);
        assert_eq!(get(KEY), None);
    }
}
