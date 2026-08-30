//! The parent session on the phone surface — an `HttpOnly` cookie the page's
//! own script can never read, probed rather than stored.
//!
//! **QA round 2, Q2-02** replaced what used to live here. Until this change
//! the phone kept a bearer session UUID in `localStorage` and threaded it
//! through every WebSocket send and every privileged `#[server]` fn, which
//! contradicted PLAN v2 §3 T1.4 / §P5.5 default 31 ("30-day session,
//! `HttpOnly`/`Secure`/`SameSite=Lax` cookie") and left the cookie the server
//! had already been minting since QA round 1 (Q1-11) unused by the only
//! client that exists. `docs/HANDOFF.md` H-19 and H-25 are the two requests
//! this module closes.
//!
//! The shape now:
//!
//! | fn | HTTP | why |
//! | --- | --- | --- |
//! | [`probe`] | `GET /api/session` | 204 parent · 404 first run · anything else signed out |
//! | [`login`] | `POST /api/login` | the six-digit PIN; the browser stores the `Set-Cookie` |
//! | [`setup`] | `POST /api/setup` | first run only: setup code + the new PIN |
//! | [`logout`] | `POST /api/logout` | revokes the session **server-side**, then expires the cookie |
//!
//! A script can never read an `HttpOnly` cookie's value, so "is this phone
//! signed in?" is not a local question: it is [`probe`]'s answer, cached in a
//! [`SessionSignal`] that `MobileShell` provides through context and every
//! component reads with [`state`] / [`is_parent`]. Nothing about the session
//! is persisted by this code any more — the cookie outlives the page load on
//! its own, for the 30 days the server gave it, and a `localStorage` value
//! that XSS could read no longer exists to steal.
//!
//! Every request goes out with `credentials: 'same-origin'` so the cookie
//! rides along; the same-origin rule the server applies to `/api/login` and
//! `/api/setup` (`auth::same_origin_or_absent`) is what stops another site
//! from driving these routes with the family's ambient cookie.

use dioxus::prelude::*;

/// What `GET /api/session` last said about this phone.
///
/// `FirstRun` is the state the hub is in before anyone has ever set a parent
/// PIN: `/api/session` answers `404` (no PIN on record), and Settings offers
/// the setup form instead of the sign-in form. Without this state the owner
/// could not complete `docs/OWNER_CHECKLIST.md` step 4 from any UI at all —
/// which is exactly what Q2-02 found.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SessionState {
    /// No parent PIN has ever been set on this hub.
    FirstRun,
    /// A PIN exists; this phone does not hold a live session.
    SignedOut,
    /// This phone holds a valid parent session cookie.
    Parent,
}

impl SessionState {
    /// Does this state authorise the parent-only affordances?
    pub fn is_parent(self) -> bool {
        matches!(self, SessionState::Parent)
    }
}

/// The context type [`crate::client::components::mobile::MobileShell`]
/// provides. `None` means "not probed yet" — distinct from `SignedOut`, so
/// the first render never flashes a sign-in form at a signed-in parent.
pub type SessionSignal = Signal<Option<SessionState>>;

/// The signal `MobileShell` provided, if this code is running inside a
/// Dioxus scope that has one above it.
///
/// Deliberately `try_consume_context` and not the `try_use_context` **hook**:
/// this is reached from inside `rsx!` conditionals (`routine.rs`'s photo-task
/// dialog, for one), where a hook would break the rules of hooks. Reading the
/// signal afterwards still subscribes the calling reactive scope, so a
/// component re-renders when the probe lands.
///
/// The two `try_*` guards are what keep a plain `#[test]` (no `VirtualDom`,
/// no runtime) from panicking inside `Runtime::current()`.
fn signal() -> Option<SessionSignal> {
    let runtime = dioxus::core::Runtime::try_current()?;
    runtime.try_current_scope_id()?;
    try_consume_context::<SessionSignal>()
}

/// The current session state, or `None` if the probe has not answered yet
/// (or this component is not under `MobileShell`).
pub fn state() -> Option<SessionState> {
    signal().and_then(|signal| signal())
}

/// `true` when this phone holds a parent session.
pub fn is_parent() -> bool {
    matches!(state(), Some(SessionState::Parent))
}

/// Ask the hub what this phone's cookie is worth.
///
/// `None` means the request itself failed (offline, or the hub is down) —
/// the caller keeps whatever it already had rather than downgrading a
/// signed-in parent to "signed out" because the Wi-Fi dropped.
pub async fn probe() -> Option<SessionState> {
    match http::status("GET", "/api/session", None).await? {
        204 => Some(SessionState::Parent),
        404 => Some(SessionState::FirstRun),
        _ => Some(SessionState::SignedOut),
    }
}

