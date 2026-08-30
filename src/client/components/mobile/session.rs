//! The parent session token, held client-side on the phone surface.
//!
//! `docs/HANDOFF.md` H-19 is the reason this exists in this shape. PLAN v2
//! §P5.5 default 31 wants the 30-day parent session delivered as an
//! `HttpOnly` + `Secure` + `SameSite=Lax` cookie; QA round 1 (Q1-11) landed
//! the server side of exactly that — `POST /api/login`, `POST /api/logout`
//! and `GET /api/session` in `src/server/router.rs`, `auth::session_from_headers`
//! / `auth::require_parent` in `src/server/auth.rs`, and `/ws`'s upgrade now
//! honouring the cookie for `SetView`/`SetActiveProfile` with no bearer
//! `auth` on the message at all (`src/server/api/realtime.rs`). Every
//! privileged `api::profiles::*` call also accepts an **empty** `auth` and
//! falls back to that cookie.
//!
//! **This module is deliberately unchanged.** Migrating the phone's own
//! client-side flow — `mobile/settings.rs`'s sign-in form to `POST
//! /api/login`, `mobile/remote.rs` and `calendar.rs` to stop threading
//! `session::token()` through every WS/server-fn call, `is_parent()` to the
//! `GET /api/session` probe this module's doc once sketched (a script can
//! never read an `HttpOnly` cookie's value to check it directly) — is a
//! `src/client/components/mobile/**` change (T2.2's file, PURPLE §P4), out of
//! scope for a T1.4 QA fix. Recorded in `docs/HANDOFF.md` as the follow-up
//! for whichever task next touches this directory; until then both session
//! mechanisms work side by side (the server accepts either), so nothing here
//! is broken by leaving it as-is.
//!
//! Security note for whoever does that migration: this token is what
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
