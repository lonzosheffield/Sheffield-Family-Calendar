//! Client side WebSocket bus. Every open kiosk or phone subscribes to `/ws`
//! and reacts to broadcasts without polling or refreshing.

use dioxus::prelude::*;

use crate::shared::types::{CalendarEvent, StrokeSegment, WsMessage};

/// Shared realtime state, provided as a context by [`crate::client::app::App`].
#[derive(Clone, Copy)]
pub struct RealtimeBus {
    /// Bumped whenever a routine change arrives, used to refetch progress.
    pub routine_version: Signal<u64>,
    /// Latest calendar payload pushed by the polling task.
    pub calendar_events: Signal<Vec<CalendarEvent>>,
    /// Most recent remote stroke, consumed by the whiteboard canvas.
    pub inbound_stroke: Signal<Option<StrokeSegment>>,
    /// Bumped whenever a remote client clears the whiteboard.
    pub clear_version: Signal<u64>,
}

impl RealtimeBus {
    pub fn apply(&mut self, message: WsMessage) {
        match message {
            WsMessage::RoutineUpdated { .. } => {
                let next = self.routine_version.peek().wrapping_add(1);
                self.routine_version.set(next);
            }
            WsMessage::CalendarUpdated { events } => self.calendar_events.set(events),
            WsMessage::Draw { segment } => self.inbound_stroke.set(Some(segment)),
            WsMessage::ClearCanvas => {
                let next = self.clear_version.peek().wrapping_add(1);
                self.clear_version.set(next);
            }
        }
    }
}

/// Handle used by components to publish messages to the other clients.
pub type RealtimeSender = Coroutine<WsMessage>;

pub fn use_realtime() -> (RealtimeBus, RealtimeSender) {
    (
        use_context::<RealtimeBus>(),
        use_context::<RealtimeSender>(),
    )
}

/// Create the bus and start the WebSocket pump.
pub fn use_realtime_provider() -> RealtimeBus {
    let bus = use_context_provider(|| RealtimeBus {
        routine_version: Signal::new(0),
        calendar_events: Signal::new(Vec::new()),
        inbound_stroke: Signal::new(None),
        clear_version: Signal::new(0),
    });

    let _sender = use_coroutine(move |rx: UnboundedReceiver<WsMessage>| async move {
        pump(bus, rx).await;
    });

    bus
}

#[cfg(all(feature = "web", target_arch = "wasm32"))]
async fn pump(mut bus: RealtimeBus, mut rx: UnboundedReceiver<WsMessage>) {
    use futures_util::{SinkExt, StreamExt};
    use gloo_net::websocket::{futures::WebSocket, Message};

    let Some(url) = socket_url() else {
        return;
    };

    let Ok(socket) = WebSocket::open(&url) else {
        return;
    };

    let (mut sink, mut stream) = socket.split();

    let incoming = async move {
        while let Some(Ok(Message::Text(payload))) = stream.next().await {
            if let Ok(message) = serde_json::from_str::<WsMessage>(&payload) {
                bus.apply(message);
            }
        }
    };

    let outgoing = async move {
        while let Some(message) = rx.next().await {
            let Ok(payload) = serde_json::to_string(&message) else {
                continue;
            };
            if sink.send(Message::Text(payload)).await.is_err() {
                break;
            }
        }
    };

    futures_util::pin_mut!(incoming, outgoing);
    futures_util::future::select(incoming, outgoing).await;
}

#[cfg(not(all(feature = "web", target_arch = "wasm32")))]
async fn pump(_bus: RealtimeBus, _rx: UnboundedReceiver<WsMessage>) {}

#[cfg(all(feature = "web", target_arch = "wasm32"))]
fn socket_url() -> Option<String> {
    let location = web_sys::window()?.location();
    let protocol = if location.protocol().ok()?.starts_with("https") {
        "wss"
    } else {
        "ws"
    };
    Some(format!("{protocol}://{}/ws", location.host().ok()?))
}
