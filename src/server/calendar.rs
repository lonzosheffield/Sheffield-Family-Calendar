//! Calendar v2 — SQLite-backed events, local recurrence, and a **windowed**
//! Google Calendar poll (PLAN v2 G10/R-19/R-28/W3/W4, task T2.4).
//!
//! What changed from v1, and why each change is load bearing:
//!
//! * **Events live in `events` (0002_core), not in a process `OnceLock`.**
//!   v1's cache was empty after every restart and could never shrink back to
//!   nothing (G10/W3). Every read below goes through the pool.
//! * **The Google poll replaces a whole window instead of upserting.**
//!   R-19: a `syncToken` cannot be combined with `timeMin`/`timeMax`, and an
//!   upsert-only sync never notices a deletion. So each poll deletes every
//!   `source='google'` row for that calendar inside `[window_start,
//!   window_end)` and re-inserts what the API just returned, inside one
//!   transaction — a remote deletion disappears **by construction**, with no
//!   `status: "cancelled"` bookkeeping and no sync token at all. `orderBy` is
//!   not sent either (occurrences are sorted here), so the parameter conflict
//!   cannot come back.
//! * **Local recurring events store `DTSTART` + `TZID` + `RRULE`** and are
//!   expanded at read time by [`expand_recurrence`], always through
//!   `rrule::RRuleSet::all(limit)` — **never** `all_unchecked()`
//!   (`docs/reviews/PURPLE_TEAM.md` §P5.4). A pathological rule therefore
//!   returns at most `limit` occurrences and terminates.
//! * **[`rfc3339_local`] is deterministic.** v1 fell back to
//!   `naive.and_utc()` when `Local` could not resolve a wall-clock time, i.e.
//!   it silently relabelled local time as UTC on exactly the two days a year
//!   that matter (R-28). This version resolves with `.earliest()` (via
//!   [`crate::server::db::resolve_local`]) and, for a time that does not exist
//!   at all, steps forward to the first minute that does.
//! * **The midnight tick forces a poll** (W4): [`register_midnight_poll`]
//!   hangs a hook off `realtime::on_day_rolled`, and the poll loop wakes on it
//!   rather than waiting out the rest of its 15 minute interval.
//!
//! Timestamps in the `events` table are **server-local wall clock**, stored as
//! `YYYY-MM-DDTHH:MM:SS` (PURPLE §P5.5 default 14: every surface shows
//! server-local time). One uniform format keeps the window `DELETE` and every
//! range `SELECT` a plain lexicographic string comparison; all-day events are
//! stored at `T00:00:00` with `all_day = 1` rather than as a bare date, so
//! they sort with everything else.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, TimeZone};
use rrule::{RRuleSet, Tz as RruleTz};
use serde::Deserialize;
use sqlx::{Row, SqlitePool};
use tokio::sync::Notify;

use crate::server::api::realtime;
use crate::server::db;
use crate::shared::types::{CalendarEvent, ServerMessage};

/// How often the Google window is re-fetched when credentials exist.
pub const POLL_INTERVAL: Duration = Duration::from_secs(15 * 60);

/// Days of history the polled window keeps (a week is enough for "what did we
/// do on Monday" without dragging years of history onto the television).
pub const WINDOW_PAST_DAYS: i64 = 7;
/// Days ahead the polled window covers.
pub const WINDOW_FUTURE_DAYS: i64 = 60;

/// Hard cap handed to `RRuleSet::all`. One year of daily occurrences is far
/// more than any window below asks for, and it is what makes a runaway rule
/// impossible (§P5.4: always `all(limit)`).
pub const RECURRENCE_LIMIT: u16 = 366;

/// How the `events` table stores a wall-clock timestamp.
pub const TIMESTAMP_FORMAT: &str = "%Y-%m-%dT%H:%M:%S";

const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const SCOPE: &str = "https://www.googleapis.com/auth/calendar.readonly";

/// Every column of `events`, in the order [`StoredEvent::from_row`] reads.
const EVENT_COLUMNS: &str = "id, source, external_id, calendar_id, title, description, location, \
                             starts_at, ends_at, all_day, tzid, rrule, user_id, color";

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum CalendarError {
    Db(sqlx::Error),
    /// An `RRULE`/`DTSTART` that `rrule` refused, carrying its own message.
    Rule(String),
    /// A malformed timestamp, title or window from a caller.
    Invalid(String),
}

impl std::fmt::Display for CalendarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CalendarError::Db(err) => write!(f, "calendar storage error: {err}"),
            CalendarError::Rule(msg) => write!(f, "invalid recurrence rule: {msg}"),
            CalendarError::Invalid(msg) => write!(f, "invalid calendar input: {msg}"),
        }
    }
}

impl std::error::Error for CalendarError {}

impl From<sqlx::Error> for CalendarError {
    fn from(err: sqlx::Error) -> Self {
        CalendarError::Db(err)
    }
}

// ---------------------------------------------------------------------------
// Time helpers
// ---------------------------------------------------------------------------

/// Deterministic local RFC3339 (R-28).
///
/// Ambiguous wall-clock times (the hour that repeats when DST ends) resolve to
/// the **earliest** of the two instants; a time that does not exist at all
/// (the hour skipped when DST begins) steps forward minute by minute to the
/// first instant that does. Neither case can produce v1's silent
/// "local relabelled as UTC" answer, which is the bug this replaces.
pub fn rfc3339_local(naive: NaiveDateTime) -> String {
    resolve_local_forward(naive).to_rfc3339()
}

