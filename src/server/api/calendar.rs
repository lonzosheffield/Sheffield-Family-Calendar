//! Calendar server functions.
//!
//! Split out of the former `src/server/api.rs` by T1.2; **owned by T2.4** from
//! here on (SQLite-backed events, local CRUD, Today + Week views, windowed
//! Google polling, `rrule` recurrence, `Loading`/`Empty`/`Error` states).
//!
//! Protocol v2 note: [`crate::shared::types::ServerMessage::CalendarUpdated`]
//! now carries only the affected `date`. The server no longer pushes the event
//! payload itself — clients refetch through [`get_today_events`] — which is
//! what makes the message unspoofable by a client (G13) and keeps the
//! broadcast frames small.

use dioxus::prelude::*;

use crate::shared::types::CalendarEvent;

/// Today's cached calendar events.
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
