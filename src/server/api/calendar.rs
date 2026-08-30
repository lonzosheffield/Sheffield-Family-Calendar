//! Calendar server functions (T2.4).
//!
//! Split out of the former `src/server/api.rs` by T1.2; **owned by T2.4** from
//! here on. The storage, recurrence and Google-polling logic lives in
//! [`crate::server::calendar`] — this file is the thin `#[server]`-fn layer
//! over it, the same shape `api::routine` has over `server::db`.
//!
//! Two things are deliberate:
//!
//! * **Reads are unauthenticated, writes require a parent session.** The TV
//!   has no session and cannot obtain one (§P5.5 default 31), and it must be
//!   able to render the day; but PLAN v2's scope line (§P5.5 default 35) puts
//!   "calendar editing" on the phone, behind the parent PIN. Every mutating
//!   function below therefore calls `auth::require_session` server-side,
//!   before touching the database, exactly as `api::profiles` does.
//! * **The week is computed on the server.** `chrono` is a server-only
//!   dependency, so a client cannot do calendar arithmetic at all — and it
//!   should not: every surface shows *server-local* time (§P5.5 default 14).
//!   [`get_calendar_week`] hands the client seven ready-made days.
//!
//! Protocol v2 note: [`crate::shared::types::ServerMessage::CalendarUpdated`]
//! carries only the affected `date`. The server never pushes the event payload
//! — clients refetch through these functions — which is what makes the message
//! unspoofable by a client (G13) and keeps the broadcast frames small.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::shared::types::{CalendarEvent, SessionToken};

/// One day of [`WeekView`], already rendered in server-local time.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct CalendarDay {
    /// `YYYY-MM-DD`.
    pub date: String,
    /// `Sunday`, `Monday`, … — the hub's own naming, not the browser's.
    pub weekday: String,
    /// Day of the month, for the column heading.
    pub day_of_month: u32,
    /// Is this the hub's today?
    pub is_today: bool,
    pub events: Vec<CalendarEvent>,
}

/// Seven days, Sunday first (§P5.5 default 14).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct WeekView {
    /// The Sunday the week starts on, `YYYY-MM-DD`.
    pub start: String,
    /// The Saturday the week ends on, `YYYY-MM-DD`.
    pub end: String,
    /// The hub's today, so the client can highlight it without a clock.
    pub today: String,
    pub days: Vec<CalendarDay>,
}

impl WeekView {
    /// Does the whole week hold nothing at all? Drives the week view's
    /// `Empty` state (W3).
    pub fn is_empty(&self) -> bool {
        self.days.iter().all(|day| day.events.is_empty())
    }
}

/// A local event as the phone submits it.
///
/// Timestamps are server-local wall clock (`YYYY-MM-DDTHH:MM`, which is
/// exactly what an `<input type="datetime-local">` produces); `rrule` is a
/// bare `FREQ=…` body and `tzid` an IANA zone name, both optional.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct LocalEventInput {
    pub title: String,
    pub starts_at: String,
    pub ends_at: Option<String>,
    pub all_day: bool,
    pub tzid: Option<String>,
    pub rrule: Option<String>,
    pub description: Option<String>,
    pub location: Option<String>,
    pub user_id: Option<i64>,
    pub color: Option<String>,
}

#[cfg(feature = "server")]
fn to_error(err: crate::server::calendar::CalendarError) -> ServerFnError {
    ServerFnError::new(err.to_string())
}

#[cfg(feature = "server")]
fn require_parent(auth: Option<SessionToken>) -> Result<(), ServerFnError> {
    let token = auth.unwrap_or_default();
    crate::server::auth::require_session(&token).map_err(|err| ServerFnError::new(err.to_string()))
}