/// [`rfc3339_local`]'s resolution step, exposed for the tests that pin it.
pub fn resolve_local_forward(naive: NaiveDateTime) -> chrono::DateTime<Local> {
    if let Some(resolved) = db::resolve_local(&Local, naive) {
        return resolved;
    }
    // A skipped wall-clock time. DST gaps are at most a couple of hours
    // anywhere on earth, so walking forward a few minutes always lands.
    for minute in 1..=180 {
        let candidate = naive + chrono::Duration::minutes(minute);
        if let Some(resolved) = db::resolve_local(&Local, candidate) {
            return resolved;
        }
    }
    // Unreachable for any real zone; still not a silent UTC relabel.
    Local
        .timestamp_opt(naive.and_utc().timestamp(), 0)
        .earliest()
        .unwrap_or_else(|| {
            Local.timestamp_nanos(naive.and_utc().timestamp_nanos_opt().unwrap_or_default())
        })
}

/// Parse the wall-clock forms this module accepts from a client or from the
/// database: `YYYY-MM-DDTHH:MM:SS`, `YYYY-MM-DDTHH:MM` and a bare
/// `YYYY-MM-DD` (midnight).
pub fn parse_timestamp(raw: &str) -> Option<NaiveDateTime> {
    let raw = raw.trim();
    NaiveDateTime::parse_from_str(raw, TIMESTAMP_FORMAT)
        .ok()
        .or_else(|| NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M").ok())
        .or_else(|| NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S").ok())
        .or_else(|| {
            NaiveDate::parse_from_str(raw, "%Y-%m-%d")
                .ok()
                .and_then(|date| date.and_hms_opt(0, 0, 0))
        })
        .or_else(|| {
            chrono::DateTime::parse_from_rfc3339(raw)
                .ok()
                .map(|dt| dt.with_timezone(&Local).naive_local())
        })
}

/// Render a wall-clock timestamp the way the `events` table stores it.
pub fn format_timestamp(value: NaiveDateTime) -> String {
    value.format(TIMESTAMP_FORMAT).to_string()
}

/// `YYYY-MM-DD` → the date, for the day/week server functions.
pub fn parse_date(raw: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d").ok()
}

/// The Sunday on or before `date` (PURPLE §P5.5 default 14: weeks start on
/// Sunday).
pub fn week_start(date: NaiveDate) -> NaiveDate {
    let offset = date.weekday().num_days_from_sunday() as i64;
    date - chrono::Duration::days(offset)
}

/// The seven dates of the week containing `date`, Sunday first.
///
/// Days, not durations: a week containing a DST transition is 167 or 169
/// hours long, so anything built by adding 24 h seven times would be wrong on
/// exactly the two weeks a year that matter. `NaiveDate` arithmetic is
/// calendar arithmetic and cannot drift.
pub fn week_days(date: NaiveDate) -> [NaiveDate; 7] {
    let start = week_start(date);
    std::array::from_fn(|index| start + chrono::Duration::days(index as i64))
}

/// The hub's current local date.
pub fn today_local() -> NaiveDate {
    Local::now().date_naive()
}

// ---------------------------------------------------------------------------
// Rows and drafts
// ---------------------------------------------------------------------------

/// One row of `events`, exactly as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredEvent {
    pub id: i64,
    pub source: String,
    pub external_id: Option<String>,
    pub calendar_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub starts_at: NaiveDateTime,
    pub ends_at: Option<NaiveDateTime>,
    pub all_day: bool,
    pub tzid: Option<String>,
    pub rrule: Option<String>,
    pub user_id: Option<i64>,
    pub color: Option<String>,
}

impl StoredEvent {
    fn from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Self, CalendarError> {
        let starts_raw: String = row.try_get("starts_at")?;
        let ends_raw: Option<String> = row.try_get("ends_at")?;
        let starts_at = parse_timestamp(&starts_raw).ok_or_else(|| {
            CalendarError::Invalid(format!("unparsable starts_at {starts_raw:?}"))
        })?;

        Ok(StoredEvent {
            id: row.try_get("id")?,
            source: row.try_get("source")?,
            external_id: row.try_get("external_id")?,
            calendar_id: row.try_get("calendar_id")?,
            title: row.try_get("title")?,
            description: row.try_get("description")?,
            location: row.try_get("location")?,
            starts_at,
            ends_at: ends_raw.as_deref().and_then(parse_timestamp),
            all_day: row.try_get::<i64, _>("all_day")? != 0,
            tzid: row.try_get("tzid")?,
            rrule: row.try_get("rrule")?,
            user_id: row.try_get("user_id")?,
            color: row.try_get("color")?,
        })
    }

    /// How long the event lasts, used to give every expanded occurrence of a
    /// recurring event the same duration as its `DTSTART`.
    fn duration(&self) -> Option<chrono::Duration> {
        self.ends_at.map(|end| end - self.starts_at)
    }
}

/// A local event as a client asks for it to be created or updated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventDraft {
    pub title: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub starts_at: NaiveDateTime,
    pub ends_at: Option<NaiveDateTime>,
    pub all_day: bool,
    pub tzid: Option<String>,
    pub rrule: Option<String>,
    pub user_id: Option<i64>,
    pub color: Option<String>,
}

