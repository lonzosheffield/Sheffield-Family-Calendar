//! Profile server functions, the parent PIN, and session issuing.
//!
//! Split out of the former `src/server/api.rs` by T1.2 so that **T1.4** owns a
//! file of its own (`docs/reviews/PURPLE_TEAM.md` §P4). T1.4 lands
//! `0003_profiles.sql`, the profile CRUD server functions, the parent PIN and
//! the session token here; the PIN hashing, setup code and session store
//! themselves live in `src/server/auth.rs` (also owned by T1.4) so this file
//! stays the thin `#[server]`-fn layer over it, matching how `api::routine`
//! sits over `server::db`.
//!
//! Protocol v2 already carries the broadcast T1.4 needs
//! ([`crate::shared::types::ServerMessage::ProfilesUpdated`], PURPLE default
//! 12/W6); [`publish_profiles_updated`] is the one call every mutating
//! function here makes after a profile is created, renamed, recolored or
//! removed.
//!
//! **Every privileged function below checks the session server-side**, via
//! `crate::server::auth::require_session`, before touching the database —
//! the acceptance requirement that calling one directly with no session
//! errors, regardless of what any client would or would not have done.

use dioxus::prelude::*;

use crate::shared::types::{Profile, SessionToken, SetupStatus};

#[cfg(feature = "server")]
use sqlx::Row;

/// Tell every connected client that the profile list changed, so it refetches.
///
/// Deliberately parameterless: a profile change is rare and affects the whole
/// roster, so there is nothing useful to scope it by (unlike `RoutineUpdated`
/// and `TasksUpdated`, which carry `user_id` + `date`).
#[cfg(feature = "server")]
pub fn publish_profiles_updated() {
    super::realtime::publish(&crate::shared::types::ServerMessage::ProfilesUpdated);
}

#[cfg(feature = "server")]
fn to_auth_error(err: crate::server::auth::AuthError) -> ServerFnError {
    ServerFnError::new(err.to_string())
}

/// Every privileged function below's session check (QA round 1, Q1-11): an
/// explicit `auth` bearer token still works exactly as it always has — the
/// PWA's existing `localStorage`-token flow (`docs/HANDOFF.md` H-19) needs no
/// change — but an **empty** one now falls back to the `fh_session` cookie on
/// the live HTTP request, via `auth::require_parent`. A direct in-process
/// call with an empty token (every existing "no session" acceptance test)
/// has no live request underneath it, so the fallback finds no cookie and
/// still fails closed.
///
/// **HS4 (visibility only):** made `pub(crate)` so `api::homeschool`'s parent-
/// only functions (Together ticks, Finish/Back week, enroll/unenroll, pause,
/// plan edits — `docs/homeschool/PLAN_HOMESCHOOL.md` §2 H7) can call the same
/// check `api::profiles` already uses, instead of duplicating it.
#[cfg(feature = "server")]
pub(crate) async fn require_session_or_cookie(auth: &str) -> Result<(), ServerFnError> {
    if auth.is_empty() {
        crate::server::auth::require_parent()
            .await
            .map_err(to_auth_error)
    } else {
        crate::server::auth::require_session(auth).map_err(to_auth_error)
    }
}

#[cfg(feature = "server")]
async fn data_dir() -> std::path::PathBuf {
    crate::server::config::FamilyHubConfig::load().data_dir
}

#[cfg(feature = "server")]
async fn fetch_profiles(pool: &sqlx::SqlitePool) -> Result<Vec<Profile>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, name, color, avatar, is_parent, sort_order \
         FROM profiles ORDER BY sort_order, id",
    )
    .fetch_all(pool)
    .await?;

    rows.iter()
        .map(|row| {
            Ok(Profile {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                color: row.try_get("color")?,
                avatar: row.try_get("avatar")?,
                is_parent: row.try_get::<i64, _>("is_parent")? != 0,
                sort_order: row.try_get("sort_order")?,
            })
        })
        .collect()
}

