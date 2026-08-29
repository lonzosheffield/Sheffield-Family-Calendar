//! Routine and custom-task server functions.
//!
//! Split out of the former `src/server/api.rs` by T1.2; **owned by T1.5** from
//! here on (explicit dates, idempotency keys, ownership checks, the missing
//! `TasksUpdated` broadcast).
//!
//! Every mutating fn here (`toggle_routine_task`, `toggle_custom_task`) now
//! takes an explicit **client-declared `date`**, validated within ±1 day of
//! the server's own clock (`db::date_within_window`, R-24), and a client-
//! generated **`idempotency_key`**, deduped through the `mutation_log` table
//! (`db::claim_mutation`, R-15) so a retried or replayed request produces
//! exactly one effect. `toggle_custom_task` additionally checks that the
//! caller-supplied `user_id` actually owns `task_id` before writing anything
//! (R-23: "user 2 cannot toggle user 3's task"), and — closing G22/W1 — now
//! publishes `ServerMessage::TasksUpdated` on a successful change, which the
//! v1 endpoint never did.

use dioxus::prelude::*;

use crate::shared::types::{CustomTaskView, RoutineItemView};

/// Build a `ServerFnError` for a validation failure this module owns (never
/// a raw `sqlx::Error`, which [`super::to_server_error`] handles instead).
#[cfg(feature = "server")]
fn validation_error(message: &str) -> ServerFnError {
    ServerFnError::new(message.to_string())
}

/// Today's date in `YYYY-MM-DD`, as seen by the server.
#[server(endpoint = "today")]
pub async fn today() -> Result<String, ServerFnError> {
    #[cfg(feature = "server")]
    {
        Ok(chrono::Local::now().format("%Y-%m-%d").to_string())
    }
    #[cfg(not(feature = "server"))]
    {
        unreachable!("server function bodies only run on the server")
    }
}

/// The eight routine templates joined with `user_id`'s progress on `date`.
#[server(endpoint = "get_daily_routine")]
pub async fn get_daily_routine(
    user_id: u32,
    date: String,
) -> Result<Vec<RoutineItemView>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        // H-9: read-only, so the read pool — a `SELECT` here must never queue
        // behind the single write connection.
        let pool = crate::server::db::read_pool()
            .await
            .map_err(super::to_server_error)?;
        crate::server::db::daily_routine(pool, user_id, &date)
            .await
            .map_err(super::to_server_error)
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (user_id, date);
        unreachable!("server function bodies only run on the server")
    }
}

/// Check or uncheck a routine item for `date` and notify every connected
/// client.
///
/// **T1.5 (R-24/R-15):** `date` is the caller's belief about which day this
/// completion belongs to — no longer `chrono::Local::now()` read at write
/// time, which silently misfiled a completion logged just after midnight.
/// The server still has the final say: `date` must fall within ±1 day of its
/// own clock (`db::date_within_window`) or the call is rejected before
/// anything is written. `idempotency_key` is claimed in `mutation_log`
/// first, so replaying the same request (a retried HTTP call, a queued
/// offline mutation replayed twice) toggles the row at most once.
#[server(endpoint = "toggle_routine_task")]
pub async fn toggle_routine_task(
    user_id: u32,
    template_id: u32,
    completed: bool,
    date: String,
    idempotency_key: String,
) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        if !crate::server::db::date_within_window(&date, &today) {
            return Err(validation_error(&format!(
                "date {date} is outside the ±1 day window around {today}"
            )));
        }

        let pool = crate::server::db::pool()
            .await
            .map_err(super::to_server_error)?;

        let payload =
            format!(r#"{{"template_id":{template_id},"completed":{completed},"date":"{date}"}}"#);
        let claimed = crate::server::db::claim_mutation(
            pool,
            &idempotency_key,
            "toggle_routine_task",
            user_id,
            &payload,
        )
        .await
        .map_err(super::to_server_error)?;

        if !claimed {
            // Already applied by an earlier delivery of this same key (R-15):
            // report success without touching the row a second time.
            return Ok(());
        }

        crate::server::db::set_routine_completion(pool, user_id, template_id, completed, &date)
            .await
            .map_err(super::to_server_error)?;

        // Protocol v2: the notification carries `user_id` **and** `date` so a
        // client refetches only the profile that actually changed (W7).
        super::realtime::publish(&crate::shared::types::ServerMessage::RoutineUpdated {
            user_id: i64::from(user_id),
            date,
        });
        Ok(())
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (user_id, template_id, completed, date, idempotency_key);
        unreachable!("server function bodies only run on the server")
    }
}

