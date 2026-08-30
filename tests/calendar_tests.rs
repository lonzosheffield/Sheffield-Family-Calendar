//! **T2.4 acceptance suite** — Calendar v2, `docs/reviews/PURPLE_TEAM.md` §P3:
//!
//! | # | Assertion | Test |
//! | --- | --- | --- |
//! | a | A fixture-driven Google poll upserts 3 events; a re-poll with a 2-event response leaves the window holding exactly 2, and the removed one is gone | [`t2_4_a_a_full_window_replace_removes_the_event_the_second_response_dropped`] |
//! | b | A 02:30 daily rule expanded across **both** the US and the UK DST transitions produces the correct local times, with a named assertion per boundary | [`t2_4_b_us_spring_forward_moves_the_0230_occurrence_into_edt`], [`t2_4_b_us_fall_back_keeps_0230_and_returns_to_est`], [`t2_4_b_uk_spring_forward_keeps_0230_and_moves_into_bst`], [`t2_4_b_uk_fall_back_keeps_0230_and_returns_to_gmt`] |
//! | c | The week view for a week containing a DST change has exactly 7 days with correct day boundaries | [`t2_4_c_a_dst_week_has_exactly_seven_days_with_correct_boundaries`] |
//! | d | `all(limit)` is enforced — a pathological RRULE returns at most `limit` and does not hang (2 s timeout) | [`t2_4_d_a_pathological_rrule_is_capped_by_all_limit_and_returns_inside_two_seconds`] |
//! | e | Deleting the last event of a day makes the panel render `Empty`, not the stale event | [`t2_4_e_deleting_the_last_event_of_a_day_renders_empty_not_the_stale_event`] |
//!
//! Plus W4, which the task line calls out separately: the midnight tick forces
//! a poll ([`t2_4_the_midnight_tick_forces_a_calendar_poll`]).
//!
//! No service account and no network anywhere in this file (§P5.5 default 24 /
//! H-24): the Google path is driven by the two committed fixtures in
//! `tests/fixtures/`, through
//! [`family_calendar::server::calendar::apply_poll_response`] — the same
//! function the live poller calls once it has a response body.
//!
//! Harness follows `tests/profiles_tests.rs` and `tests/whiteboard_tests.rs`:
//! one throwaway sqlite file per test process via `DATABASE_URL`, every
//! `#[server]` fn called directly (the real server-side implementation running
//! in-process), and one process-wide lock because the pools, the broadcast
//! sender and the `events` table are all shared.

#![cfg(feature = "server")]

use std::time::{Duration, Instant};

use chrono::{Datelike, NaiveDate, NaiveDateTime, Timelike};
use family_calendar::client::components::calendar::{CalendarPayload, CalendarState};
use family_calendar::server::api::calendar::{get_calendar_week, get_events_for_day};
use family_calendar::server::calendar as cal;
use family_calendar::server::db;
use sqlx::SqlitePool;

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// Serialises every test in this binary: the pools, `realtime::publish` and
/// the `events` table are all process-wide.
async fn calendar_lock() -> tokio::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await
}

fn init_test_env() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let base =
            std::env::temp_dir().join(format!("familyhub-calendar-tests-{}", std::process::id()));
        std::fs::create_dir_all(&base).expect("test scratch directory is creatable");
        let db_path = base.join("family.db");
        let url = format!(
            "sqlite://{}",
            db_path.display().to_string().replace('\\', "/")
        );
        std::env::set_var("DATABASE_URL", url);
        std::env::set_var("FAMILY_HUB_DATA_DIR", &base);
    });
}

/// A migrated write pool with an empty `events` table.
async fn fresh_pool() -> &'static SqlitePool {
    init_test_env();
    let pool = db::pool().await.expect("pools open and migrate");
    sqlx::query("DELETE FROM events")
        .execute(pool)
        .await
        .expect("events table is truncatable");
    sqlx::query("DELETE FROM google_sync_state")
        .execute(pool)
        .await
        .expect("sync state is truncatable");
    pool
}

