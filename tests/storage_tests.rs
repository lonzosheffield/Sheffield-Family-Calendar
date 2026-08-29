#![cfg(feature = "server")]
//! T1.1 — migrations, pools, pragmas, the WAL checkpoint hook and the DST-safe
//! date helper. PURPLE §P3 T1.1 assertions (a)–(f) are named in the test
//! function docs below.

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicU32, Ordering};

use chrono::{DateTime, LocalResult, NaiveDate, NaiveDateTime, TimeZone};
use family_calendar::server::db;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

static SCRATCH_COUNTER: AtomicU32 = AtomicU32::new(0);

/// A fresh, empty scratch directory for one test.
fn scratch_dir(name: &str) -> PathBuf {
    let n = SCRATCH_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("familyhub-t11-{name}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// sqlx wants forward slashes in a sqlite URL, even on Windows.
fn sqlite_url(path: &Path) -> String {
    format!("sqlite://{}", path.display().to_string().replace('\\', "/"))
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

fn v1_fixture() -> PathBuf {
    fixtures_dir().join("family_v1.db")
}

async fn table_names(pool: &SqlitePool) -> Vec<String> {
    sqlx::query("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .fetch_all(pool)
        .await
        .expect("sqlite_master")
        .into_iter()
        .map(|row| row.get::<String, _>("name"))
        .collect()
}

async fn column_names(pool: &SqlitePool, table: &str) -> Vec<String> {
    sqlx::query(&format!("PRAGMA table_info({table})"))
        .fetch_all(pool)
        .await
        .expect("table_info")
        .into_iter()
        .map(|row| row.get::<String, _>("name"))
        .collect()
}

async fn count(pool: &SqlitePool, table: &str) -> i64 {
    sqlx::query(&format!("SELECT COUNT(*) AS n FROM {table}"))
        .fetch_one(pool)
        .await
        .expect("count")
        .get::<i64, _>("n")
}

// ---------------------------------------------------------------------------
// (a) a fresh database gets every table
// ---------------------------------------------------------------------------

/// PURPLE §P3 T1.1 (a): fresh DB → the migrator runs → all tables present.
#[tokio::test]
async fn fresh_database_runs_every_embedded_migration() {
    let dir = scratch_dir("fresh");
    let db_path = dir.join("family.db");
    let pools = db::open_pools(&sqlite_url(&db_path)).await.expect("pools");
    db::migrate(&pools.write).await.expect("migrate");

    let tables = table_names(&pools.read).await;
    for expected in [
        "_sqlx_migrations",
        "custom_tasks",
        "daily_routine_logs",
        "events",
        "google_sync_state",
        "mutation_log",
        "routine_templates",
        "settings",
        "whiteboard_strokes",
    ] {
        assert!(
            tables.iter().any(|t| t == expected),
            "table {expected} missing; got {tables:?}"
        );
    }

    assert!(
        column_names(&pools.read, "custom_tasks")
            .await
            .contains(&"due_date".to_string()),
        "0002_core must add custom_tasks.due_date"
    );

    // Both migrations recorded, and the eight routine templates seeded.
    assert_eq!(
        db::migration_version(&pools.read).await.expect("version"),
        Some(2)
    );
    assert_eq!(count(&pools.read, "routine_templates").await, 8);

    // Re-running is a no-op, not an error.
    db::migrate(&pools.write).await.expect("second migrate");
    assert_eq!(count(&pools.read, "routine_templates").await, 8);

    pools.close().await;
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// (b) an existing v1 family.db is baselined and keeps every row
// ---------------------------------------------------------------------------

/// PURPLE §P3 T1.1 (b): a fixture copy of a v1 `family.db` with routine logs is
/// baselined, migrated, and **every log row survives**.
#[tokio::test]
async fn v1_database_is_baselined_and_every_log_row_survives() {
    let fixture = v1_fixture();
    assert!(
        fixture.is_file(),
        "missing v1 fixture {} — regenerate with `cargo test --features server \
         --test storage_tests -- --ignored generate_v1_fixture`",
        fixture.display()
    );

    let dir = scratch_dir("v1");
    let db_path = dir.join("family.db");
    std::fs::copy(&fixture, &db_path).expect("copy fixture");

    // What the v1 database held before anything touched it.
    let url = sqlite_url(&db_path);
    let before = db::connect(&url).await.expect("open fixture");
    assert!(
        !table_names(&before)
            .await
            .contains(&"_sqlx_migrations".to_string()),
        "the fixture must predate sqlx migrations, otherwise it proves nothing"
    );
    let logs_before: Vec<(i64, i64, String)> = sqlx::query_as(
        "SELECT user_id, template_id, date_logged FROM daily_routine_logs \
         ORDER BY user_id, template_id, date_logged",
    )
    .fetch_all(&before)
    .await
    .expect("v1 logs");
    let tasks_before = count(&before, "custom_tasks").await;
    assert!(
        logs_before.len() >= 10,
        "fixture should carry real routine history, got {} rows",
        logs_before.len()
    );
    before.close().await;

    // Migrate it the way the server does at boot.
    let pools = db::open_pools(&url).await.expect("pools");
    db::migrate(&pools.write)
        .await
        .expect("migrate v1 database");

    // 0001 is recorded as *applied*, not re-run, and 0002 landed on top.
    let applied: Vec<(i64, String)> =
        sqlx::query_as("SELECT version, description FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&pools.read)
            .await
            .expect("_sqlx_migrations");
    assert_eq!(
        applied.iter().map(|(v, _)| *v).collect::<Vec<_>>(),
        vec![1, 2],
        "expected 0001 baselined and 0002 applied, got {applied:?}"
    );
    assert_eq!(
        db::migration_version(&pools.read).await.expect("version"),
        Some(2)
    );

    // Every single routine log row survived, byte for byte.
    let logs_after: Vec<(i64, i64, String)> = sqlx::query_as(
        "SELECT user_id, template_id, date_logged FROM daily_routine_logs \
         ORDER BY user_id, template_id, date_logged",
    )
    .fetch_all(&pools.read)
    .await
    .expect("migrated logs");
    assert_eq!(
        logs_after, logs_before,
        "migrating a v1 database must not lose or alter a single routine log row"
    );
    assert_eq!(count(&pools.read, "custom_tasks").await, tasks_before);

    // ...and the new schema is there for the rest of Phase 1 to build on.
    let tables = table_names(&pools.read).await;
    for expected in ["events", "whiteboard_strokes", "settings", "mutation_log"] {
        assert!(tables.contains(&expected.to_string()), "{expected} missing");
    }
    assert!(column_names(&pools.read, "custom_tasks")
        .await
        .contains(&"due_date".to_string()));

    pools.close().await;
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// (c) restore drill
// ---------------------------------------------------------------------------

/// PURPLE §P3 T1.1 (c): `VACUUM INTO` a copy, delete the original, restore the
/// copy, re-migrate, assert row counts identical.
#[tokio::test]
async fn vacuum_into_backup_restores_to_identical_row_counts() {
    let dir = scratch_dir("restore");
    let db_path = dir.join("family.db");
    let backup_path = dir.join("family-backup.db");
    let url = sqlite_url(&db_path);

    let pools = db::open_pools(&url).await.expect("pools");
    db::migrate(&pools.write).await.expect("migrate");

    for user_id in 1..=4u32 {
        for template_id in 1..=6u32 {
            db::set_routine_completion(&pools.write, user_id, template_id, true, "2026-03-14")
                .await
                .expect("log");
        }
    }
    for i in 0..25 {
        db::insert_stroke(
            &pools.write,
            db::DEFAULT_BOARD_ID,
            "restore-client",
            "#e8a838",
            4.0,
            &format!("[[0.1,0.1],[0.2,{}]]", i as f64 / 100.0),
        )
        .await
        .expect("stroke");
    }

    let logs = count(&pools.read, "daily_routine_logs").await;
    let strokes = count(&pools.read, "whiteboard_strokes").await;
    let templates = count(&pools.read, "routine_templates").await;
    assert_eq!(logs, 24);
    assert_eq!(strokes, 25);

    // `VACUUM INTO` is the only safe hot backup: it takes a read lock and
    // writes a consistent, WAL-free copy.
    sqlx::query("VACUUM INTO ?1")
        .bind(backup_path.display().to_string())
        .execute(&pools.write)
        .await
        .expect("vacuum into");
    assert!(backup_path.is_file());

    pools.close().await;

    // Blow the live database away, sidecars and all.
    for suffix in ["", "-wal", "-shm"] {
        let victim = PathBuf::from(format!("{}{suffix}", db_path.display()));
        let _ = std::fs::remove_file(&victim);
    }
    assert!(!db_path.exists());

    // Restore and boot again.
    std::fs::copy(&backup_path, &db_path).expect("restore");
    let restored = db::open_pools(&url).await.expect("restored pools");
    db::migrate(&restored.write).await.expect("re-migrate");

    assert_eq!(count(&restored.read, "daily_routine_logs").await, logs);
    assert_eq!(count(&restored.read, "whiteboard_strokes").await, strokes);
    assert_eq!(count(&restored.read, "routine_templates").await, templates);
    assert_eq!(
        db::migration_version(&restored.read)
            .await
            .expect("version"),
        Some(2)
    );

    restored.close().await;
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// (d) pragmas
// ---------------------------------------------------------------------------

/// PURPLE §P3 T1.1 (d): `PRAGMA journal_mode` is `wal` and `busy_timeout` is
/// 30000 — on **both** pools — and the checkpoint hook truncates the WAL.
#[tokio::test]
async fn pragmas_are_wal_normal_and_thirty_second_busy_timeout() {
    let dir = scratch_dir("pragmas");
    let db_path = dir.join("family.db");
    let pools = db::open_pools(&sqlite_url(&db_path)).await.expect("pools");
    db::migrate(&pools.write).await.expect("migrate");

    for (label, pool) in [("read", &pools.read), ("write", &pools.write)] {
        let journal_mode: String = sqlx::query("PRAGMA journal_mode")
            .fetch_one(pool)
            .await
            .expect("journal_mode")
            .get(0);
        assert_eq!(
            journal_mode.to_lowercase(),
            "wal",
            "{label} pool journal mode"
        );

        let busy_timeout: i64 = sqlx::query("PRAGMA busy_timeout")
            .fetch_one(pool)
            .await
            .expect("busy_timeout")
            .get(0);
        assert_eq!(busy_timeout, 30_000, "{label} pool busy timeout (ms)");

        // synchronous=NORMAL is 1 in SQLite's enumeration.
        let synchronous: i64 = sqlx::query("PRAGMA synchronous")
            .fetch_one(pool)
            .await
            .expect("synchronous")
            .get(0);
        assert_eq!(synchronous, 1, "{label} pool synchronous=NORMAL");

        let foreign_keys: i64 = sqlx::query("PRAGMA foreign_keys")
            .fetch_one(pool)
            .await
            .expect("foreign_keys")
            .get(0);
        assert_eq!(foreign_keys, 1, "{label} pool foreign_keys");
    }

    assert_eq!(db::READ_POOL_MAX_CONNECTIONS, 5);
    assert_eq!(db::WRITE_POOL_MAX_CONNECTIONS, 1);
    assert_eq!(pools.read.options().get_max_connections(), 5);
    assert_eq!(pools.write.options().get_max_connections(), 1);

    // Write enough to grow the -wal sidecar, then prove the midnight hook
    // truncates it back to nothing.
    for i in 0..400 {
        db::insert_stroke(
            &pools.write,
            db::DEFAULT_BOARD_ID,
            "wal-client",
            "#3b6ea5",
            3.0,
            &format!("[[0.0,0.0],[1.0,{}]]", i as f64 / 400.0),
        )
        .await
        .expect("stroke");
    }
    let wal_path = PathBuf::from(format!("{}-wal", db_path.display()));
    let grown = std::fs::metadata(&wal_path).map(|m| m.len()).unwrap_or(0);
    assert!(
        grown > 0,
        "WAL journalling should have produced a -wal file"
    );

    db::checkpoint_truncate(&pools.write)
        .await
        .expect("wal_checkpoint(TRUNCATE)");
    let truncated = std::fs::metadata(&wal_path).map(|m| m.len()).unwrap_or(0);
    assert_eq!(
        truncated, 0,
        "wal_checkpoint(TRUNCATE) must truncate the -wal file (was {grown} bytes)"
    );
    // The data is still there — the checkpoint folded it into the main file.
    assert_eq!(count(&pools.read, "whiteboard_strokes").await, 400);

    pools.close().await;
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// (e) 20 concurrent writers, zero SQLITE_BUSY
// ---------------------------------------------------------------------------

/// PURPLE §P3 T1.1 (e): 20 concurrent writers via the write pool complete with
/// zero `SQLITE_BUSY`, while readers hammer the read pool at the same time.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn twenty_concurrent_writers_see_no_sqlite_busy() {
    let dir = scratch_dir("writers");
    let db_path = dir.join("family.db");
    let pools = db::open_pools(&sqlite_url(&db_path)).await.expect("pools");
    db::migrate(&pools.write).await.expect("migrate");

    let readers: Vec<_> = (0..5)
        .map(|_| {
            let read = pools.read.clone();
            tokio::spawn(async move {
                for _ in 0..40 {
                    db::board_snapshot(&read, db::DEFAULT_BOARD_ID)
                        .await
                        .map_err(|err| err.to_string())?;
                }
                Ok::<(), String>(())
            })
        })
        .collect();

    let writers: Vec<_> = (0..20)
        .map(|i| {
            let write = pools.write.clone();
            tokio::spawn(async move {
                db::insert_stroke(
                    &write,
                    db::DEFAULT_BOARD_ID,
                    &format!("client-{i}"),
                    "#c0392b",
                    2.5,
                    &format!("[[0.0,0.0],[{}, 0.5]]", i as f64 / 20.0),
                )
                .await
                .map_err(|err| err.to_string())
            })
        })
        .collect();

    let mut seqs = Vec::new();
    for writer in writers {
        match writer.await.expect("writer task") {
            Ok(stroke) => seqs.push(stroke.seq),
            Err(err) => panic!("a concurrent writer failed: {err}"),
        }
    }
    for reader in readers {
        if let Err(err) = reader.await.expect("reader task") {
            panic!("a concurrent reader failed: {err}");
        }
    }

    seqs.sort_unstable();
    assert_eq!(
        seqs,
        (1..=20).collect::<Vec<i64>>(),
        "every writer should get its own dense seq"
    );
    assert_eq!(count(&pools.read, "whiteboard_strokes").await, 20);

    pools.close().await;
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// (f) the DST helper
// ---------------------------------------------------------------------------

fn ndt(y: i32, m: u32, d: u32, h: u32, min: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(y, m, d)
        .expect("valid date")
        .and_hms_opt(h, min, 0)
        .expect("valid time")
}

/// A fixed-offset marker for the synthetic zones below.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TestOffset(i32);

impl chrono::Offset for TestOffset {
    fn fix(&self) -> chrono::FixedOffset {
        chrono::FixedOffset::east_opt(self.0).expect("in-range offset")
    }
}

impl std::fmt::Display for TestOffset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", chrono::Offset::fix(self))
    }
}

const EDT: i32 = -4 * 3600;
const EST: i32 = -5 * 3600;

/// US Eastern for 2025, hand-rolled so the test does not depend on the host's
/// timezone database or on adding `chrono-tz` (a crate this task does not own
/// `Cargo.toml` to add). DST begins 09 Mar 02:00 → 03:00 and ends 02 Nov
/// 02:00 → 01:00, so 01:00–02:00 on 02 Nov happens **twice**.
#[derive(Clone, Copy, Debug)]
struct TestEastern;

impl TimeZone for TestEastern {
    type Offset = TestOffset;

    fn from_offset(_offset: &TestOffset) -> Self {
        TestEastern
    }

    fn offset_from_local_date(&self, _local: &NaiveDate) -> LocalResult<TestOffset> {
        LocalResult::Single(TestOffset(EST))
    }

    fn offset_from_local_datetime(&self, local: &NaiveDateTime) -> LocalResult<TestOffset> {
        let gap_start = ndt(2025, 3, 9, 2, 0);
        let gap_end = ndt(2025, 3, 9, 3, 0);
        let overlap_start = ndt(2025, 11, 2, 1, 0);
        let overlap_end = ndt(2025, 11, 2, 2, 0);

        if *local >= gap_start && *local < gap_end {
            // The hour that never happened.
            LocalResult::None
        } else if *local >= overlap_start && *local < overlap_end {
            // The hour that happened twice: EDT first, then EST.
            LocalResult::Ambiguous(TestOffset(EDT), TestOffset(EST))
        } else if *local >= gap_end && *local < overlap_start {
            LocalResult::Single(TestOffset(EDT))
        } else {
            LocalResult::Single(TestOffset(EST))
        }
    }

    fn offset_from_utc_date(&self, _utc: &NaiveDate) -> TestOffset {
        TestOffset(EST)
    }

    fn offset_from_utc_datetime(&self, utc: &NaiveDateTime) -> TestOffset {
        if *utc >= ndt(2025, 3, 9, 7, 0) && *utc < ndt(2025, 11, 2, 6, 0) {
            TestOffset(EDT)
        } else {
            TestOffset(EST)
        }
    }
}

/// A zone that skips **midnight itself** when DST begins (as America/Santiago
/// does): 2025-09-07 has no 00:00–01:00 at all.
#[derive(Clone, Copy, Debug)]
struct TestSkippedMidnight;

const SKIP_STD: i32 = -4 * 3600;
const SKIP_DST: i32 = -3 * 3600;

impl TimeZone for TestSkippedMidnight {
    type Offset = TestOffset;

    fn from_offset(_offset: &TestOffset) -> Self {
        TestSkippedMidnight
    }

    fn offset_from_local_date(&self, _local: &NaiveDate) -> LocalResult<TestOffset> {
        LocalResult::Single(TestOffset(SKIP_STD))
    }

    fn offset_from_local_datetime(&self, local: &NaiveDateTime) -> LocalResult<TestOffset> {
        let gap_start = ndt(2025, 9, 7, 0, 0);
        let gap_end = ndt(2025, 9, 7, 1, 0);

        if *local >= gap_start && *local < gap_end {
            LocalResult::None
        } else if *local >= gap_end {
            LocalResult::Single(TestOffset(SKIP_DST))
        } else {
            LocalResult::Single(TestOffset(SKIP_STD))
        }
    }

    fn offset_from_utc_date(&self, _utc: &NaiveDate) -> TestOffset {
        TestOffset(SKIP_STD)
    }

    fn offset_from_utc_datetime(&self, utc: &NaiveDateTime) -> TestOffset {
        if *utc >= ndt(2025, 9, 7, 4, 0) {
            TestOffset(SKIP_DST)
        } else {
            TestOffset(SKIP_STD)
        }
    }
}

fn utc_of<Tz: TimeZone>(moment: &DateTime<Tz>) -> NaiveDateTime {
    moment.naive_utc()
}

/// PURPLE §P3 T1.1 (f): the DST-ambiguity unit test on the date helper — an
/// ambiguous local time resolves to the **earliest** offset.
#[test]
fn ambiguous_local_time_resolves_to_the_earliest_offset() {
    // 01:30 on 2 Nov 2025 exists twice in US Eastern.
    let ambiguous = ndt(2025, 11, 2, 1, 30);
    assert!(
        matches!(
            TestEastern.offset_from_local_datetime(&ambiguous),
            LocalResult::Ambiguous(_, _)
        ),
        "the fixture zone must actually be ambiguous here, or this proves nothing"
    );

    let resolved = db::resolve_local(&TestEastern, ambiguous).expect("ambiguous still resolves");

    // The earliest of the two instants is the pre-transition one: EDT (-4),
    // i.e. 05:30 UTC. The later, EST (-5), would be 06:30 UTC.
    assert_eq!(
        chrono::Offset::fix(resolved.offset()).local_minus_utc(),
        EDT,
        "resolve_local must pick the earliest (pre-transition) offset"
    );
    assert_eq!(utc_of(&resolved), ndt(2025, 11, 2, 5, 30));

    let latest = TestEastern
        .from_local_datetime(&ambiguous)
        .latest()
        .expect("latest exists too");
    assert_eq!(utc_of(&latest), ndt(2025, 11, 2, 6, 30));
    assert!(
        resolved < latest,
        "earliest must be strictly before latest, otherwise .earliest() is doing nothing"
    );

    // An unambiguous time is unaffected.
    let plain = ndt(2025, 6, 1, 9, 0);
    let plain_resolved = db::resolve_local(&TestEastern, plain).expect("unambiguous");
    assert_eq!(
        chrono::Offset::fix(plain_resolved.offset()).local_minus_utc(),
        EDT
    );

    // The hour that never happened has no instant at all.
    assert!(db::resolve_local(&TestEastern, ndt(2025, 3, 9, 2, 30)).is_none());
}

/// The midnight tick must land on the first instant the local clock reads
/// 00:00 across both transitions — and must not stall in a zone that skips
/// midnight entirely.
#[test]
fn next_local_midnight_is_correct_across_both_dst_boundaries() {
    // Into the fall-back day: midnight on 2 Nov is still EDT (-4).
    let before_fall_back = TestEastern
        .from_local_datetime(&ndt(2025, 11, 1, 12, 0))
        .earliest()
        .expect("noon exists");
    let rollover = db::next_local_midnight(&TestEastern, &before_fall_back).expect("rollover");
    assert_eq!(
        rollover.date_naive(),
        NaiveDate::from_ymd_opt(2025, 11, 2).unwrap()
    );
    assert_eq!(utc_of(&rollover), ndt(2025, 11, 2, 4, 0));
    assert_eq!(
        chrono::Offset::fix(rollover.offset()).local_minus_utc(),
        EDT
    );

    // Into the spring-forward day: midnight on 9 Mar is still EST (-5); the
    // skipped hour is 02:00, not 00:00.
    let before_spring_forward = TestEastern
        .from_local_datetime(&ndt(2025, 3, 8, 12, 0))
        .earliest()
        .expect("noon exists");
    let rollover = db::next_local_midnight(&TestEastern, &before_spring_forward).expect("rollover");
    assert_eq!(utc_of(&rollover), ndt(2025, 3, 9, 5, 0));
    assert_eq!(
        chrono::Offset::fix(rollover.offset()).local_minus_utc(),
        EST
    );

    // A zone with no midnight at all rolls over at the first wall-clock time
    // that exists (01:00), rather than returning None and stalling the tick.
    let before = TestSkippedMidnight
        .from_local_datetime(&ndt(2025, 9, 6, 12, 0))
        .earliest()
        .expect("noon exists");
    let rollover = db::next_local_midnight(&TestSkippedMidnight, &before).expect("rollover");
    assert_eq!(utc_of(&rollover), ndt(2025, 9, 7, 4, 0));
    assert_eq!(
        rollover.date_naive(),
        NaiveDate::from_ymd_opt(2025, 9, 7).unwrap()
    );
}

// ---------------------------------------------------------------------------
// storage helpers added by this task
// ---------------------------------------------------------------------------

async fn migrated_memory_pool() -> SqlitePool {
    let pool = db::connect("sqlite::memory:").await.expect("memory pool");
    db::migrate(&pool).await.expect("migrate");
    pool
}

#[tokio::test]
async fn strokes_are_ordered_by_seq_and_cleared_by_the_watermark() {
    let pool = migrated_memory_pool().await;

    for i in 0..5 {
        db::insert_stroke(
            &pool,
            db::DEFAULT_BOARD_ID,
            if i % 2 == 0 { "alice" } else { "bob" },
            "#222222",
            3.0,
            &format!("[[0.0,0.0],[0.5,{i}]]"),
        )
        .await
        .expect("stroke");
    }

    let snapshot = db::board_snapshot(&pool, db::DEFAULT_BOARD_ID)
        .await
        .expect("snapshot");
    assert_eq!(snapshot.len(), 5);
    assert_eq!(
        snapshot.iter().map(|s| s.seq).collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5]
    );

    // Undo removes only the caller's last stroke.
    let undone = db::undo_last_stroke(&pool, db::DEFAULT_BOARD_ID, "alice")
        .await
        .expect("undo");
    assert_eq!(undone, Some(5));
    let snapshot = db::board_snapshot(&pool, db::DEFAULT_BOARD_ID)
        .await
        .expect("snapshot");
    assert_eq!(snapshot.len(), 4);
    assert!(snapshot.iter().any(|s| s.client_id == "bob"));

    // Clearing stamps the watermark: the snapshot empties, the rows remain
    // for T1.6's compaction pass.
    let cleared = db::clear_board(&pool, db::DEFAULT_BOARD_ID)
        .await
        .expect("clear");
    assert_eq!(cleared, 4);
    assert!(db::board_snapshot(&pool, db::DEFAULT_BOARD_ID)
        .await
        .expect("snapshot")
        .is_empty());
    assert_eq!(count(&pool, "whiteboard_strokes").await, 4);

    // A stroke drawn after the clear starts a fresh visible board. Its `seq`
    // continues above the cleared rows (which are still on disk until T1.6
    // compacts them) but reuses the number freed by the undo — `seq` orders
    // the live board, it is not a lifetime-unique id.
    db::insert_stroke(
        &pool,
        db::DEFAULT_BOARD_ID,
        "alice",
        "#222222",
        3.0,
        "[[0.9,0.9],[1.0,1.0]]",
    )
    .await
    .expect("stroke");
    let snapshot = db::board_snapshot(&pool, db::DEFAULT_BOARD_ID)
        .await
        .expect("snapshot");
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].seq, 5);

    pool.close().await;
}

#[tokio::test]
async fn settings_round_trip_and_overwrite() {
    let pool = migrated_memory_pool().await;

    assert_eq!(db::get_setting(&pool, "week_start").await.unwrap(), None);
    db::set_setting(&pool, "week_start", "sunday")
        .await
        .expect("set");
    assert_eq!(
        db::get_setting(&pool, "week_start").await.unwrap(),
        Some("sunday".to_string())
    );
    db::set_setting(&pool, "week_start", "monday")
        .await
        .expect("overwrite");
    assert_eq!(
        db::get_setting(&pool, "week_start").await.unwrap(),
        Some("monday".to_string())
    );
    assert_eq!(count(&pool, "settings").await, 1);

    pool.close().await;
}

// ---------------------------------------------------------------------------
// fixture generation (run explicitly, output committed)
// ---------------------------------------------------------------------------

/// Regenerates `tests/fixtures/family_v1.db` — a database written exactly the
/// way the pre-migration build wrote one: v1 DDL verbatim, default (`delete`)
/// journalling, and **no** `_sqlx_migrations` table.
///
/// The output is committed, so this is `#[ignore]`d; run it deliberately with
/// `cargo test --features server --test storage_tests -- --ignored \
/// generate_v1_fixture` if the v1 schema ever has to be restated.
#[tokio::test]
#[ignore = "regenerates a committed binary fixture; run explicitly"]
async fn generate_v1_fixture() {
    let path = v1_fixture();
    std::fs::create_dir_all(fixtures_dir()).expect("fixtures dir");
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(PathBuf::from(format!("{}{suffix}", path.display())));
    }

    // v1 opened sqlite with nothing but `create_if_missing` + `foreign_keys`.
    let options = SqliteConnectOptions::from_str(&sqlite_url(&path))
        .expect("url")
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Delete);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("v1 pool");

    // The v1 DDL, character for character as `db::migrate` used to emit it.
    for ddl in [
        "CREATE TABLE IF NOT EXISTS routine_templates (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            title       TEXT    NOT NULL UNIQUE,
            description TEXT    NOT NULL,
            icon_name   TEXT    NOT NULL,
            sort_order  INTEGER NOT NULL
        );",
        "CREATE TABLE IF NOT EXISTS daily_routine_logs (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id      INTEGER NOT NULL CHECK (user_id BETWEEN 1 AND 4),
            template_id  INTEGER NOT NULL REFERENCES routine_templates(id) ON DELETE CASCADE,
            date_logged  DATE    NOT NULL,
            completed_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
            UNIQUE (user_id, template_id, date_logged)
        );",
        "CREATE TABLE IF NOT EXISTS custom_tasks (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id      INTEGER NOT NULL CHECK (user_id BETWEEN 1 AND 4),
            title        TEXT    NOT NULL,
            photo_path   TEXT,
            is_completed BOOLEAN NOT NULL DEFAULT 0,
            created_at   TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
        );",
    ] {
        sqlx::query(ddl).execute(&pool).await.expect("v1 ddl");
    }

    for (index, (title, description, icon)) in
        family_calendar::server::db::SHEFFIELD_MORNING_ROUTINE
            .iter()
            .enumerate()
    {
        sqlx::query(
            "INSERT INTO routine_templates (title, description, icon_name, sort_order) \
             VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(title)
        .bind(description)
        .bind(icon)
        .bind(index as i64 + 1)
        .execute(&pool)
        .await
        .expect("seed");
    }

    // Three weeks of real-looking history for the four boys.
    let mut rows = 0;
    for day in 1..=21i64 {
        let date = format!("2026-08-{day:02}");
        for user_id in 1..=4i64 {
            for template_id in 1..=(1 + (day + user_id) % 8) {
                sqlx::query(
                    "INSERT INTO daily_routine_logs (user_id, template_id, date_logged) \
                     VALUES (?1, ?2, ?3)",
                )
                .bind(user_id)
                .bind(template_id)
                .bind(&date)
                .execute(&pool)
                .await
                .expect("log");
                rows += 1;
            }
        }
    }

    for (user_id, title) in [
        (1i64, "Take out the recycling"),
        (2, "Practise piano"),
        (3, "Tidy the Lego"),
        (4, "Feed the dog"),
    ] {
        sqlx::query("INSERT INTO custom_tasks (user_id, title, is_completed) VALUES (?1, ?2, 0)")
            .bind(user_id)
            .bind(title)
            .execute(&pool)
            .await
            .expect("task");
    }

    pool.close().await;
    assert!(path.is_file());
    assert!(rows > 100, "generated {rows} log rows");
    println!("wrote {} ({rows} routine log rows)", path.display());
}