/// Create a custom task, optionally storing a photo captured on a phone.
#[server(endpoint = "create_photo_task")]
pub async fn create_photo_task(
    user_id: u32,
    title: String,
    photo_base64: Option<String>,
) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        let pool = crate::server::db::pool()
            .await
            .map_err(super::to_server_error)?;
        crate::server::db::insert_custom_task(
            pool,
            user_id,
            &title,
            photo_base64.as_deref(),
            crate::server::db::upload_dir(),
        )
        .await
        .map_err(super::to_server_error)?;

        super::realtime::publish(&crate::shared::types::ServerMessage::TasksUpdated {
            user_id: i64::from(user_id),
            date: chrono::Local::now().format("%Y-%m-%d").to_string(),
        });
        Ok(())
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (user_id, title, photo_base64);
        unreachable!("server function bodies only run on the server")
    }
}

/// Custom tasks belonging to `user_id`, newest first.
#[server(endpoint = "get_custom_tasks")]
pub async fn get_custom_tasks(user_id: u32) -> Result<Vec<CustomTaskView>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        // H-9: read-only, so the read pool.
        let pool = crate::server::db::read_pool()
            .await
            .map_err(super::to_server_error)?;
        crate::server::db::custom_tasks(pool, user_id)
            .await
            .map_err(super::to_server_error)
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = user_id;
        unreachable!("server function bodies only run on the server")
    }
}

/// Tick or untick a custom task.
///
/// **T1.5 (G22/W1)** adds the owning `user_id`, the explicit date, the
/// ownership check and the `TasksUpdated` broadcast here; protocol v2 already
/// carries the message
/// ([`crate::shared::types::ServerMessage::TasksUpdated`]).
///
/// `user_id` is the profile the caller claims to be acting as; it must
/// actually own `task_id` (R-23 — "user 2 cannot toggle user 3's task") or
/// the call is rejected and nothing is written. `date` and
/// `idempotency_key` follow the same contract as
/// [`toggle_routine_task`].
#[server(endpoint = "toggle_custom_task")]
pub async fn toggle_custom_task(
    user_id: u32,
    task_id: u32,
    completed: bool,
    date: String,
    idempotency_key: String,
) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        if !crate::server::db::date_within_window(&date, &today) {
            return Err(validation_error(&format!(
                "date {date} is outside the ±1 day window around {today}"
            )));
        }

        // H-9: the ownership lookup only reads, so it goes through the read
        // pool; the actual write below still goes through the single write
        // connection.
        let read_pool = crate::server::db::read_pool()
            .await
            .map_err(super::to_server_error)?;
        let owner = crate::server::db::custom_task_owner(read_pool, task_id)
            .await
            .map_err(super::to_server_error)?;
        if owner != Some(user_id) {
            return Err(validation_error(
                "you may not change the completion state of another profile's task",
            ));
        }

        let pool = crate::server::db::pool()
            .await
            .map_err(super::to_server_error)?;

        let payload = format!(r#"{{"task_id":{task_id},"completed":{completed},"date":"{date}"}}"#);
        let claimed = crate::server::db::claim_mutation(
            pool,
            &idempotency_key,
            "toggle_custom_task",
            user_id,
            &payload,
        )
        .await
        .map_err(super::to_server_error)?;

        if !claimed {
            return Ok(());
        }

        crate::server::db::set_custom_task_completion(pool, task_id, completed)
            .await
            .map_err(super::to_server_error)?;

        // G22/W1: the v1 endpoint never told anyone this changed, so a
        // phone's tick never reached the TV. Scoped by `user_id` + `date`
        // exactly like `RoutineUpdated` (W7).
        super::realtime::publish(&crate::shared::types::ServerMessage::TasksUpdated {
            user_id: i64::from(user_id),
            date,
        });
        Ok(())
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (user_id, task_id, completed, date, idempotency_key);
        unreachable!("server function bodies only run on the server")
    }
}