impl EventDraft {
    /// Reject an empty title, a backwards interval and — the important one —
    /// a recurrence rule `rrule` will not accept, **before** it reaches the
    /// database and starts failing on every read instead of on this write.
    pub fn validate(&self) -> Result<(), CalendarError> {
        if self.title.trim().is_empty() {
            return Err(CalendarError::Invalid("an event needs a title".into()));
        }
        if let Some(end) = self.ends_at {
            if end < self.starts_at {
                return Err(CalendarError::Invalid(
                    "an event cannot end before it starts".into(),
                ));
            }
        }
        if let Some(rule) = self.rrule.as_deref() {
            expand_recurrence(self.starts_at, self.tzid.as_deref(), rule, None, 1)?;
        }
        Ok(())
    }
}

/// One occurrence of one event, in **server-local** wall clock. This is what
/// a day or a week is made of; a recurring event contributes one of these per
/// expanded occurrence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Occurrence {
    pub event_id: i64,
    pub source: String,
    pub title: String,
    pub start: NaiveDateTime,
    pub end: Option<NaiveDateTime>,
    pub all_day: bool,
    pub color: Option<String>,
    /// Did this come out of an `RRULE` expansion?
    pub recurring: bool,
}

impl Occurrence {
    /// The wire form every surface already renders
    /// ([`crate::shared::types::CalendarEvent`]).
    ///
    /// The id is `{source}:{row id}@{occurrence start}`, so two occurrences of
    /// the same recurring event are distinct keys — the TV uses this string
    /// for its focus ids, so collisions there would break D-pad navigation —
    /// and the phone can tell a locally-owned event (deletable) from a Google
    /// one (not: the next poll would just bring it back) without a second
    /// round trip.
    pub fn to_calendar_event(&self) -> CalendarEvent {
        let id = format!(
            "{}:{}@{}",
            self.source,
            self.event_id,
            self.start.format("%Y%m%dT%H%M%S")
        );
        let (start, end) = if self.all_day {
            let end_date = self
                .end
                .map(|end| end.date())
                .unwrap_or_else(|| self.start.date());
            (
                self.start.date().format("%Y-%m-%d").to_string(),
                end_date.format("%Y-%m-%d").to_string(),
            )
        } else {
            (
                rfc3339_local(self.start),
                rfc3339_local(self.end.unwrap_or(self.start)),
            )
        };

        CalendarEvent {
            id,
            summary: self.title.clone(),
            start,
            end,
            all_day: self.all_day,
        }
    }
}

// ---------------------------------------------------------------------------
// Recurrence
// ---------------------------------------------------------------------------

/// The result of expanding one `RRULE`.
#[derive(Clone, Debug)]
pub struct RecurrenceExpansion {
    /// Occurrences **in the rule's own timezone**, so a DST test can read the
    /// local wall-clock time and the UTC offset the rule actually produced.
    pub occurrences: Vec<chrono::DateTime<RruleTz>>,
    /// `true` when `all(limit)` stopped early — either it hit `limit` or
    /// `rrule`'s own iteration guard tripped. Never a hang, never unbounded.
    pub limited: bool,
}

impl RecurrenceExpansion {
    /// The same occurrences converted to server-local wall clock, which is
    /// what the surfaces display (§P5.5 default 14).
    pub fn local_starts(&self) -> Vec<NaiveDateTime> {
        self.occurrences
            .iter()
            .map(|dt| dt.with_timezone(&Local).naive_local())
            .collect()
    }
}

/// Expand `rule` from `dtstart`, optionally clipped to `window` (interpreted
/// in the rule's own timezone), through `RRuleSet::all(limit)`.
///
/// `tzid` is an IANA name (`America/New_York`); `None` means the hub's own
/// zone. The rule is handed to `rrule` as the iCalendar text it parses
/// natively, which is also what makes `chrono-tz`'s real DST tables — not
/// fixed offsets — decide where each occurrence lands.
pub fn expand_recurrence(
    dtstart: NaiveDateTime,
    tzid: Option<&str>,
    rule: &str,
    window: Option<(NaiveDateTime, NaiveDateTime)>,
    limit: u16,
) -> Result<RecurrenceExpansion, CalendarError> {
    let rule = rule.trim();
    if rule.is_empty() {
        return Err(CalendarError::Rule("empty RRULE".into()));
    }
    let rule_body = rule.strip_prefix("RRULE:").unwrap_or(rule);
    let dtstart_line = match tzid {
        Some(zone) if !zone.trim().is_empty() => format!(
            "DTSTART;TZID={}:{}",
            zone.trim(),
            dtstart.format("%Y%m%dT%H%M%S")
        ),
        _ => format!("DTSTART:{}", dtstart.format("%Y%m%dT%H%M%S")),
    };

    let text = format!("{dtstart_line}\nRRULE:{rule_body}");
    let set: RRuleSet = text
        .parse()
        .map_err(|err: rrule::RRuleError| CalendarError::Rule(err.to_string()))?;

    let tz = set.get_dt_start().timezone();
    let set = match window {
        Some((from, to)) => {
            let after = zoned_earliest(tz, from)?;
            // `RRuleSet::all` treats both bounds as inclusive; back the upper
            // bound off by a second so a window is half-open like every other
            // range in this module.
            let before = zoned_earliest(tz, to - chrono::Duration::seconds(1))?;
            set.after(after).before(before)
        }
        None => set,
    };

    let result = set.all(limit);
    Ok(RecurrenceExpansion {
        occurrences: result.dates,
        limited: result.limited,
    })
}

