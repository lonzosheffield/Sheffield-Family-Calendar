//! Routine and custom-task server functions.
//!
//! Split out of the former `src/server/api.rs` by T1.2; **owned by T1.5** from
//! here on (explicit dates, idempotency keys, ownership checks, the missing
//! `TasksUpdated` broadcast).

use dioxus::prelude::*;

use crate::shared::types::{CustomTaskView, RoutineItemView};

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
        let pool = crate::server::db::pool()
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

/// Check or uncheck a routine item for today and notify every connected client.
#[server(endpoint = "toggle_routine_task")]
pub async fn toggle_routine_task(
    user_id: u32,
    template_id: u32,
    completed: bool,
) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        let date = chrono::Local::now().format("%Y-%m-%d").to_string();
        let pool = crate::server::db::pool()
            .await
            .map_err(super::to_server_error)?;
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
        let _ = (user_id, template_id, completed);
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
        let pool = crate::server::db::pool()
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
#[server(endpoint = "toggle_custom_task")]
pub async fn toggle_custom_task(task_id: u32, completed: bool) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        let pool = crate::server::db::pool()
            .await
            .map_err(super::to_server_error)?;
        crate::server::db::set_custom_task_completion(pool, task_id, completed)
            .await
            .map_err(super::to_server_error)
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (task_id, completed);
        unreachable!("server function bodies only run on the server")
    }
}