/// Turn a [`LocalEventInput`] into the validated draft storage takes.
#[cfg(feature = "server")]
fn draft_from(
    input: &LocalEventInput,
) -> Result<crate::server::calendar::EventDraft, ServerFnError> {
    use crate::server::calendar::{parse_timestamp, CalendarError, EventDraft};

    let starts_at = parse_timestamp(&input.starts_at).ok_or_else(|| {
        to_error(CalendarError::Invalid(format!(
            "unparsable start {:?}",
            input.starts_at
        )))
    })?;
    let ends_at =
        match input.ends_at.as_deref().map(str::trim) {
            Some(raw) if !raw.is_empty() => Some(parse_timestamp(raw).ok_or_else(|| {
                to_error(CalendarError::Invalid(format!("unparsable end {raw:?}")))
            })?),
            _ => None,
        };

    let clean = |value: &Option<String>| {
        value
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };

    let draft = EventDraft {
        title: input.title.trim().to_string(),
        description: clean(&input.description),
        location: clean(&input.location),
        starts_at,
        ends_at,
        all_day: input.all_day,
        tzid: clean(&input.tzid),
        rrule: clean(&input.rrule),
        user_id: input.user_id,
        color: clean(&input.color),
    };
    draft.validate().map_err(to_error)?;
    Ok(draft)
}

// ---------------------------------------------------------------------------
// Reads
// ---------------------------------------------------------------------------

/// Today's events, in server-local time.
///
/// Unauthenticated on purpose: this is what the kiosk's calendar panel shows,
/// and the TV holds no session.
#[server(endpoint = "get_today_events")]
pub async fn get_today_events() -> Result<Vec<CalendarEvent>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::calendar as cal;

        // H-9: a read, so the read pool — never queued behind the writer.
        let pool = crate::server::db::read_pool()
            .await
            .map_err(super::to_server_error)?;
        let events = cal::occurrences_on(pool, cal::today_local())
            .await
            .map_err(to_error)?;
        Ok(events
            .iter()
            .map(cal::Occurrence::to_calendar_event)
            .collect())
    }
    #[cfg(not(feature = "server"))]
    {
        unreachable!("server function bodies only run on the server")
    }
}

/// Every event on one `YYYY-MM-DD`, in server-local time.
#[server(endpoint = "get_events_for_day")]
pub async fn get_events_for_day(date: String) -> Result<Vec<CalendarEvent>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::calendar as cal;

        let day = cal::parse_date(&date).ok_or_else(|| {
            to_error(cal::CalendarError::Invalid(format!(
                "expected YYYY-MM-DD, got {date:?}"
            )))
        })?;
        let pool = crate::server::db::read_pool()
            .await
            .map_err(super::to_server_error)?;
        let events = cal::occurrences_on(pool, day).await.map_err(to_error)?;
        Ok(events
            .iter()
            .map(cal::Occurrence::to_calendar_event)
            .collect())
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = date;
        unreachable!("server function bodies only run on the server")
    }
}