/// Resolve a wall-clock time in `tz`, earliest-wins, stepping forward over a
/// DST gap — the same determinism [`rfc3339_local`] gives the hub's own zone.
fn zoned_earliest(
    tz: RruleTz,
    naive: NaiveDateTime,
) -> Result<chrono::DateTime<RruleTz>, CalendarError> {
    for minute in 0..=180 {
        let candidate = naive + chrono::Duration::minutes(minute);
        if let Some(resolved) = tz.from_local_datetime(&candidate).earliest() {
            return Ok(resolved);
        }
    }
    Err(CalendarError::Invalid(format!(
        "{naive} does not exist in {}",
        tz.name()
    )))
}

// ---------------------------------------------------------------------------
// Reads
// ---------------------------------------------------------------------------

async fn fetch_events(
    pool: &SqlitePool,
    sql: &str,
    binds: &[String],
) -> Result<Vec<StoredEvent>, CalendarError> {
    let mut query = sqlx::query(sql);
    for bind in binds {
        query = query.bind(bind);
    }
    let rows = query.fetch_all(pool).await?;
    rows.iter().map(StoredEvent::from_row).collect()
}

/// Every occurrence between `from` (inclusive) and `to` (exclusive), sorted.
///
/// Two queries, because the two kinds of row need different treatment:
/// single events are selected by their own `starts_at`, while recurring
/// events have to be fetched whenever their `DTSTART` is before the end of
/// the window and then expanded — a rule that started in 2024 still produces
/// occurrences today. Recurrence is a **local-event-only** feature (§P5.5
/// default 13: Google events are stored already expanded), so that second set
/// is tiny and owner-authored.
pub async fn occurrences_between(
    pool: &SqlitePool,
    from: NaiveDateTime,
    to: NaiveDateTime,
) -> Result<Vec<Occurrence>, CalendarError> {
    let from_text = format_timestamp(from);
    let to_text = format_timestamp(to);

    let singles = fetch_events(
        pool,
        &format!(
            "SELECT {EVENT_COLUMNS} FROM events \
             WHERE rrule IS NULL AND starts_at < ?2 \
               AND COALESCE(ends_at, starts_at) >= ?1 \
             ORDER BY starts_at"
        ),
        &[from_text.clone(), to_text.clone()],
    )
    .await?;

    let recurring = fetch_events(
        pool,
        &format!(
            "SELECT {EVENT_COLUMNS} FROM events \
             WHERE rrule IS NOT NULL AND starts_at < ?1 \
             ORDER BY starts_at"
        ),
        &[to_text],
    )
    .await?;

    let mut out: Vec<Occurrence> = Vec::new();
    for event in singles {
        out.push(occurrence_of(&event, event.starts_at, false));
    }

    for event in recurring {
        let Some(rule) = event.rrule.clone() else {
            continue;
        };
        let expansion = match expand_recurrence(
            event.starts_at,
            event.tzid.as_deref(),
            &rule,
            Some((from, to)),
            RECURRENCE_LIMIT,
        ) {
            Ok(expansion) => expansion,
            Err(err) => {
                // One bad rule must not blank the whole panel — that is
                // exactly the failure mode the explicit `Error` state exists
                // to make visible for real outages, not for one bad row.
                tracing::warn!(event = event.id, %err, "skipping unexpandable recurring event");
                continue;
            }
        };
        for start in expansion.local_starts() {
            if start >= from && start < to {
                out.push(occurrence_of(&event, start, true));
            }
        }
    }

    out.sort_by(|a, b| {
        b.all_day
            .cmp(&a.all_day)
            .then(a.start.cmp(&b.start))
            .then(a.title.cmp(&b.title))
            .then(a.event_id.cmp(&b.event_id))
    });
    Ok(out)
}

fn occurrence_of(event: &StoredEvent, start: NaiveDateTime, recurring: bool) -> Occurrence {
    let end = if recurring {
        event.duration().map(|duration| start + duration)
    } else {
        event.ends_at
    };
    Occurrence {
        event_id: event.id,
        source: event.source.clone(),
        title: event.title.clone(),
        start,
        end,
        all_day: event.all_day,
        color: event.color.clone(),
        recurring,
    }
}

/// Every occurrence on one local date.
pub async fn occurrences_on(
    pool: &SqlitePool,
    date: NaiveDate,
) -> Result<Vec<Occurrence>, CalendarError> {
    let from = date
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| CalendarError::Invalid("midnight is always valid".into()))?;
    let to = (date + chrono::Duration::days(1))
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| CalendarError::Invalid("midnight is always valid".into()))?;
    occurrences_between(pool, from, to).await
}

/// One `StoredEvent` by row id.
pub async fn event_by_id(pool: &SqlitePool, id: i64) -> Result<Option<StoredEvent>, CalendarError> {
    let row = sqlx::query(&format!("SELECT {EVENT_COLUMNS} FROM events WHERE id = ?1"))
        .bind(id)
        .fetch_optional(pool)
        .await?;
    row.as_ref().map(StoredEvent::from_row).transpose()
}

// ---------------------------------------------------------------------------
// Local CRUD
// ---------------------------------------------------------------------------