#[cfg(feature = "server")]
async fn profile_exists(pool: &sqlx::SqlitePool, id: i64) -> Result<bool, sqlx::Error> {
    let row: Option<(i64,)> = sqlx::query_as("SELECT id FROM profiles WHERE id = ?1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.is_some())
}

// ---------------------------------------------------------------------------
// Reads (no session required — the TV shows the roster to everyone)
// ---------------------------------------------------------------------------

/// Every profile, in display order. Unauthenticated on purpose: the TV needs
/// this to render the profile selector before anyone has signed in.
#[server(endpoint = "list_profiles")]
pub async fn list_profiles() -> Result<Vec<Profile>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        let pool = crate::server::db::read_pool()
            .await
            .map_err(super::to_server_error)?;
        fetch_profiles(pool).await.map_err(super::to_server_error)
    }
    #[cfg(not(feature = "server"))]
    {
        unreachable!("server function bodies only run on the server")
    }
}

/// Whether the parent PIN has been set yet. Also the trigger that generates
/// the first-run setup code the first time it is asked
/// (`auth::ensure_setup_code`).
#[server(endpoint = "parent_setup_status")]
pub async fn parent_setup_status() -> Result<SetupStatus, ServerFnError> {
    #[cfg(feature = "server")]
    {
        let pool = crate::server::db::pool()
            .await
            .map_err(super::to_server_error)?;
        let dir = data_dir().await;
        crate::server::auth::ensure_setup_code(pool, &dir)
            .await
            .map_err(to_auth_error)?;
        let pin_set = crate::server::auth::pin_is_set(pool)
            .await
            .map_err(to_auth_error)?;
        Ok(SetupStatus { pin_set })
    }
    #[cfg(not(feature = "server"))]
    {
        unreachable!("server function bodies only run on the server")
    }
}

// ---------------------------------------------------------------------------
// Parent PIN
// ---------------------------------------------------------------------------

/// Set the very first parent PIN, gated by the first-run setup code (proving
/// physical access to the server's log / `<data>\setup-code.txt` / the TV).
/// Returns a fresh 30-day session token on success.
#[server(endpoint = "set_initial_parent_pin")]
pub async fn set_initial_parent_pin(
    setup_code: String,
    pin: String,
) -> Result<SessionToken, ServerFnError> {
    #[cfg(feature = "server")]
    {
        let pool = crate::server::db::pool()
            .await
            .map_err(super::to_server_error)?;
        let dir = data_dir().await;
        crate::server::auth::set_initial_pin(pool, &dir, &setup_code, &pin)
            .await
            .map_err(to_auth_error)
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (setup_code, pin);
        unreachable!("server function bodies only run on the server")
    }
}

/// Verify a PIN attempt. **Enforced entirely server-side**: the client sends
/// only the six digits, and this function is the sole place they are ever
/// checked (R-17/R-23). Wrong guesses are answered — never rejected outright
/// (no lockout) — but only after an exponentially increasing delay
/// (`auth::backoff_delay`), so guessing is slow without ever locking a parent
/// out.
#[server(endpoint = "verify_parent_pin")]
pub async fn verify_parent_pin(pin: String) -> Result<SessionToken, ServerFnError> {
    #[cfg(feature = "server")]
    {
        let pool = crate::server::db::pool()
            .await
            .map_err(super::to_server_error)?;
        crate::server::auth::verify_pin(pool, &pin)
            .await
            .map_err(to_auth_error)
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = pin;
        unreachable!("server function bodies only run on the server")
    }
}

/// Change the PIN. Privileged: requires an already-valid parent session.
#[server(endpoint = "change_parent_pin")]
pub async fn change_parent_pin(auth: SessionToken, new_pin: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        require_session_or_cookie(&auth).await?;
        let pool = crate::server::db::pool()
            .await
            .map_err(super::to_server_error)?;
        crate::server::auth::change_pin(pool, &new_pin)
            .await
            .map_err(to_auth_error)
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (auth, new_pin);
        unreachable!("server function bodies only run on the server")
    }
}

// ---------------------------------------------------------------------------
// Profile CRUD — every mutation is privileged and broadcasts ProfilesUpdated
// ---------------------------------------------------------------------------