/// Sign in with the six-digit PIN. `true` when the hub set the cookie.
pub async fn login(pin: &str) -> bool {
    let Some(pin) = json_digits(pin) else {
        return false;
    };
    http::status("POST", "/api/login", Some(&format!(r#"{{"pin":"{pin}"}}"#))).await == Some(200)
}

/// First run only: turn the setup code printed in the hub's log (and written
/// to `<data>\setup-code.txt`) into the family's first parent PIN. `true`
/// when the hub accepted it and set the cookie.
pub async fn setup(setup_code: &str, pin: &str) -> bool {
    let (Some(setup_code), Some(pin)) = (json_digits(setup_code), json_digits(pin)) else {
        return false;
    };
    http::status(
        "POST",
        "/api/setup",
        Some(&format!(r#"{{"setup_code":"{setup_code}","pin":"{pin}"}}"#)),
    )
    .await
        == Some(200)
}

/// Sign out — the server revokes the session, then expires the cookie. Unlike
/// the `localStorage` `clear()` this replaces, this is a real revocation: a
/// phone that is lost after a sign-out cannot be un-signed-out by restoring a
/// backup of its storage.
pub async fn logout() {
    let _ = http::status("POST", "/api/logout", None).await;
}

/// Both bodies this module sends are built by `format!` into a JSON string
/// literal, so every value that reaches one has to be free of anything that
/// could end the string. The setup code and the PIN are both six digits
/// (`auth::is_valid_pin_format`, `auth::generate_six_digit_code`), so
/// "digits only, non-empty" is the whole rule — and a value that fails it
/// would have been refused by the server anyway.
fn json_digits(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty() && value.chars().all(|c| c.is_ascii_digit())).then_some(value)
}

/// The one browser call this module needs: a `fetch` that carries the
/// same-origin cookie and reports back only the status code.
///
/// **Why a `wasm_bindgen(inline_js)` snippet rather than `web-sys`:** the
/// same reason `routine.rs`'s upload snippet gives — `Request`, `Response`,
/// `RequestInit` and `RequestCredentials` are four more `web-sys` features on
/// a `Cargo.toml` that is Boss-serialised (`docs/reviews/PURPLE_TEAM.md`
/// §P4), and `credentials: 'same-origin'` is the single property that makes
/// the cookie ride along. Declared on the existing `inline_js` row of
/// `docs/NON_RUST.md`.
mod http {
    /// Perform the request, returning the HTTP status, or `None` if the
    /// request never completed.
    pub async fn status(method: &str, url: &str, body: Option<&str>) -> Option<u16> {
        imp::status(method, url, body).await
    }

    #[cfg(all(feature = "web", target_arch = "wasm32"))]
    mod imp {
        use wasm_bindgen::prelude::*;

        #[wasm_bindgen(inline_js = r#"
export async function family_hub_session_fetch(method, url, body) {
    try {
        const init = {
            method,
            credentials: 'same-origin',
            headers: { 'content-type': 'application/json' },
        };
        if (body !== undefined && body !== null) {
            init.body = body;
        }
        const response = await fetch(url, init);
        return response.status;
    } catch (err) {
        console.error('family hub session request failed', err);
        return 0;
    }
}
"#)]
        extern "C" {
            async fn family_hub_session_fetch(
                method: String,
                url: String,
                body: Option<String>,
            ) -> u16;
        }

        pub async fn status(method: &str, url: &str, body: Option<&str>) -> Option<u16> {
            let status = family_hub_session_fetch(
                method.to_string(),
                url.to_string(),
                body.map(str::to_string),
            )
            .await;
            // `0` is the snippet's "the fetch threw" sentinel — no browser
            // ever reports it as a real status.
            (status != 0).then_some(status)
        }
    }

    /// Server-side rendering and every non-wasm build (including `cargo
    /// test`) never run one of these: SSR has no cookie jar of its own and
    /// the probe re-runs on the client after hydration. Same `imp` split as
    /// `mobile::storage` and `routine::upload`.
    #[cfg(not(all(feature = "web", target_arch = "wasm32")))]
    mod imp {
        pub async fn status(_method: &str, _url: &str, _body: Option<&str>) -> Option<u16> {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_parent_authorises_the_parent_only_affordances() {
        assert!(SessionState::Parent.is_parent());
        assert!(!SessionState::SignedOut.is_parent());
        assert!(!SessionState::FirstRun.is_parent());
    }

    #[test]
    fn is_parent_is_false_with_no_shell_above_it() {
        // No `MobileShell` and no Dioxus runtime at all: the phone's
        // parent-only affordances must fail closed rather than panic.
        assert_eq!(state(), None);
        assert!(!is_parent());
    }

    #[test]
    fn a_body_value_that_is_not_six_plain_digits_is_never_interpolated() {
        assert_eq!(json_digits("482913"), Some("482913"));
        assert_eq!(json_digits("  482913 "), Some("482913"));
        assert_eq!(json_digits(""), None);
        assert_eq!(json_digits("48\",\"x\":\"1"), None);
        assert_eq!(json_digits("4829a3"), None);
    }

    /// The non-wasm build must not pretend a request succeeded: `login` and
    /// `setup` return `false`, and `probe` reports "no answer" rather than
    /// inventing a state SSR could render the wrong form from.
    #[tokio::test]
    async fn the_non_wasm_stub_never_claims_a_session() {
        assert_eq!(probe().await, None);
        assert!(!login("482913").await);
        assert!(!setup("123456", "482913").await);
        logout().await;
    }
}