fn fixture(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

fn naive(text: &str) -> NaiveDateTime {
    cal::parse_timestamp(text).unwrap_or_else(|| panic!("{text} parses"))
}

fn date(text: &str) -> NaiveDate {
    cal::parse_date(text).unwrap_or_else(|| panic!("{text} parses"))
}

async fn google_titles(pool: &SqlitePool, calendar_id: &str) -> Vec<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT external_id FROM events WHERE source = 'google' AND calendar_id = ?1 \
         ORDER BY external_id",
    )
    .bind(calendar_id)
    .fetch_all(pool)
    .await
    .expect("google rows are readable")
}

// ---------------------------------------------------------------------------
// (a) Windowed poll with a full-window replace (R-19)
// ---------------------------------------------------------------------------

/// **T2.4 (a).** Two committed fixtures, no service account, no network. The
/// first response has three events; the second has two. Because the poll
/// *replaces* the window rather than upserting into it, the event the second
/// response dropped is gone with no `status: "cancelled"` bookkeeping and no
/// sync token.
#[tokio::test]
async fn t2_4_a_a_full_window_replace_removes_the_event_the_second_response_dropped() {
    let _guard = calendar_lock().await;
    let pool = fresh_pool().await;
    let calendar_id = "family@sheffield.test";
    let window = cal::PollWindow::around(date("2026-08-29"));

    let first = cal::apply_poll_response(
        pool,
        calendar_id,
        window,
        &fixture("google_events_window_3.json"),
    )
    .await
    .expect("the three-event response applies");
    assert_eq!(first.inserted, 3, "the fixture carries three events");
    assert_eq!(first.removed, 0, "nothing was in the window to begin with");
    assert_eq!(
        google_titles(pool, calendar_id).await,
        vec![
            "sheffield-dentist-0002".to_string(),
            "sheffield-halfterm-0003".to_string(),
            "sheffield-swimming-0001".to_string(),
        ]
    );

    let second = cal::apply_poll_response(
        pool,
        calendar_id,
        window,
        &fixture("google_events_window_2.json"),
    )
    .await
    .expect("the two-event response applies");
    assert_eq!(second.inserted, 2, "the second fixture carries two events");
    assert_eq!(
        second.removed, 3,
        "the whole window is deleted before it is re-inserted — that is the replace"
    );

    let remaining = google_titles(pool, calendar_id).await;
    assert_eq!(
        remaining,
        vec![
            "sheffield-halfterm-0003".to_string(),
            "sheffield-swimming-0001".to_string(),
        ],
        "the window holds exactly the two events the second response listed"
    );
    assert!(
        !remaining.iter().any(|id| id == "sheffield-dentist-0002"),
        "the event missing from the second response must be gone, not stale"
    );

    // The replace also carries the edit that came with it.
    let summary: String = sqlx::query_scalar(
        "SELECT title FROM events WHERE external_id = 'sheffield-swimming-0001'",
    )
    .fetch_one(pool)
    .await
    .expect("the surviving event is readable");
    assert_eq!(summary, "Swimming lesson (moved to lane 3)");

    // And the window itself was recorded, so `/health` and the next poll can
    // see what was covered.
    let (start, end): (String, String) = sqlx::query_as(
        "SELECT window_start, window_end FROM google_sync_state WHERE calendar_id = ?1",
    )
    .bind(calendar_id)
    .fetch_one(pool)
    .await
    .expect("sync state row exists");
    assert_eq!(start, cal::format_timestamp(window.start));
    assert_eq!(end, cal::format_timestamp(window.end));
}

/// A poll that fails must not empty the window: local-first means the last
/// good answer stays on the wall while the internet is out.
#[tokio::test]
async fn a_failed_poll_records_the_error_and_leaves_the_window_intact() {
    let _guard = calendar_lock().await;
    let pool = fresh_pool().await;
    let calendar_id = "offline@sheffield.test";
    let window = cal::PollWindow::around(date("2026-08-29"));

    cal::apply_poll_response(
        pool,
        calendar_id,
        window,
        &fixture("google_events_window_3.json"),
    )
    .await
    .expect("the three-event response applies");

    cal::record_poll_failure(pool, calendar_id, "dns failure")
        .await
        .expect("a failure is recordable");

    assert_eq!(
        google_titles(pool, calendar_id).await.len(),
        3,
        "a failed poll must not delete the window it could not refresh"
    );
    let error: Option<String> =
        sqlx::query_scalar("SELECT last_error FROM google_sync_state WHERE calendar_id = ?1")
            .bind(calendar_id)
            .fetch_one(pool)
            .await
            .expect("sync state row exists");
    assert_eq!(error.as_deref(), Some("dns failure"));
}