/// The Sunday-start week containing `anchor` (or today when `anchor` is
/// `None`), as **exactly seven** days.
///
/// The seven days are built with `NaiveDate` arithmetic, so the week
/// containing a DST transition — which is 167 or 169 hours long — still has
/// seven days with the right boundaries, rather than the six-and-a-bit a
/// 24-hour-multiple would produce.
#[server(endpoint = "get_calendar_week")]
pub async fn get_calendar_week(anchor: Option<String>) -> Result<WeekView, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::calendar as cal;

        let today = cal::today_local();
        let anchor_date = match anchor.as_deref().map(str::trim) {
            Some(raw) if !raw.is_empty() => cal::parse_date(raw).ok_or_else(|| {
                to_error(cal::CalendarError::Invalid(format!(
                    "expected YYYY-MM-DD, got {raw:?}"
                )))
            })?,
            _ => today,
        };

        let dates = cal::week_days(anchor_date);
        let start = dates[0];
        let end = dates[6];
        let from = start
            .and_hms_opt(0, 0, 0)
            .expect("midnight is a valid time of day");
        let to = (end + chrono::Duration::days(1))
            .and_hms_opt(0, 0, 0)
            .expect("midnight is a valid time of day");

        let pool = crate::server::db::read_pool()
            .await
            .map_err(super::to_server_error)?;
        let occurrences = cal::occurrences_between(pool, from, to)
            .await
            .map_err(to_error)?;

        let days = dates
            .iter()
            .map(|date| {
                let events = occurrences
                    .iter()
                    .filter(|occurrence| occurrence.start.date() == *date)
                    .map(cal::Occurrence::to_calendar_event)
                    .collect();
                CalendarDay {
                    date: date.format("%Y-%m-%d").to_string(),
                    weekday: date.format("%A").to_string(),
                    day_of_month: chrono::Datelike::day(date),
                    is_today: *date == today,
                    events,
                }
            })
            .collect();

        Ok(WeekView {
            start: start.format("%Y-%m-%d").to_string(),
            end: end.format("%Y-%m-%d").to_string(),
            today: today.format("%Y-%m-%d").to_string(),
            days,
        })
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = anchor;
        unreachable!("server function bodies only run on the server")
    }
}

// ---------------------------------------------------------------------------
// Local CRUD (parent session required)
// ---------------------------------------------------------------------------

/// Create a local event. Returns its row id.
#[server(endpoint = "create_local_event")]
pub async fn create_local_event(
    input: LocalEventInput,
    auth: Option<SessionToken>,
) -> Result<i64, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::calendar as cal;

        require_parent(auth)?;
        let draft = draft_from(&input)?;
        let pool = crate::server::db::pool()
            .await
            .map_err(super::to_server_error)?;
        let id = cal::insert_local_event(pool, &draft)
            .await
            .map_err(to_error)?;
        cal::publish_calendar_updated(draft.starts_at.date());
        Ok(id)
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (input, auth);
        unreachable!("server function bodies only run on the server")
    }
}

/// Overwrite a local event.
#[server(endpoint = "update_local_event")]
pub async fn update_local_event(
    id: i64,
    input: LocalEventInput,
    auth: Option<SessionToken>,
) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::calendar as cal;

        require_parent(auth)?;
        let draft = draft_from(&input)?;
        let pool = crate::server::db::pool()
            .await
            .map_err(super::to_server_error)?;
        // The day the event used to be on has to be refreshed too, or a moved
        // event lingers on its old day until something else invalidates it.
        let previous = cal::event_by_id(pool, id).await.map_err(to_error)?;
        if !cal::update_local_event(pool, id, &draft)
            .await
            .map_err(to_error)?
        {
            return Err(ServerFnError::new(format!("no local event {id}")));
        }
        if let Some(previous) = previous {
            cal::publish_calendar_updated(previous.starts_at.date());
        }
        cal::publish_calendar_updated(draft.starts_at.date());
        Ok(())
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (id, input, auth);
        unreachable!("server function bodies only run on the server")
    }
}

/// Delete a local event.
///
/// Deleting the last event of a day is what drives the panel back to `Empty`
/// (W3) — v1's `is_empty()` fallback could never get there.
#[server(endpoint = "delete_local_event")]
pub async fn delete_local_event(id: i64, auth: Option<SessionToken>) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::calendar as cal;

        require_parent(auth)?;
        let pool = crate::server::db::pool()
            .await
            .map_err(super::to_server_error)?;
        let existing = cal::event_by_id(pool, id).await.map_err(to_error)?;
        if !cal::delete_local_event(pool, id).await.map_err(to_error)? {
            return Err(ServerFnError::new(format!("no local event {id}")));
        }
        let date = existing
            .map(|event| event.starts_at.date())
            .unwrap_or_else(cal::today_local);
        cal::publish_calendar_updated(date);
        Ok(())
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (id, auth);
        unreachable!("server function bodies only run on the server")
    }
}