/// Insert a local event; returns its row id.
pub async fn insert_local_event(
    pool: &SqlitePool,
    draft: &EventDraft,
) -> Result<i64, CalendarError> {
    draft.validate()?;
    let row = sqlx::query(
        "INSERT INTO events \
           (source, title, description, location, starts_at, ends_at, all_day, tzid, rrule, user_id, color) \
         VALUES ('local', ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) \
         RETURNING id",
    )
    .bind(draft.title.trim())
    .bind(draft.description.as_deref())
    .bind(draft.location.as_deref())
    .bind(format_timestamp(draft.starts_at))
    .bind(draft.ends_at.map(format_timestamp))
    .bind(i64::from(draft.all_day))
    .bind(draft.tzid.as_deref())
    .bind(draft.rrule.as_deref())
    .bind(draft.user_id)
    .bind(draft.color.as_deref())
    .fetch_one(pool)
    .await?;
    let id: i64 = row.try_get("id")?;
    Ok(id)
}

/// Overwrite a local event. Returns `false` when no **local** row has that id
/// (a Google row is never editable here: the next poll would overwrite it).
pub async fn update_local_event(
    pool: &SqlitePool,
    id: i64,
    draft: &EventDraft,
) -> Result<bool, CalendarError> {
    draft.validate()?;
    let result = sqlx::query(
        "UPDATE events SET title = ?2, description = ?3, location = ?4, starts_at = ?5, \
                ends_at = ?6, all_day = ?7, tzid = ?8, rrule = ?9, user_id = ?10, color = ?11, \
                updated_at = CURRENT_TIMESTAMP \
         WHERE id = ?1 AND source = 'local'",
    )
    .bind(id)
    .bind(draft.title.trim())
    .bind(draft.description.as_deref())
    .bind(draft.location.as_deref())
    .bind(format_timestamp(draft.starts_at))
    .bind(draft.ends_at.map(format_timestamp))
    .bind(i64::from(draft.all_day))
    .bind(draft.tzid.as_deref())
    .bind(draft.rrule.as_deref())
    .bind(draft.user_id)
    .bind(draft.color.as_deref())
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Delete a local event. Returns `false` when nothing was deleted.
pub async fn delete_local_event(pool: &SqlitePool, id: i64) -> Result<bool, CalendarError> {
    let result = sqlx::query("DELETE FROM events WHERE id = ?1 AND source = 'local'")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Tell every client the calendar moved on `date`, so it refetches.
///
/// Protocol v2 (T1.2/G13): the message carries only the date, never the
/// payload, so it is unspoofable and small.
pub fn publish_calendar_updated(date: NaiveDate) {
    realtime::publish(&ServerMessage::CalendarUpdated {
        date: date.format("%Y-%m-%d").to_string(),
    });
}

// ---------------------------------------------------------------------------
// Windowed Google polling (R-19)
// ---------------------------------------------------------------------------

/// One event as the Google response describes it, already normalised to
/// server-local wall clock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoogleEventInput {
    pub external_id: String,
    pub title: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub start: NaiveDateTime,
    pub end: Option<NaiveDateTime>,
    pub all_day: bool,
}

/// What one window replace did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowReplace {
    pub removed: u64,
    pub inserted: usize,
}

/// The `[start, end)` local window one poll covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PollWindow {
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
}

impl PollWindow {
    /// The window around `today`: [`WINDOW_PAST_DAYS`] back,
    /// [`WINDOW_FUTURE_DAYS`] forward, both snapped to local midnight.
    pub fn around(today: NaiveDate) -> Self {
        let start = (today - chrono::Duration::days(WINDOW_PAST_DAYS))
            .and_hms_opt(0, 0, 0)
            .expect("midnight is a valid time of day");
        let end = (today + chrono::Duration::days(WINDOW_FUTURE_DAYS))
            .and_hms_opt(0, 0, 0)
            .expect("midnight is a valid time of day");
        Self { start, end }
    }
}

/// **Replace** the calendar's whole window (R-19).
///
/// One transaction: every `source='google'` row for `calendar_id` whose start
/// falls inside the window is deleted, then everything the API just returned
/// is inserted. An event deleted remotely is therefore gone locally with no
/// tombstone logic, no `syncToken`, and no way for the two sides to drift —
/// which is the entire reason this is a replace and not an upsert.
pub async fn replace_google_window(
    pool: &SqlitePool,
    calendar_id: &str,
    window: PollWindow,
    events: &[GoogleEventInput],
) -> Result<WindowReplace, CalendarError> {
    let start = format_timestamp(window.start);
    let end = format_timestamp(window.end);

    let mut tx = pool.begin().await?;

    let removed = sqlx::query(
        "DELETE FROM events \
         WHERE source = 'google' AND calendar_id = ?1 AND starts_at >= ?2 AND starts_at < ?3",
    )
    .bind(calendar_id)
    .bind(&start)
    .bind(&end)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    let mut inserted = 0usize;
    for event in events {
        if event.start < window.start || event.start >= window.end {
            continue;
        }
        sqlx::query(
            "INSERT INTO events \
               (source, external_id, calendar_id, title, description, location, starts_at, ends_at, all_day) \
             VALUES ('google', ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
             ON CONFLICT (source, calendar_id, external_id) WHERE external_id IS NOT NULL              DO UPDATE SET \
               title = excluded.title, description = excluded.description, \
               location = excluded.location, starts_at = excluded.starts_at, \
               ends_at = excluded.ends_at, all_day = excluded.all_day, \
               updated_at = CURRENT_TIMESTAMP",
        )
        .bind(&event.external_id)
        .bind(calendar_id)
        .bind(&event.title)
        .bind(event.description.as_deref())
        .bind(event.location.as_deref())
        .bind(format_timestamp(event.start))
        .bind(event.end.map(format_timestamp))
        .bind(i64::from(event.all_day))
        .execute(&mut *tx)
        .await?;
        inserted += 1;
    }

    sqlx::query(
        "INSERT INTO google_sync_state (calendar_id, window_start, window_end, last_polled_at, last_success_at, last_error) \
         VALUES (?1, ?2, ?3, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, NULL) \
         ON CONFLICT (calendar_id) DO UPDATE SET \
           window_start = excluded.window_start, window_end = excluded.window_end, \
           last_polled_at = excluded.last_polled_at, last_success_at = excluded.last_success_at, \
           last_error = NULL",
    )
    .bind(calendar_id)
    .bind(&start)
    .bind(&end)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(WindowReplace { removed, inserted })
}