/// A malformed body is an error, never a silent window wipe.
#[test]
fn a_malformed_google_response_is_an_error_not_an_empty_window() {
    assert!(cal::parse_events_response("{not json").is_err());
    let empty = cal::parse_events_response(r#"{"items": []}"#).expect("an empty list parses");
    assert!(empty.is_empty());
    let cancelled = cal::parse_events_response(
        r#"{"items":[{"id":"x","status":"cancelled","start":{"date":"2026-08-29"},"end":{"date":"2026-08-30"}}]}"#,
    )
    .expect("a cancelled item parses");
    assert!(cancelled.is_empty(), "cancelled items carry no occurrence");
}

// ---------------------------------------------------------------------------
// (b) rrule DST — one named assertion per boundary
// ---------------------------------------------------------------------------

/// `(local wall clock, UTC offset in seconds)` for every occurrence, read in
/// the rule's **own** timezone so the assertions do not depend on where this
/// machine happens to be.
fn expand(dtstart: &str, tzid: &str, rule: &str) -> Vec<(String, i32)> {
    let expansion = cal::expand_recurrence(naive(dtstart), Some(tzid), rule, None, 32)
        .unwrap_or_else(|err| panic!("{tzid} {rule} expands: {err}"));
    expansion
        .occurrences
        .iter()
        .map(|dt| {
            (
                dt.format("%Y-%m-%d %H:%M").to_string(),
                chrono::Offset::fix(&chrono::TimeZone::offset_from_utc_datetime(
                    &dt.timezone(),
                    &dt.naive_utc(),
                ))
                .local_minus_utc(),
            )
        })
        .collect()
}

const EST: i32 = -5 * 3600;
const EDT: i32 = -4 * 3600;
const GMT: i32 = 0;
const BST: i32 = 3600;

/// **T2.4 (b) — US spring forward, 2026-03-08.** 02:30 does not exist that
/// morning in `America/New_York` (the clock jumps 02:00 → 03:00), so the
/// occurrence is placed at the same *elapsed time from local midnight*, which
/// is 03:30 EDT. Every other occurrence stays at 02:30, and the offset moves
/// from EST to EDT across the boundary — proof the expansion is using a real
/// timezone database and not fixed-offset arithmetic.
#[test]
fn t2_4_b_us_spring_forward_moves_the_0230_occurrence_into_edt() {
    let occurrences = expand(
        "2026-03-05T02:30:00",
        "America/New_York",
        "FREQ=DAILY;COUNT=6",
    );
    assert_eq!(
        occurrences,
        vec![
            ("2026-03-05 02:30".to_string(), EST),
            ("2026-03-06 02:30".to_string(), EST),
            ("2026-03-07 02:30".to_string(), EST),
            ("2026-03-08 03:30".to_string(), EDT),
            ("2026-03-09 02:30".to_string(), EDT),
            ("2026-03-10 02:30".to_string(), EDT),
        ],
        "US spring-forward boundary"
    );
}

/// **T2.4 (b) — US fall back, 2026-11-01.** The repeated hour is 01:00–02:00,
/// so 02:30 is unambiguous: it stays 02:30 and the offset returns to EST.
#[test]
fn t2_4_b_us_fall_back_keeps_0230_and_returns_to_est() {
    let occurrences = expand(
        "2026-10-29T02:30:00",
        "America/New_York",
        "FREQ=DAILY;COUNT=6",
    );
    assert_eq!(
        occurrences,
        vec![
            ("2026-10-29 02:30".to_string(), EDT),
            ("2026-10-30 02:30".to_string(), EDT),
            ("2026-10-31 02:30".to_string(), EDT),
            ("2026-11-01 02:30".to_string(), EST),
            ("2026-11-02 02:30".to_string(), EST),
            ("2026-11-03 02:30".to_string(), EST),
        ],
        "US fall-back boundary"
    );
}

/// **T2.4 (b) — UK spring forward, 2026-03-29.** London jumps 01:00 GMT →
/// 02:00 BST, so 02:30 still exists that morning; the local time is unchanged
/// and only the offset moves.
#[test]
fn t2_4_b_uk_spring_forward_keeps_0230_and_moves_into_bst() {
    let occurrences = expand("2026-03-26T02:30:00", "Europe/London", "FREQ=DAILY;COUNT=6");
    assert_eq!(
        occurrences,
        vec![
            ("2026-03-26 02:30".to_string(), GMT),
            ("2026-03-27 02:30".to_string(), GMT),
            ("2026-03-28 02:30".to_string(), GMT),
            ("2026-03-29 02:30".to_string(), BST),
            ("2026-03-30 02:30".to_string(), BST),
            ("2026-03-31 02:30".to_string(), BST),
        ],
        "UK spring-forward boundary"
    );
}

/// **T2.4 (b) — UK fall back, 2026-10-25.** The repeated hour is 01:00–02:00
/// BST/GMT, so 02:30 is unambiguous and returns to GMT.
#[test]
fn t2_4_b_uk_fall_back_keeps_0230_and_returns_to_gmt() {
    let occurrences = expand("2026-10-22T02:30:00", "Europe/London", "FREQ=DAILY;COUNT=6");
    assert_eq!(
        occurrences,
        vec![
            ("2026-10-22 02:30".to_string(), BST),
            ("2026-10-23 02:30".to_string(), BST),
            ("2026-10-24 02:30".to_string(), BST),
            ("2026-10-25 02:30".to_string(), GMT),
            ("2026-10-26 02:30".to_string(), GMT),
            ("2026-10-27 02:30".to_string(), GMT),
        ],
        "UK fall-back boundary"
    );
}

/// The same four boundaries, but reached through the storage layer: a stored
/// recurring event expands into the range query the panels use.
#[tokio::test]
async fn a_stored_recurring_event_expands_across_a_dst_boundary() {
    let _guard = calendar_lock().await;
    let pool = fresh_pool().await;

    let draft = cal::EventDraft {
        title: "Night feed".into(),
        description: None,
        location: None,
        starts_at: naive("2026-03-05T02:30:00"),
        ends_at: Some(naive("2026-03-05T03:00:00")),
        all_day: false,
        tzid: Some("America/New_York".into()),
        rrule: Some("FREQ=DAILY;COUNT=6".into()),
        user_id: None,
        color: None,
    };
    let id = cal::insert_local_event(pool, &draft)
        .await
        .expect("a recurring event is storable");
    assert!(id > 0);

    let occurrences = cal::occurrences_between(
        pool,
        naive("2026-03-01T00:00:00"),
        naive("2026-03-20T00:00:00"),
    )
    .await
    .expect("the range expands");
    assert_eq!(
        occurrences.len(),
        6,
        "COUNT=6 yields six occurrences inside the window"
    );
    assert!(
        occurrences.iter().all(|occurrence| occurrence.recurring),
        "every occurrence is marked as coming from an expansion"
    );
    // Each occurrence keeps the DTSTART's own duration.
    for occurrence in &occurrences {
        let end = occurrence.end.expect("an end was stored");
        assert_eq!(end - occurrence.start, chrono::Duration::minutes(30));
    }
}

// ---------------------------------------------------------------------------
// (c) The week view
// ---------------------------------------------------------------------------

/// **T2.4 (c).** The week containing the US spring forward is 167 hours long
/// and the week containing the fall back is 169; both must still be exactly
/// seven days, Sunday first, with every event landing on its own day. A week
/// built by adding 24 hours seven times would be wrong on precisely these two
/// weeks — which is why the days come from calendar arithmetic.
#[tokio::test]
async fn t2_4_c_a_dst_week_has_exactly_seven_days_with_correct_boundaries() {
    let _guard = calendar_lock().await;
    let pool = fresh_pool().await;

    // 2026-03-08 (US spring forward) is a Sunday, so its week is 03-08..03-14.
    let week_dates = [
        "2026-03-08",
        "2026-03-09",
        "2026-03-10",
        "2026-03-11",
        "2026-03-12",
        "2026-03-13",
        "2026-03-14",
    ];
    for day in week_dates {
        // Noon exists on every date in every zone, and a first/last minute
        // pair pins the day boundaries themselves.
        for time in ["T00:00:00", "T12:00:00", "T23:59:00"] {
            let draft = cal::EventDraft {
                title: format!("{day}{time}"),
                description: None,
                location: None,
                starts_at: naive(&format!("{day}{time}")),
                ends_at: None,
                all_day: false,
                tzid: None,
                rrule: None,
                user_id: None,
                color: None,
            };
            cal::insert_local_event(pool, &draft)
                .await
                .expect("event stored");
        }
    }

    let week = get_calendar_week(Some("2026-03-11".into()))
        .await
        .expect("the week loads");

    assert_eq!(week.days.len(), 7, "a week is exactly seven days");
    assert_eq!(week.start, "2026-03-08", "weeks start on Sunday");
    assert_eq!(week.end, "2026-03-14");
    assert_eq!(week.days[0].weekday, "Sunday");
    assert_eq!(week.days[6].weekday, "Saturday");

    for (index, day) in week.days.iter().enumerate() {
        assert_eq!(day.date, week_dates[index], "day {index} is in order");
        assert_eq!(
            day.events.len(),
            3,
            "{} holds exactly its own three events, none borrowed from a neighbour",
            day.date
        );
        for event in &day.events {
            assert!(
                event.summary.starts_with(&day.date),
                "{} leaked onto {}",
                event.summary,
                day.date
            );
        }
    }
    assert!(!week.is_empty());

    // The UK boundary week (2026-03-29 is a Sunday) is the same shape.
    let uk = get_calendar_week(Some("2026-03-29".into()))
        .await
        .expect("the UK boundary week loads");
    assert_eq!(uk.days.len(), 7);
    assert_eq!(uk.start, "2026-03-29");
    assert_eq!(uk.end, "2026-04-04");
    assert!(
        uk.is_empty(),
        "a week with nothing in it reports empty, so the panel can say so"
    );

    // And the pure helper agrees, on the fall-back week too.
    let autumn = cal::week_days(date("2026-11-01"));
    assert_eq!(autumn[0], date("2026-11-01"));
    assert_eq!(autumn[6], date("2026-11-07"));
    for pair in autumn.windows(2) {
        assert_eq!(pair[1].signed_duration_since(pair[0]).num_days(), 1);
    }
    assert_eq!(autumn[0].weekday(), chrono::Weekday::Sun);
}

// ---------------------------------------------------------------------------
// (d) `all(limit)` is the only expansion path
// ---------------------------------------------------------------------------

/// **T2.4 (d).** Two pathological rules, both bounded, both inside a 2 s
/// timeout on a worker thread so a hang fails the test rather than wedging
/// the suite:
///
/// * `FREQ=SECONDLY` would produce an occurrence per second forever. This is
///   the runaway-**output** case, and it is where `all(limit)`'s cap does the
///   work: exactly `limit` occurrences come back and the result says it was
///   cut short.
/// * `FREQ=YEARLY;BYMONTH=2;BYMONTHDAY=30` matches **no** date that has ever
///   existed — February has never had a 30th. This is the runaway-**iteration**
///   case: nothing is ever emitted, so the cap can never bite, and what has to
///   hold instead is that the expansion still terminates (`rrule` walks to the
///   end of its supported year range and stops) well inside the timeout, with
///   no occurrences.
///
/// Measured on this machine: `FREQ=SECONDLY` returns 366 dates with
/// `limited = true`; the February-30th rule returns 0 dates with
/// `limited = false`, because it genuinely exhausted its search space rather
/// than being cut off.
#[tokio::test]
async fn t2_4_d_a_pathological_rrule_is_capped_by_all_limit_and_returns_inside_two_seconds() {
    const LIMIT: u16 = 366;

    // (rule, does it emit anything, does `all(limit)` cut it short)
    for (rule, expect_dates, expect_limited) in [
        ("FREQ=SECONDLY", true, true),
        ("FREQ=YEARLY;BYMONTH=2;BYMONTHDAY=30", false, false),
    ] {
        let started = Instant::now();
        let expansion = tokio::time::timeout(
            Duration::from_secs(2),
            tokio::task::spawn_blocking(move || {
                cal::expand_recurrence(
                    NaiveDate::from_ymd_opt(2026, 1, 1)
                        .unwrap()
                        .and_hms_opt(0, 0, 0)
                        .unwrap(),
                    Some("Europe/London"),
                    rule,
                    None,
                    LIMIT,
                )
            }),
        )
        .await
        .unwrap_or_else(|_| panic!("{rule} did not return within 2 s"))
        .expect("the expansion task did not panic")
        .unwrap_or_else(|err| panic!("{rule} expands: {err}"));
        let elapsed = started.elapsed();

        assert!(
            expansion.occurrences.len() <= usize::from(LIMIT),
            "{rule} returned {} occurrences, more than the limit of {LIMIT}",
            expansion.occurrences.len()
        );
        assert_eq!(
            expansion.limited, expect_limited,
            "{rule} misreported whether `all(limit)` cut it short"
        );
        assert_eq!(
            !expansion.occurrences.is_empty(),
            expect_dates,
            "{rule} produced an unexpected number of occurrences"
        );
        if expect_limited {
            assert_eq!(
                expansion.occurrences.len(),
                usize::from(LIMIT),
                "{rule} must be stopped *by the cap*, exactly at the cap"
            );
        }
        assert!(elapsed < Duration::from_secs(2), "{rule} took {elapsed:?}");
    }

    // The cap is a real cap, not an accident of the rule: a daily rule asked
    // for 3 occurrences returns 3, and says it stopped early.
    let short = cal::expand_recurrence(
        NaiveDate::from_ymd_opt(2026, 1, 1)
            .unwrap()
            .and_hms_opt(9, 0, 0)
            .unwrap(),
        None,
        "FREQ=DAILY",
        None,
        3,
    )
    .expect("a daily rule expands");
    assert_eq!(short.occurrences.len(), 3);
    assert!(short.limited);

    // And a stored event with a runaway rule cannot flood a day either: the
    // range query clips to the window it was asked for.
    let _guard = calendar_lock().await;
    let pool = fresh_pool().await;
    let draft = cal::EventDraft {
        title: "Tick".into(),
        description: None,
        location: None,
        starts_at: naive("2026-08-29T00:00:00"),
        ends_at: None,
        all_day: false,
        tzid: None,
        rrule: Some("FREQ=MINUTELY".into()),
        user_id: None,
        color: None,
    };
    cal::insert_local_event(pool, &draft).await.expect("stored");
    let started = Instant::now();
    let day = cal::occurrences_on(pool, date("2026-08-29"))
        .await
        .expect("the day loads");
    assert!(
        day.len() <= usize::from(cal::RECURRENCE_LIMIT),
        "a minutely rule must not put 1,440 rows on the television"
    );
    assert!(started.elapsed() < Duration::from_secs(2));
}

// ---------------------------------------------------------------------------
// (e) Empty, not stale (W3)
// ---------------------------------------------------------------------------

/// **T2.4 (e).** v1 cached events in a `OnceLock` that could never shrink, so
/// deleting the last event of a day left it on the wall. Here the read comes
/// from SQLite and the panel's state machine turns "no events" into `Empty` —
/// a different render from `Loading` and from `Error`.
#[tokio::test]
async fn t2_4_e_deleting_the_last_event_of_a_day_renders_empty_not_the_stale_event() {
    let _guard = calendar_lock().await;
    let pool = fresh_pool().await;
    let today = cal::today_local();
    let today_text = today.format("%Y-%m-%d").to_string();

    let draft = cal::EventDraft {
        title: "Last thing today".into(),
        description: None,
        location: None,
        starts_at: today
            .and_hms_opt(18, 0, 0)
            .expect("18:00 is a valid time of day"),
        ends_at: Some(today.and_hms_opt(19, 0, 0).unwrap()),
        all_day: false,
        tzid: None,
        rrule: None,
        user_id: None,
        color: None,
    };
    let id = cal::insert_local_event(pool, &draft)
        .await
        .expect("the event is storable");

    let events = get_events_for_day(today_text.clone())
        .await
        .expect("today loads");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].summary, "Last thing today");
    assert_eq!(
        CalendarState::resolve(
            Some(Ok(CalendarPayload::Day(events.clone()))),
            CalendarPayload::is_empty
        )
        .name(),
        "ready"
    );
    // The wire id names its source, so the phone knows this one is deletable.
    assert!(events[0].id.starts_with(&format!("local:{id}@")));

    assert!(
        cal::delete_local_event(pool, id)
            .await
            .expect("the delete runs"),
        "the row existed and was removed"
    );

    let after = get_events_for_day(today_text)
        .await
        .expect("today loads again");
    assert!(after.is_empty(), "the deleted event must not come back");
    let state = CalendarState::resolve(
        Some(Ok(CalendarPayload::Day(after))),
        CalendarPayload::is_empty,
    );
    assert_eq!(
        state.name(),
        "empty",
        "the panel renders Empty, not the stale event and not a spinner"
    );

    // Deleting the same row twice is not a second delete.
    assert!(!cal::delete_local_event(pool, id).await.expect("runs"));
}

