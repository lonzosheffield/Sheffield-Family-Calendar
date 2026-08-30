//! The parent session token, held client-side on the phone surface.
//!
//! `docs/HANDOFF.md` H-19 is the reason this exists in this shape. PLAN v2
//! §P5.5 default 31 wants the 30-day parent session delivered as an
//! `HttpOnly` + `Secure` + `SameSite=Lax` cookie, but the only place a
//! `Set-Cookie` could be attached is `src/server/router.rs`, which belongs to
//! T2.5 in this wave; Boss scheduled that login route as a micro-commit at
//! the 2-a/2-b boundary and recorded that **"until then T2.2 holds the bearer
//! token client-side and passes it as `auth: Option<SessionToken>` exactly as
//! the WS protocol already does."** That is precisely what this module is.
//!
//! Security note for whoever lands the cookie: this token is what
//! `ClientMessage::SetView` / `SetActiveProfile` carry (T1.2's auth check) and
//! what every privileged `api::profiles::*` call verifies server-side (T1.4).
//! Moving it to a cookie removes it from `localStorage` and therefore from
//! XSS reach; nothing else about the flow has to change, because the server
//! already treats it as an opaque bearer value either way.

use crate::client::components::mobile::storage;
use crate::shared::types::SessionToken;

/// `localStorage` key holding the parent session token.
pub const SESSION_STORAGE_KEY: &str = "familyhub.parent_session.v1";

/// The stored parent session token, if this phone has signed in.
pub fn token() -> Option<SessionToken> {
    storage::get(SESSION_STORAGE_KEY).filter(|value| !value.is_empty())
}

/// `true` when this phone holds a parent session.
pub fn is_parent() -> bool {
    token().is_some()
}

/// Remember a token returned by `api::profiles::verify_parent_pin`.
pub fn store(token: &str) {
    storage::set(SESSION_STORAGE_KEY, token);
}

/// Forget the token — the Settings tab's "Sign out".
///
/// Only clears this phone's copy. The server-side session stays valid until
/// it expires, which is the honest description of what a client-side sign-out
/// can do; a real revocation endpoint belongs with the cookie work in H-19.
pub fn clear() {
    storage::remove(SESSION_STORAGE_KEY);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One test, not three: they all drive the same process-wide storage key,
    /// and `cargo test` runs test functions on parallel threads.
    #[test]
    fn the_token_round_trips_clears_and_never_counts_an_empty_string() {
        clear();
        assert!(!is_parent());

        store("a-session-token");
        assert_eq!(token().as_deref(), Some("a-session-token"));
        assert!(is_parent());

        store("");
        assert!(!is_parent(), "an empty token must not count as signed in");

        clear();
        assert!(!is_parent());
    }
}