/// Record a failed poll without touching the events already in the window —
/// an offline hub keeps showing the last good answer (local-first, D0.2).
pub async fn record_poll_failure(
    pool: &SqlitePool,
    calendar_id: &str,
    error: &str,
) -> Result<(), CalendarError> {
    sqlx::query(
        "INSERT INTO google_sync_state (calendar_id, last_polled_at, last_error) \
         VALUES (?1, CURRENT_TIMESTAMP, ?2) \
         ON CONFLICT (calendar_id) DO UPDATE SET \
           last_polled_at = CURRENT_TIMESTAMP, last_error = excluded.last_error",
    )
    .bind(calendar_id)
    .bind(error)
    .execute(pool)
    .await?;
    Ok(())
}

/// Google's `events.list` response, only the fields the hub uses.
#[derive(Deserialize)]
struct EventsResponse {
    #[serde(default)]
    items: Vec<GoogleEvent>,
}

#[derive(Deserialize)]
struct GoogleEvent {
    id: String,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    location: Option<String>,
    start: Option<GoogleTime>,
    end: Option<GoogleTime>,
}

#[derive(Deserialize)]
struct GoogleTime {
    #[serde(rename = "dateTime")]
    date_time: Option<String>,
    date: Option<String>,
}

impl GoogleTime {
    /// Google sends either an offset-carrying RFC3339 `dateTime` or an
    /// all-day `date`. Both become server-local wall clock here, so the whole
    /// table is in one frame of reference (§P5.5 default 14).
    fn to_local(&self) -> Option<(NaiveDateTime, bool)> {
        if let Some(raw) = self.date_time.as_deref() {
            let parsed = chrono::DateTime::parse_from_rfc3339(raw).ok()?;
            return Some((parsed.with_timezone(&Local).naive_local(), false));
        }
        let raw = self.date.as_deref()?;
        let date = NaiveDate::parse_from_str(raw, "%Y-%m-%d").ok()?;
        Some((date.and_hms_opt(0, 0, 0)?, true))
    }
}

/// Parse an `events.list` body into the rows a window replace inserts.
///
/// Pure and public precisely so the acceptance test can drive a **committed
/// JSON fixture** through the same code path the network path uses, with no
/// service account anywhere in the run (§P5.5 default 24 / H-24).
pub fn parse_events_response(body: &str) -> Result<Vec<GoogleEventInput>, CalendarError> {
    let response: EventsResponse = serde_json::from_str(body)
        .map_err(|err| CalendarError::Invalid(format!("google response: {err}")))?;

    let mut events: Vec<GoogleEventInput> = response
        .items
        .into_iter()
        .filter(|item| item.status.as_deref() != Some("cancelled"))
        .filter_map(|item| {
            let (start, all_day) = item.start.as_ref()?.to_local()?;
            let end = item
                .end
                .as_ref()
                .and_then(|time| time.to_local())
                .map(|(value, _)| value);
            Some(GoogleEventInput {
                external_id: item.id,
                title: item.summary.unwrap_or_else(|| "(no title)".into()),
                description: item.description,
                location: item.location,
                start,
                end,
                all_day,
            })
        })
        .collect();

    // Sorted here rather than by `orderBy=startTime` on the request: R-19's
    // parameter conflict cannot recur if the parameter is never sent.
    events.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then(a.external_id.cmp(&b.external_id))
    });
    Ok(events)
}

/// Apply one already-fetched response body to the window and broadcast if
/// anything changed. This is the seam the fixture test drives.
pub async fn apply_poll_response(
    pool: &SqlitePool,
    calendar_id: &str,
    window: PollWindow,
    body: &str,
) -> Result<WindowReplace, CalendarError> {
    let events = parse_events_response(body)?;
    let stats = replace_google_window(pool, calendar_id, window, &events).await?;
    if stats.removed > 0 || stats.inserted > 0 {
        publish_calendar_updated(today_local());
    }
    Ok(stats)
}

// ---------------------------------------------------------------------------
// The poll loop, and the midnight force (W4)
// ---------------------------------------------------------------------------

static POLL_REQUESTS: AtomicU64 = AtomicU64::new(0);
static POLL_NOTIFY: OnceLock<Notify> = OnceLock::new();
static MIDNIGHT_HOOK: std::sync::Once = std::sync::Once::new();

fn poll_notify() -> &'static Notify {
    POLL_NOTIFY.get_or_init(Notify::new)
}

/// Ask the poll loop to run now instead of waiting out its interval.
pub fn request_poll() {
    POLL_REQUESTS.fetch_add(1, Ordering::SeqCst);
    poll_notify().notify_one();
}

/// How many forced polls have been requested this process. Exposed so the
/// acceptance test can prove the midnight tick reaches the poller (W4)
/// without needing credentials or a network.
pub fn poll_requests() -> u64 {
    POLL_REQUESTS.load(Ordering::SeqCst)
}