/// A Google event is not editable through the local CRUD path: the next
/// window replace would simply bring it back, so the server refuses rather
/// than pretending.
#[tokio::test]
async fn a_google_event_cannot_be_deleted_through_the_local_crud_path() {
    let _guard = calendar_lock().await;
    let pool = fresh_pool().await;
    let window = cal::PollWindow::around(date("2026-08-29"));
    cal::apply_poll_response(
        pool,
        "family@sheffield.test",
        window,
        &fixture("google_events_window_3.json"),
    )
    .await
    .expect("the fixture applies");

    let id: i64 = sqlx::query_scalar("SELECT id FROM events WHERE source = 'google' LIMIT 1")
        .fetch_one(pool)
        .await
        .expect("a google row exists");
    assert!(
        !cal::delete_local_event(pool, id).await.expect("runs"),
        "a google row is not deletable locally"
    );
    let draft = cal::EventDraft {
        title: "Hijacked".into(),
        description: None,
        location: None,
        starts_at: naive("2026-08-29T17:00:00"),
        ends_at: None,
        all_day: false,
        tzid: None,
        rrule: None,
        user_id: None,
        color: None,
    };
    assert!(
        !cal::update_local_event(pool, id, &draft)
            .await
            .expect("runs"),
        "a google row is not editable locally"
    );
}

