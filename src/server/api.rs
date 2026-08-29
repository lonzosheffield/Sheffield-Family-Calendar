use dioxus::prelude::*;

use crate::shared::types::{CalendarEvent, CustomTaskView, RoutineItemView};

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
        let pool = crate::server::db::pool().await.map_err(to_server_error)?;
        crate::server::db::daily_routine(pool, user_id, &date)
            .await
            .map_err(to_server_error)
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
        let pool = crate::server::db::pool().await.map_err(to_server_error)?;
        crate::server::db::set_routine_completion(pool, user_id, template_id, completed, &date)
            .await
            .map_err(to_server_error)?;

        realtime::publish(&crate::shared::types::WsMessage::RoutineUpdated { user_id });
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
        let pool = crate::server::db::pool().await.map_err(to_server_error)?;
        crate::server::db::insert_custom_task(
            pool,
            user_id,
            &title,
            photo_base64.as_deref(),
            crate::server::db::upload_dir(),
        )
        .await
        .map_err(to_server_error)?;

        realtime::publish(&crate::shared::types::WsMessage::RoutineUpdated { user_id });
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
        let pool = crate::server::db::pool().await.map_err(to_server_error)?;
        crate::server::db::custom_tasks(pool, user_id)
            .await
            .map_err(to_server_error)
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = user_id;
        unreachable!("server function bodies only run on the server")
    }
}

#[server(endpoint = "toggle_custom_task")]
pub async fn toggle_custom_task(task_id: u32, completed: bool) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        let pool = crate::server::db::pool().await.map_err(to_server_error)?;
        crate::server::db::set_custom_task_completion(pool, task_id, completed)
            .await
            .map_err(to_server_error)
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (task_id, completed);
        unreachable!("server function bodies only run on the server")
    }
}

/// Today's cached Google Calendar events.
#[server(endpoint = "get_today_events")]
pub async fn get_today_events() -> Result<Vec<CalendarEvent>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        Ok(crate::server::calendar::cached_events().await)
    }
    #[cfg(not(feature = "server"))]
    {
        unreachable!("server function bodies only run on the server")
    }
}

/// Web paths of the ambient screensaver photos, sorted by file name.
#[server(endpoint = "list_screensaver_images")]
pub async fn list_screensaver_images() -> Result<Vec<String>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        // The on-disk directory is resolved from `FamilyHubConfig`
        // (T0.5: absolute, under `FAMILY_HUB_DATA_DIR`), independent of the
        // URL route the screensaver is served under (`/assets/screensaver`,
        // wired up by T0.6's `ServeDir`).
        const URL_PREFIX: &str = "/assets/screensaver";

        let dir = crate::server::config::FamilyHubConfig::load().screensaver_dir();
        let mut images = Vec::new();
        let Ok(mut entries) = tokio::fs::read_dir(&dir).await else {
            return Ok(images);
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();
            let is_image = matches!(
                std::path::Path::new(&name)
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(str::to_ascii_lowercase)
                    .as_deref(),
                Some("jpg" | "jpeg" | "png" | "webp" | "avif")
            );
            if is_image {
                images.push(format!("{URL_PREFIX}/{name}"));
            }
        }

        images.sort();
        Ok(images)
    }
    #[cfg(not(feature = "server"))]
    {
        unreachable!("server function bodies only run on the server")
    }
}

#[cfg(feature = "server")]
fn to_server_error(err: sqlx::Error) -> ServerFnError {
    ServerFnError::new(err.to_string())
}

/// Broadcast plumbing shared by the WebSocket route and the server functions.
#[cfg(feature = "server")]
pub mod realtime {
    use std::sync::OnceLock;

    use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
    use axum::response::Response;
    use futures_util::{SinkExt, StreamExt};
    use tokio::sync::broadcast;

    use crate::shared::types::WsMessage;

    const CHANNEL_CAPACITY: usize = 256;

    static CHANNEL: OnceLock<broadcast::Sender<String>> = OnceLock::new();

    pub fn sender() -> &'static broadcast::Sender<String> {
        CHANNEL.get_or_init(|| broadcast::channel(CHANNEL_CAPACITY).0)
    }

    /// Fan a message out to every connected kiosk and phone.
    pub fn publish(message: &WsMessage) {
        match serde_json::to_string(message) {
            Ok(payload) => {
                let _ = sender().send(payload);
            }
            Err(err) => tracing::error!("failed to encode websocket message: {err}"),
        }
    }

    pub async fn ws_handler(upgrade: WebSocketUpgrade) -> Response {
        upgrade.on_upgrade(handle_socket)
    }

    async fn handle_socket(socket: WebSocket) {
        let (mut sink, mut stream) = socket.split();
        let mut receiver = sender().subscribe();

        let mut outgoing = tokio::spawn(async move {
            while let Ok(payload) = receiver.recv().await {
                if sink.send(Message::Text(payload.into())).await.is_err() {
                    break;
                }
            }
        });

        // Anything a client sends (whiteboard strokes, clears) is rebroadcast
        // verbatim to the other clients.
        let mut incoming = tokio::spawn(async move {
            while let Some(Ok(message)) = stream.next().await {
                if let Message::Text(payload) = message {
                    if serde_json::from_str::<WsMessage>(payload.as_str()).is_ok() {
                        let _ = sender().send(payload.to_string());
                    }
                }
            }
        });

        tokio::select! {
            _ = &mut outgoing => incoming.abort(),
            _ = &mut incoming => outgoing.abort(),
        }
    }
}