/// Register the day-rolled hook that forces a poll at midnight (W4).
///
/// Registered through `realtime::on_day_rolled` rather than by editing the
/// tick loop, exactly as `docs/HANDOFF.md` H-10 asks. Idempotent: the tick is
/// process-wide and so is this.
pub fn register_midnight_poll() {
    MIDNIGHT_HOOK.call_once(|| {
        realtime::on_day_rolled(std::sync::Arc::new(|date: String| {
            Box::pin(async move {
                tracing::info!(%date, "midnight rollover - forcing a calendar poll");
                request_poll();
            })
        }));
    });
}

/// Credentials read from a Google service account JSON key file.
#[derive(Clone, Deserialize)]
struct ServiceAccount {
    client_email: String,
    private_key: String,
}

struct PollerConfig {
    account: ServiceAccount,
    calendar_id: String,
}

fn load_config() -> Option<PollerConfig> {
    let path = std::env::var("GOOGLE_SERVICE_ACCOUNT_JSON").ok()?;
    let calendar_id = std::env::var("GOOGLE_CALENDAR_ID").unwrap_or_else(|_| "primary".into());
    let raw = std::fs::read_to_string(&path)
        .map_err(|err| tracing::warn!("cannot read {path}: {err}"))
        .ok()?;
    let account: ServiceAccount = serde_json::from_str(&raw)
        .map_err(|err| tracing::warn!("invalid service account json: {err}"))
        .ok()?;

    Some(PollerConfig {
        account,
        calendar_id,
    })
}

/// Start the polling loop and register the midnight force.
///
/// The hook is registered **whether or not** credentials exist: local events
/// still need `DayRolled` to move the day on, and if credentials appear later
/// the hub does not have to be restarted for the forced poll to work.
pub fn spawn_polling_task() {
    register_midnight_poll();

    let Some(config) = load_config() else {
        tracing::info!(
            "GOOGLE_SERVICE_ACCOUNT_JSON not set - local events only, no Google polling"
        );
        return;
    };

    tokio::spawn(async move {
        let client = reqwest::Client::new();
        loop {
            let window = PollWindow::around(today_local());
            match poll_once(&client, &config, window).await {
                Ok(stats) => {
                    tracing::info!(
                        removed = stats.removed,
                        inserted = stats.inserted,
                        "google calendar window replaced"
                    );
                    // T1.7 HANDOFF: `/health`'s `last_google_poll` is fed here.
                    crate::server::health::record_google_poll_success(Local::now());
                }
                Err(err) => {
                    tracing::error!("google calendar poll failed: {err}");
                    if let Ok(pool) = db::pool().await {
                        let _ =
                            record_poll_failure(pool, &config.calendar_id, &err.to_string()).await;
                    }
                }
            }

            // Either the interval elapses or the midnight tick forces a poll
            // (W4) — whichever happens first.
            tokio::select! {
                _ = tokio::time::sleep(POLL_INTERVAL) => {}
                _ = poll_notify().notified() => {
                    tracing::info!("forced calendar poll");
                }
            }
        }
    });
}

async fn poll_once(
    client: &reqwest::Client,
    config: &PollerConfig,
    window: PollWindow,
) -> Result<WindowReplace, Box<dyn std::error::Error + Send + Sync>> {
    let body = fetch_window(client, config, window).await?;
    let pool = db::pool().await?;
    Ok(apply_poll_response(pool, &config.calendar_id, window, &body).await?)
}

/// Fetch the window as raw JSON.
///
/// Deliberately **no `syncToken` and no `orderBy`** (R-19): a sync token is
/// rejected alongside `timeMin`/`timeMax`, and ordering is done in
/// [`parse_events_response`]. `singleEvents=true` is what makes Google expand
/// its own recurrences, so the hub only ever stores concrete occurrences from
/// this source.
async fn fetch_window(
    client: &reqwest::Client,
    config: &PollerConfig,
    window: PollWindow,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let token = access_token(client, &config.account).await?;
    let url = format!(
        "https://www.googleapis.com/calendar/v3/calendars/{}/events",
        urlencode(&config.calendar_id)
    );

    let body = client
        .get(url)
        .bearer_auth(token)
        .query(&[
            ("timeMin", rfc3339_local(window.start)),
            ("timeMax", rfc3339_local(window.end)),
            ("singleEvents", "true".to_string()),
            ("maxResults", "2500".to_string()),
        ])
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    Ok(body)
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
}

#[derive(serde::Serialize)]
struct Claims {
    iss: String,
    scope: String,
    aud: String,
    exp: i64,
    iat: i64,
}

async fn access_token(
    client: &reqwest::Client,
    account: &ServiceAccount,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let now = chrono::Utc::now().timestamp();
    let claims = Claims {
        iss: account.client_email.clone(),
        scope: SCOPE.to_string(),
        aud: TOKEN_URL.to_string(),
        exp: now + 3600,
        iat: now,
    };

    let key = jsonwebtoken::EncodingKey::from_rsa_pem(account.private_key.as_bytes())?;
    let assertion = jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256),
        &claims,
        &key,
    )?;

    let response: TokenResponse = client
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
            ("assertion", &assertion),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    Ok(response.access_token)
}