// ---------------------------------------------------------------------------
// W4 — the midnight tick forces a poll
// ---------------------------------------------------------------------------

/// The tick belongs to T1.2, so this is registered through
/// `realtime::on_day_rolled` (H-10) rather than by editing the loop. Running
/// the rollover must reach the poller, which is what stops a stale window from
/// surviving into the new day (W4).
#[tokio::test]
async fn t2_4_the_midnight_tick_forces_a_calendar_poll() {
    let _guard = calendar_lock().await;
    cal::register_midnight_poll();
    // Idempotent: registering twice must not double-register the hook.
    cal::register_midnight_poll();

    let before = cal::poll_requests();
    family_calendar::server::api::realtime::run_day_rolled("2026-08-30".into()).await;
    let after = cal::poll_requests();

    assert_eq!(
        after,
        before + 1,
        "the day-rolled hook forces exactly one poll"
    );
}

// ---------------------------------------------------------------------------
// The deterministic local resolver (R-28)
// ---------------------------------------------------------------------------

/// v1's `rfc3339_local` fell back to `naive.and_utc()` whenever `Local` could
/// not resolve a wall-clock time, silently relabelling local time as UTC on
/// the two days a year that matter. This asserts the replacement is a real
/// local instant with a real local offset, whatever zone this machine is in.
#[test]
fn rfc3339_local_is_deterministic_and_never_relabels_local_as_utc() {
    for text in [
        "2026-03-08T02:30:00",
        "2026-11-01T01:30:00",
        "2026-03-29T01:30:00",
        "2026-10-25T01:30:00",
    ] {
        let value = naive(text);
        let resolved = cal::resolve_local_forward(value);
        let rendered = cal::rfc3339_local(value);
        let parsed = chrono::DateTime::parse_from_rfc3339(&rendered).expect("valid rfc3339");
        assert_eq!(parsed.timestamp(), resolved.timestamp());

        // Whatever wall clock came back, it is the *earliest* instant that
        // renders it: resolving it again must land on the same instant.
        let again = cal::resolve_local_forward(resolved.naive_local());
        assert_eq!(
            again.timestamp(),
            resolved.timestamp(),
            "{text} did not resolve deterministically"
        );

        // The result is at or after the requested wall clock, never before —
        // a skipped time steps forward, it does not step back a day.
        assert!(resolved.naive_local() >= value, "{text} moved backwards");
        assert!(
            resolved.naive_local() - value < chrono::Duration::hours(3),
            "{text} moved further than any real DST gap"
        );
        assert_eq!(resolved.naive_local().second(), value.second());
    }
}
