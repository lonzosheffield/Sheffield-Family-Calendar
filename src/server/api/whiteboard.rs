//! Whiteboard "undo own last stroke" — **T2.3**.
//!
//! `docs/reviews/PURPLE_TEAM.md` §P3 T2.3 calls for undo-own-last-stroke, but
//! `docs/PROTOCOL.md`'s `ClientMessage` has no bespoke "undo" variant (it
//! wasn't needed: the server already knows how to tell every viewer "here is
//! the whole board" via `ServerMessage::Snapshot`, which every client already
//! applies by clearing the canvas and replaying in `seq` order). Undo is
//! therefore a plain `#[server]` fn, the same shape every other mutation in
//! this codebase uses (`api::routine::toggle_routine_task`,
//! `api::profiles::rename_profile`, …), rather than a new WebSocket message —
//! it is infrequent, needs no sub-second fan-out latency, and this way
//! `src/shared/types.rs` (owned by T1.2, later edited by T1.4) does not need
//! a third editor for one button.
//!
//! `client_id` is the WS-minted [`crate::shared::types::ClientId`] the
//! caller's own `RealtimeBus::client_id` already holds — not a family
//! profile's `user_id` — so "the calling client" means exactly what
//! `db::undo_last_stroke` (T1.1) and `docs/PROTOCOL.md` §2 mean by it: undo
//! only ever removes a stroke stamped with this same connection's `origin`.

use dioxus::prelude::*;

/// Remove the caller's own most recent live stroke from board 1.
///
/// Returns the removed `seq`, or `None` when that client has nothing left to
/// undo (never an error — "nothing to undo" is a normal outcome, not a
/// failure). On an actual removal every connected client — the caller
/// included — is sent a fresh `Snapshot` of the whole board, so undo repaints
/// from the authoritative log exactly the way a `Resync` or a fresh
/// connection already does.
#[server(endpoint = "undo_last_stroke")]
pub async fn undo_last_stroke(client_id: String) -> Result<Option<i64>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::shared::types::{ServerMessage, DEFAULT_BOARD_ID};

        let removed = super::realtime::undo_last_stroke(DEFAULT_BOARD_ID, &client_id)
            .await
            .map_err(super::to_server_error)?;

        if removed.is_some() {
            let (seq, strokes) = super::realtime::snapshot(DEFAULT_BOARD_ID, 0)
                .await
                .map_err(super::to_server_error)?;
            super::realtime::publish(&ServerMessage::Snapshot {
                board_id: DEFAULT_BOARD_ID,
                seq,
                strokes,
            });
        }

        Ok(removed)
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = client_id;
        unreachable!("server function bodies only run on the server")
    }
}