/// Create a profile. Not capped at four: a 5th, 6th, ... profile is exactly
/// what dropping the old `CHECK (user_id BETWEEN 1 AND 4)` (W5) enables.
#[server(endpoint = "create_profile")]
pub async fn create_profile(
    auth: SessionToken,
    name: String,
    color: String,
    avatar: Option<String>,
    is_parent: bool,
) -> Result<Profile, ServerFnError> {
    #[cfg(feature = "server")]
    {
        require_session_or_cookie(&auth).await?;
        let pool = crate::server::db::pool()
            .await
            .map_err(super::to_server_error)?;

        let (sort_order,): (i64,) =
            sqlx::query_as("SELECT COALESCE(MAX(sort_order), 0) + 1 FROM profiles")
                .fetch_one(pool)
                .await
                .map_err(super::to_server_error)?;

        let id: i64 = sqlx::query(
            "INSERT INTO profiles (name, color, avatar, is_parent, sort_order) \
             VALUES (?1, ?2, ?3, ?4, ?5) RETURNING id",
        )
        .bind(&name)
        .bind(&color)
        .bind(&avatar)
        .bind(is_parent as i64)
        .bind(sort_order)
        .fetch_one(pool)
        .await
        .map_err(super::to_server_error)?
        .try_get("id")
        .map_err(super::to_server_error)?;

        publish_profiles_updated();
        Ok(Profile {
            id,
            name,
            color,
            avatar,
            is_parent,
            sort_order,
        })
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (auth, name, color, avatar, is_parent);
        unreachable!("server function bodies only run on the server")
    }
}

/// Rename a profile. Persists and broadcasts `ProfilesUpdated`.
#[server(endpoint = "rename_profile")]
pub async fn rename_profile(
    auth: SessionToken,
    id: i64,
    name: String,
) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        require_session_or_cookie(&auth).await?;
        let pool = crate::server::db::pool()
            .await
            .map_err(super::to_server_error)?;

        if !profile_exists(pool, id)
            .await
            .map_err(super::to_server_error)?
        {
            return Err(to_auth_error(
                crate::server::auth::AuthError::UnknownProfile,
            ));
        }

        sqlx::query("UPDATE profiles SET name = ?2 WHERE id = ?1")
            .bind(id)
            .bind(&name)
            .execute(pool)
            .await
            .map_err(super::to_server_error)?;

        publish_profiles_updated();
        Ok(())
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (auth, id, name);
        unreachable!("server function bodies only run on the server")
    }
}

/// Recolor a profile.
#[server(endpoint = "set_profile_color")]
pub async fn set_profile_color(
    auth: SessionToken,
    id: i64,
    color: String,
) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        require_session_or_cookie(&auth).await?;
        let pool = crate::server::db::pool()
            .await
            .map_err(super::to_server_error)?;

        if !profile_exists(pool, id)
            .await
            .map_err(super::to_server_error)?
        {
            return Err(to_auth_error(
                crate::server::auth::AuthError::UnknownProfile,
            ));
        }

        sqlx::query("UPDATE profiles SET color = ?2 WHERE id = ?1")
            .bind(id)
            .bind(&color)
            .execute(pool)
            .await
            .map_err(super::to_server_error)?;

        publish_profiles_updated();
        Ok(())
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (auth, id, color);
        unreachable!("server function bodies only run on the server")
    }
}

/// Delete a profile. `ON DELETE CASCADE` (`migrations/0003_profiles.sql`)
/// removes its routine logs and custom tasks along with it.
#[server(endpoint = "delete_profile")]
pub async fn delete_profile(auth: SessionToken, id: i64) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        require_session_or_cookie(&auth).await?;
        let pool = crate::server::db::pool()
            .await
            .map_err(super::to_server_error)?;

        let deleted = sqlx::query("DELETE FROM profiles WHERE id = ?1")
            .bind(id)
            .execute(pool)
            .await
            .map_err(super::to_server_error)?
            .rows_affected();

        if deleted == 0 {
            return Err(to_auth_error(
                crate::server::auth::AuthError::UnknownProfile,
            ));
        }

        publish_profiles_updated();
        Ok(())
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (auth, id);
        unreachable!("server function bodies only run on the server")
    }
}