fn urlencode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Unit tests for the pure helpers (the DB-backed assertions are T2.4's
// acceptance suite, `tests/calendar_tests.rs`).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn naive(text: &str) -> NaiveDateTime {
        parse_timestamp(text).expect("test timestamp parses")
    }

    #[test]
    fn a_week_starts_on_sunday_and_has_seven_calendar_days() {
        // 2026-08-29 is a Saturday; its week starts on 2026-08-23.
        let days = week_days(NaiveDate::from_ymd_opt(2026, 8, 29).unwrap());
        assert_eq!(days.len(), 7);
        assert_eq!(days[0], NaiveDate::from_ymd_opt(2026, 8, 23).unwrap());
        assert_eq!(days[6], NaiveDate::from_ymd_opt(2026, 8, 29).unwrap());
        assert_eq!(days[0].weekday(), chrono::Weekday::Sun);

        // A Sunday is its own week start.
        let sunday = NaiveDate::from_ymd_opt(2026, 8, 23).unwrap();
        assert_eq!(week_start(sunday), sunday);
    }

    #[test]
    fn the_us_dst_week_still_has_seven_days_and_no_repeats() {
        // The week containing the US spring-forward (2026-03-08) is 167 hours
        // long; day arithmetic must not skip or duplicate a date.
        let days = week_days(NaiveDate::from_ymd_opt(2026, 3, 8).unwrap());
        assert_eq!(days[0], NaiveDate::from_ymd_opt(2026, 3, 8).unwrap());
        assert_eq!(days[6], NaiveDate::from_ymd_opt(2026, 3, 14).unwrap());
        for pair in days.windows(2) {
            assert_eq!(pair[1], pair[0] + chrono::Duration::days(1));
        }
    }

    #[test]
    fn timestamps_round_trip_through_the_stored_format() {
        let value = naive("2026-08-29T07:30:00");
        assert_eq!(format_timestamp(value), "2026-08-29T07:30:00");
        assert_eq!(parse_timestamp("2026-08-29T07:30"), Some(value));
        assert_eq!(
            parse_timestamp("2026-08-29"),
            Some(naive("2026-08-29T00:00:00"))
        );
        assert_eq!(parse_timestamp("not a time"), None);
    }

    #[test]
    fn rfc3339_local_never_relabels_local_time_as_utc() {
        // The v1 bug (R-28): `.single()` returning `None` fell through to
        // `naive.and_utc()`. Whatever this machine's zone is, the rendered
        // offset must be the *local* offset for that instant.
        let value = naive("2026-08-29T07:30:00");
        let rendered = rfc3339_local(value);
        let parsed = chrono::DateTime::parse_from_rfc3339(&rendered).expect("valid rfc3339");
        let expected = Local.offset_from_utc_datetime(&parsed.naive_utc());
        assert_eq!(
            parsed.offset().local_minus_utc(),
            expected.local_minus_utc()
        );
        assert!(rendered.starts_with("2026-08-29T07:30:00"), "{rendered}");
    }

    #[test]
    fn an_empty_or_unparsable_rule_is_an_error_not_a_panic() {
        let start = naive("2026-08-29T02:30:00");
        assert!(expand_recurrence(start, None, "", None, 10).is_err());
        assert!(expand_recurrence(start, None, "FREQ=NEVER", None, 10).is_err());
        assert!(expand_recurrence(start, Some("Mars/Olympus"), "FREQ=DAILY", None, 10).is_err());
    }

    #[test]
    fn a_draft_with_a_bad_rule_is_rejected_before_it_is_stored() {
        let draft = EventDraft {
            title: "Piano".into(),
            description: None,
            location: None,
            starts_at: naive("2026-08-29T16:00:00"),
            ends_at: Some(naive("2026-08-29T17:00:00")),
            all_day: false,
            tzid: None,
            rrule: Some("FREQ=NONSENSE".into()),
            user_id: None,
            color: None,
        };
        assert!(draft.validate().is_err());

        let good = EventDraft {
            rrule: Some("FREQ=WEEKLY;COUNT=4".into()),
            ..draft.clone()
        };
        good.validate().expect("a valid weekly rule is accepted");

        let empty_title = EventDraft {
            title: "   ".into(),
            ..good.clone()
        };
        assert!(empty_title.validate().is_err());

        let backwards = EventDraft {
            ends_at: Some(naive("2026-08-29T15:00:00")),
            ..good
        };
        assert!(backwards.validate().is_err());
    }

    #[test]
    fn an_all_day_occurrence_renders_as_a_date_and_a_timed_one_as_rfc3339() {
        let all_day = Occurrence {
            event_id: 4,
            source: "google".into(),
            title: "Half term".into(),
            start: naive("2026-10-26T00:00:00"),
            end: Some(naive("2026-10-31T00:00:00")),
            all_day: true,
            color: None,
            recurring: false,
        };
        let wire = all_day.to_calendar_event();
        assert_eq!(wire.start, "2026-10-26");
        assert_eq!(wire.end, "2026-10-31");
        assert!(wire.all_day);
        assert_eq!(wire.id, "google:4@20261026T000000");

        let timed = Occurrence {
            all_day: false,
            start: naive("2026-10-26T09:15:00"),
            end: Some(naive("2026-10-26T10:00:00")),
            ..all_day
        };
        let wire = timed.to_calendar_event();
        assert!(
            wire.start.starts_with("2026-10-26T09:15:00"),
            "{}",
            wire.start
        );
        // The TV and the phone both slice `HH:MM` out of chars 11..16.
        assert_eq!(&wire.start[11..16], "09:15");
    }
}
