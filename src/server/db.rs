//! SQLite storage: pools, pragmas, embedded migrations and the data access
//! the rest of the server is built on (PLAN v2 D4, task T1.1).
//!
//! Three things here are load bearing and easy to regress:
//!
//! * **Migrations are embedded and numbered** (`sqlx::migrate!`, `migrations/`
//!   owned solely by T1.1). An existing v1 `family.db` — one written before
//!   `_sqlx_migrations` existed — is *baselined*: `0001_init` is recorded as
//!   already applied rather than executed, so `0002_core` and everything after
//!   it apply on top of the owner's real data instead of failing.
//! * **Two pools, not one** (G24). Readers get five connections, the writer
//!   exactly one, so SQLite's single-writer rule is enforced in the pool
//!   rather than discovered as `SQLITE_BUSY` at runtime. WAL +
//!   `synchronous=NORMAL` + a 30 s `busy_timeout` mean readers never block on
//!   the writer and a contended write waits instead of failing.
//! * **`wal_checkpoint(TRUNCATE)`** runs on the midnight tick
//!   ([`on_midnight_tick`]) so the `-wal` sidecar cannot grow without bound on
//!   a box that is never rebooted.

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

use base64::Engine;
use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeZone};
use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Row, SqlitePool};
use tokio::sync::OnceCell;

use crate::server::config::FamilyHubConfig;
use crate::shared::types::{CustomTaskView, RoutineItemView};

/// The numbered migrations in `migrations/`, embedded into the binary.
///
/// `build.rs` carries `cargo:rerun-if-changed=migrations`, so adding or
/// editing a file here forces a rebuild.
pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

/// Maximum simultaneous readers. Five is deliberate: the TV, two phones and
/// headroom for the pollers.
pub const READ_POOL_MAX_CONNECTIONS: u32 = 5;
/// SQLite allows exactly one writer; the pool enforces it rather than letting
/// concurrent writers race for the lock.
pub const WRITE_POOL_MAX_CONNECTIONS: u32 = 1;
/// `PRAGMA busy_timeout`, in milliseconds — a contended write waits, it does
/// not fail with `SQLITE_BUSY`.
pub const BUSY_TIMEOUT_MS: u32 = 30_000;
/// The single whiteboard (PURPLE §P5.5 default 15: named boards are cut).
pub const DEFAULT_BOARD_ID: i64 = 1;

const BUSY_TIMEOUT: Duration = Duration::from_millis(BUSY_TIMEOUT_MS as u64);
const BASELINE_VERSION: i64 = 1;

/// The v1 tables whose presence (without `_sqlx_migrations`) identifies a
/// database written by the pre-migration build.
const LEGACY_TABLES: [&str; 3] = ["routine_templates", "daily_routine_logs", "custom_tasks"];

/// `_sqlx_migrations`, byte for byte as `sqlx-sqlite` 0.8.6 creates it.
/// Baselining has to write the row before the migrator has ever run, so the
/// table has to exist first; `IF NOT EXISTS` keeps this compatible with the
/// migrator creating it itself on every subsequent boot.
const CREATE_MIGRATIONS_TABLE: &str = "
CREATE TABLE IF NOT EXISTS _sqlx_migrations (
    version BIGINT PRIMARY KEY,
    description TEXT NOT NULL,
    installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    success BOOLEAN NOT NULL,
    checksum BLOB NOT NULL,
    execution_time BIGINT NOT NULL
);
";

/// A read pool and a write pool over the same database file.
#[derive(Clone, Debug)]
pub struct Pools {
    /// Up to [`READ_POOL_MAX_CONNECTIONS`] concurrent readers.
    pub read: SqlitePool,
    /// Exactly [`WRITE_POOL_MAX_CONNECTIONS`] writer.
    pub write: SqlitePool,
}

impl Pools {
    /// Close both pools, flushing the WAL back into the main database.
    pub async fn close(&self) {
        self.write.close().await;
        self.read.close().await;
    }
}

static POOLS: OnceCell<Pools> = OnceCell::const_new();

/// Absolute directory photo tasks are written to (T0.5: resolved from
/// `FamilyHubConfig`, never a path relative to the process's CWD).
pub fn upload_dir() -> PathBuf {
    FamilyHubConfig::load().upload_dir()
}

/// The process-wide read and write pools, created (and migrated) on first use.
///
/// `DATABASE_URL`, when set, wins outright (integration tests use this to
/// point every test process at its own throwaway sqlite file). Otherwise the
/// URL is derived from [`FamilyHubConfig`], which resolves an **absolute**
/// path under `FAMILY_HUB_DATA_DIR` (default `%ProgramData%\FamilyHub`) —
/// never a bare `family.db` relative to the current working directory
/// (G23/R-14: under a Windows service the CWD is `C:\Windows\System32`).
pub async fn pools() -> Result<&'static Pools, sqlx::Error> {
    POOLS
        .get_or_try_init(|| async {
            let url = resolve_database_url().map_err(sqlx::Error::Io)?;
            let pools = open_pools(&url).await?;
            migrate(&pools.write).await?;
            Ok(pools)
        })
        .await
}

/// The write pool. Every mutating query goes through this one connection.
pub async fn pool() -> Result<&'static SqlitePool, sqlx::Error> {
    Ok(&pools().await?.write)
}

/// The read pool. Queries that only `SELECT` should use this so they never
/// queue behind a write.
pub async fn read_pool() -> Result<&'static SqlitePool, sqlx::Error> {
    Ok(&pools().await?.read)
}

fn resolve_database_url() -> std::io::Result<String> {
    if let Ok(url) = std::env::var("DATABASE_URL") {
        return Ok(url);
    }

    let config = FamilyHubConfig::load();
    config.ensure_dirs_and_log()?;
    Ok(config.database_url())
}

/// True for the in-memory URL forms sqlite understands. An in-memory database
/// is private to its connection, so the two-pool split is meaningless there
/// (and actively harmful — the pools would see different databases).
fn is_memory_url(url: &str) -> bool {
    url.contains(":memory:") || url.contains("mode=memory")
}

/// Connection options carrying every pragma D4 requires: WAL journalling,
/// `synchronous=NORMAL`, a 30 s busy timeout and enforced foreign keys.
fn connect_options(url: &str) -> Result<SqliteConnectOptions, sqlx::Error> {
    let options = SqliteConnectOptions::from_str(url)?
        .create_if_missing(true)
        .foreign_keys(true)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(BUSY_TIMEOUT);

    // WAL is a property of the database file; an in-memory database has no
    // file and reports `memory` back, so ask for what it can actually give.
    Ok(if is_memory_url(url) {
        options.journal_mode(SqliteJournalMode::Memory)
    } else {
        options.journal_mode(SqliteJournalMode::Wal)
    })
}

/// Open a single pool against `url`, creating the SQLite file when missing.
///
/// Prefer [`open_pools`]; this exists for tests and one-off tooling that only
/// needs one connection (and for `sqlite::memory:`, where a second pool would
/// open a second, empty database).
pub async fn connect(url: &str) -> Result<SqlitePool, sqlx::Error> {
    let options = connect_options(url)?;

    if is_memory_url(url) {
        // Keep the one connection alive for the life of the pool: dropping it
        // would drop the database with it.
        return SqlitePoolOptions::new()
            .min_connections(1)
            .max_connections(1)
            .idle_timeout(None)
            .max_lifetime(None)
            .connect_with(options)
            .await;
    }

    SqlitePoolOptions::new()
        .max_connections(READ_POOL_MAX_CONNECTIONS)
        .connect_with(options)
        .await
}

/// Open the read (max 5) and write (max 1) pools over the same database file.
///
/// For an in-memory URL both handles are clones of one single-connection pool,
/// since a second pool would be a second, empty database.
pub async fn open_pools(url: &str) -> Result<Pools, sqlx::Error> {
    if is_memory_url(url) {
        let pool = connect(url).await?;
        return Ok(Pools {
            read: pool.clone(),
            write: pool,
        });
    }

    let write = SqlitePoolOptions::new()
        .max_connections(WRITE_POOL_MAX_CONNECTIONS)
        .connect_with(connect_options(url)?)
        .await?;

    let read = SqlitePoolOptions::new()
        .max_connections(READ_POOL_MAX_CONNECTIONS)
        .connect_with(connect_options(url)?)
        .await?;

    Ok(Pools { read, write })
}

/// Bring the database up to the newest embedded migration and seed the
/// Sheffield morning routine.
///
/// A v1 database is baselined first (see [`baseline_legacy_database`]), so
/// this is safe to call against a fresh file, an already-migrated file, or the
/// owner's existing `family.db`.
pub async fn migrate(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    baseline_legacy_database(pool).await?;

    MIGRATOR
        .run(pool)
        .await
        .map_err(|err| sqlx::Error::Migrate(Box::new(err)))?;

    seed_routine_templates(pool).await
}

/// Record `0001_init` as already applied when the database carries the v1
/// schema but has never been touched by `sqlx::migrate!`.
///
/// Without this the migrator would run `0001_init` against a populated
/// database. Its statements are all `IF NOT EXISTS` so that would not corrupt
/// anything, but the row would then claim a migration ran that did not, and
/// any future non-idempotent `0001` edit would be a live data hazard. Being
/// explicit is the point: the owner's `family.db` starts at version 1 with the
/// checksum of the file on disk, and `0002_core` onwards apply on top of it.
async fn baseline_legacy_database(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    if table_exists(pool, "_sqlx_migrations").await? {
        return Ok(());
    }

    let mut legacy = false;
    for table in LEGACY_TABLES {
        if table_exists(pool, table).await? {
            legacy = true;
            break;
        }
    }
    if !legacy {
        return Ok(());
    }

    let baseline = MIGRATOR
        .iter()
        .find(|migration| migration.version == BASELINE_VERSION)
        .ok_or_else(|| {
            sqlx::Error::Configuration(
                "migrations/0001_init.sql is missing from the embedded migrator".into(),
            )
        })?;

    sqlx::query(CREATE_MIGRATIONS_TABLE).execute(pool).await?;
    sqlx::query(
        r#"
        INSERT OR IGNORE INTO _sqlx_migrations
            (version, description, success, checksum, execution_time)
        VALUES (?1, ?2, ?3, ?4, 0)
        "#,
    )
    .bind(baseline.version)
    .bind(baseline.description.as_ref())
    .bind(true)
    .bind(baseline.checksum.as_ref())
    .execute(pool)
    .await?;

    tracing::info!(
        version = baseline.version,
        description = %baseline.description,
        "baselined an existing pre-migration family.db"
    );

    Ok(())
}

async fn table_exists(pool: &SqlitePool, name: &str) -> Result<bool, sqlx::Error> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?1")
            .bind(name)
            .fetch_optional(pool)
            .await?;
    Ok(row.is_some())
}

/// The migration version this database is currently at, or `None` when the
/// migrator has never run (T1.7's `/health` reports this).
pub async fn migration_version(pool: &SqlitePool) -> Result<Option<i64>, sqlx::Error> {
    if !table_exists(pool, "_sqlx_migrations").await? {
        return Ok(None);
    }

    let row: (Option<i64>,) =
        sqlx::query_as("SELECT MAX(version) FROM _sqlx_migrations WHERE success <> 0")
            .fetch_one(pool)
            .await?;
    Ok(row.0)
}

/// Fold the write-ahead log back into the main database and truncate it to
/// zero bytes.
///
/// Called from [`on_midnight_tick`]. On a display that runs for months this is
/// the difference between a `family.db-wal` that stays small and one that only
/// ever grows.
pub async fn checkpoint_truncate(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE);")
        .fetch_all(pool)
        .await?;
    Ok(())
}

/// Storage housekeeping for the midnight tick (T1.2 owns the tick itself and
/// calls this once per rollover).
pub async fn on_midnight_tick() -> Result<(), sqlx::Error> {
    let pools = pools().await?;
    checkpoint_truncate(&pools.write).await
}

/// The eight items of the Sheffield morning routine.
pub const SHEFFIELD_MORNING_ROUTINE: [(&str, &str, &str); 8] = [
    (
        "Wake up and thank God for the day!",
        "Lamentations 3:23",
        "sun",
    ),
    (
        "Make your bed",
        "Responsibility for God's provision",
        "bed",
    ),
    (
        "Go to the bathroom - pee and poop. Brush your teeth.",
        "Release your bowel movements and take care of your mouth.",
        "sparkles",
    ),
    ("Drink 8 ounces of water.", "Quench your thirst.", "droplet"),
    (
        "Eat protein for breakfast.",
        "Regulate your blood sugar.",
        "utensils",
    ),
    (
        "Move your body for at least 30 minutes.",
        "Take care of your temple.",
        "activity",
    ),
    (
        "Read your Bible and ask God who you can bless today? And ask Him to reveal Himself in your day.",
        "Invite God into your day.",
        "book-open",
    ),
    ("Start your school work.", "Homeschool", "graduation-cap"),
];

async fn seed_routine_templates(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    for (index, (title, description, icon)) in SHEFFIELD_MORNING_ROUTINE.iter().enumerate() {
        sqlx::query(
            r#"
            INSERT INTO routine_templates (title, description, icon_name, sort_order)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT (title) DO UPDATE SET
                description = excluded.description,
                icon_name   = excluded.icon_name,
                sort_order  = excluded.sort_order
            "#,
        )
        .bind(title)
        .bind(description)
        .bind(icon)
        .bind(index as i64 + 1)
        .execute(pool)
        .await?;
    }

    Ok(())
}

/// Every routine template joined with `user_id`'s completion state on `date`.
pub async fn daily_routine(
    pool: &SqlitePool,
    user_id: u32,
    date: &str,
) -> Result<Vec<RoutineItemView>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT t.id, t.title, t.description, t.icon_name, t.sort_order,
               l.id IS NOT NULL AS completed
        FROM routine_templates t
        LEFT JOIN daily_routine_logs l
            ON l.template_id = t.id AND l.user_id = ?1 AND l.date_logged = ?2
        ORDER BY t.sort_order
        "#,
    )
    .bind(user_id)
    .bind(date)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|row| {
            Ok(RoutineItemView {
                template_id: row.try_get::<i64, _>("id")? as u32,
                title: row.try_get("title")?,
                description: row.try_get("description")?,
                icon_name: row.try_get("icon_name")?,
                sort_order: row.try_get("sort_order")?,
                completed: row.try_get::<i64, _>("completed")? != 0,
            })
        })
        .collect()
}

/// Record or clear a routine completion for `date`.
pub async fn set_routine_completion(
    pool: &SqlitePool,
    user_id: u32,
    template_id: u32,
    completed: bool,
    date: &str,
) -> Result<(), sqlx::Error> {
    if completed {
        sqlx::query(
            r#"
            INSERT INTO daily_routine_logs (user_id, template_id, date_logged, completed_at)
            VALUES (?1, ?2, ?3, CURRENT_TIMESTAMP)
            ON CONFLICT (user_id, template_id, date_logged)
            DO UPDATE SET completed_at = CURRENT_TIMESTAMP
            "#,
        )
        .bind(user_id)
        .bind(template_id)
        .bind(date)
        .execute(pool)
        .await?;
    } else {
        sqlx::query(
            r#"
            DELETE FROM daily_routine_logs
            WHERE user_id = ?1 AND template_id = ?2 AND date_logged = ?3
            "#,
        )
        .bind(user_id)
        .bind(template_id)
        .bind(date)
        .execute(pool)
        .await?;
    }

    Ok(())
}

/// Insert a custom task, persisting `photo_base64` to disk when supplied.
pub async fn insert_custom_task(
    pool: &SqlitePool,
    user_id: u32,
    title: &str,
    photo_base64: Option<&str>,
    upload_dir: impl AsRef<Path>,
) -> Result<u32, sqlx::Error> {
    let photo_path = match photo_base64 {
        Some(data) => Some(write_photo(data, upload_dir, user_id).await?),
        None => None,
    };

    let id = sqlx::query(
        r#"
        INSERT INTO custom_tasks (user_id, title, photo_path, is_completed, created_at)
        VALUES (?1, ?2, ?3, 0, CURRENT_TIMESTAMP)
        RETURNING id
        "#,
    )
    .bind(user_id)
    .bind(title)
    .bind(photo_path)
    .fetch_one(pool)
    .await?
    .try_get::<i64, _>("id")?;

    Ok(id as u32)
}

pub async fn custom_tasks(
    pool: &SqlitePool,
    user_id: u32,
) -> Result<Vec<CustomTaskView>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT id, user_id, title, photo_path, is_completed, created_at
        FROM custom_tasks
        WHERE user_id = ?1
        ORDER BY created_at DESC, id DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|row| {
            Ok(CustomTaskView {
                id: row.try_get::<i64, _>("id")? as u32,
                user_id: row.try_get::<i64, _>("user_id")? as u32,
                title: row.try_get("title")?,
                photo_path: row.try_get("photo_path")?,
                is_completed: row.try_get::<i64, _>("is_completed")? != 0,
                created_at: row.try_get("created_at")?,
            })
        })
        .collect()
}

pub async fn set_custom_task_completion(
    pool: &SqlitePool,
    id: u32,
    completed: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE custom_tasks SET is_completed = ?2 WHERE id = ?1")
        .bind(id)
        .bind(completed as i64)
        .execute(pool)
        .await?;
    Ok(())
}

/// The `user_id` that owns `task_id`, or `None` when the task does not exist.
///
/// **T1.5 (R-23/W1)**: `toggle_custom_task` calls this before touching a row
/// so one profile can never flip a sibling's task — the ownership check the
/// v1 endpoint never made.
pub async fn custom_task_owner(
    pool: &SqlitePool,
    task_id: u32,
) -> Result<Option<u32>, sqlx::Error> {
    let row: Option<(i64,)> = sqlx::query_as("SELECT user_id FROM custom_tasks WHERE id = ?1")
        .bind(task_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|(user_id,)| user_id as u32))
}

// ---------------------------------------------------------------------------
// Date correctness and idempotency (T1.5, R-24/R-15/W1)
// ---------------------------------------------------------------------------

/// How far a client-declared mutation date may drift from the server's own
/// notion of today (PLAN v2 T1.5, PURPLE §P3: "validated within ±1 day
/// server-side"). One day either way covers a legitimate late-night/early-
/// morning boundary crossing (a child finishing yesterday's routine at
/// 00:03, or a queued offline mutation replaying just after midnight)
/// without opening the door to backdating or postdating a log by any
/// meaningful amount.
pub const MUTATION_DATE_WINDOW_DAYS: i64 = 1;

/// `true` when `date` (a client-supplied `YYYY-MM-DD`) is within
/// [`MUTATION_DATE_WINDOW_DAYS`] of `today` (the server's own `YYYY-MM-DD`).
///
/// A date that fails to parse is never "close enough" — it is rejected the
/// same as one three days away. Pure and synchronous so every mutating
/// server fn can validate a date before it ever opens a database connection,
/// and so the boundary itself is unit-testable without a pool (R-24: no
/// mutation may trust a bare `unwrap_or_default()`; this is the explicit
/// check that replaces it).
pub fn date_within_window(date: &str, today: &str) -> bool {
    let (Ok(date), Ok(today)) = (
        NaiveDate::parse_from_str(date, "%Y-%m-%d"),
        NaiveDate::parse_from_str(today, "%Y-%m-%d"),
    ) else {
        return false;
    };
    (date - today).num_days().abs() <= MUTATION_DATE_WINDOW_DAYS
}

/// Atomically claim `idempotency_key` in the `mutation_log` ledger.
///
/// Returns `true` the **first** time a key is seen — the caller should apply
/// the mutation. Returns `false` on every replay of the same key — the
/// caller must skip the write, so a client's offline queue (or a flaky
/// network causing a retried request) produces exactly one effect no matter
/// how many times it is sent (PLAN v2 T1.5 / R-15).
///
/// This has to be a single statement, not a `SELECT` followed by an
/// `INSERT`: the write pool serialises callers onto one connection but each
/// `.execute()` still acquires and releases it, so two overlapping calls
/// could otherwise both observe "not yet claimed" between their own select
/// and insert. `INSERT OR IGNORE` makes the claim atomic — `rows_affected()`
/// is `1` only for whichever caller's row actually landed.
pub async fn claim_mutation(
    pool: &SqlitePool,
    idempotency_key: &str,
    kind: &str,
    user_id: u32,
    payload: &str,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        r#"
        INSERT OR IGNORE INTO mutation_log (idempotency_key, kind, user_id, payload)
        VALUES (?1, ?2, ?3, ?4)
        "#,
    )
    .bind(idempotency_key)
    .bind(kind)
    .bind(user_id)
    .bind(payload)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() == 1)
}

/// Decode a (possibly data-URI prefixed) base64 image and store it on disk,
/// returning the web path the client can load it from.
async fn write_photo(
    photo_base64: &str,
    upload_dir: impl AsRef<Path>,
    user_id: u32,
) -> Result<String, sqlx::Error> {
    let payload = photo_base64
        .split_once("base64,")
        .map(|(_, rest)| rest)
        .unwrap_or(photo_base64);

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload.trim())
        .map_err(|err| sqlx::Error::Protocol(format!("invalid photo payload: {err}")))?;

    let dir: PathBuf = upload_dir.as_ref().to_path_buf();
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(sqlx::Error::Io)?;

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    let file_name = format!("task-{user_id}-{stamp}.jpg");

    tokio::fs::write(dir.join(&file_name), bytes)
        .await
        .map_err(sqlx::Error::Io)?;

    Ok(format!("/uploads/{file_name}"))
}

// ---------------------------------------------------------------------------
// DST-safe local time
// ---------------------------------------------------------------------------

/// How far past a skipped local midnight to look for the first wall-clock time
/// that actually exists. Three hours covers every real-world spring-forward.
const MIDNIGHT_SEARCH_MINUTES: i64 = 180;

/// Resolve a wall-clock local time to a real instant, **always choosing the
/// earliest offset** when the clock is ambiguous.
///
/// This is the helper D5 requires. `chrono`'s `.single()` returns `None` for
/// the hour that repeats when DST ends — the v1 code fell back to treating
/// local time as UTC (G10), which silently shifts every date boundary by the
/// UTC offset. `.earliest()` instead picks the *first* of the two instants
/// that share the wall clock: the pre-transition offset. That is the choice a
/// midnight rollover wants — the day flips the first time the clock reads
/// 00:00, not an hour later.
///
/// Returns `None` only for a local time that does not exist at all (the hour
/// skipped when DST begins); use [`next_local_midnight`] for tick scheduling,
/// which handles that case.
pub fn resolve_local<Tz: TimeZone>(tz: &Tz, local: NaiveDateTime) -> Option<DateTime<Tz>> {
    tz.from_local_datetime(&local).earliest()
}

/// The next local midnight strictly after `after`.
///
/// Zones that skip midnight itself when DST begins (America/Santiago, for one)
/// have no 00:00 on that date at all, so this walks forward minute by minute
/// to the first wall-clock time that exists — the correct instant to roll the
/// day over — instead of returning `None` and stalling the tick forever.
pub fn next_local_midnight<Tz: TimeZone>(tz: &Tz, after: &DateTime<Tz>) -> Option<DateTime<Tz>> {
    let mut day = after.date_naive();

    // At most three attempts: today (in case `after` is before midnight is
    // even reachable), tomorrow, and the day after.
    for _ in 0..3 {
        day = day.succ_opt()?;
        let midnight = day.and_hms_opt(0, 0, 0)?;

        for minute in 0..MIDNIGHT_SEARCH_MINUTES {
            let candidate = midnight + chrono::Duration::minutes(minute);
            if let Some(resolved) = resolve_local(tz, candidate) {
                if resolved > *after {
                    return Some(resolved);
                }
            }
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Whiteboard strokes (schema owned here; drawing behaviour lives in T2.3)
// ---------------------------------------------------------------------------

/// One persisted stroke, as `Snapshot` replays it to a joining client.
#[derive(Clone, Debug, PartialEq)]
pub struct StoredStroke {
    pub id: i64,
    pub seq: i64,
    pub client_id: String,
    pub color: String,
    pub width: f64,
    /// JSON array of normalised `[x, y]` pairs, both in `0.0..=1.0`.
    pub points: String,
}

fn stroke_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<StoredStroke, sqlx::Error> {
    Ok(StoredStroke {
        id: row.try_get("id")?,
        seq: row.try_get("seq")?,
        client_id: row.try_get("client_id")?,
        color: row.try_get("color")?,
        width: row.try_get("width")?,
        points: row.try_get("points")?,
    })
}

/// Append a stroke to `board_id`, assigning the next `seq`.
///
/// Sequence allocation and insertion share one transaction so two writers can
/// never mint the same `seq` (the write pool's single connection already
/// serialises them; the transaction keeps that true if the pool is ever
/// widened).
///
/// `seq` orders the strokes currently on disk for a board; it is deliberately
/// **not** a lifetime-unique id. Undoing the newest stroke frees its number
/// for the next one, and once T1.6 compacts a cleared board the sequence
/// restarts — both are safe because the removal is broadcast before any stroke
/// can claim the number again.
pub async fn insert_stroke(
    pool: &SqlitePool,
    board_id: i64,
    client_id: &str,
    color: &str,
    width: f64,
    points: &str,
) -> Result<StoredStroke, sqlx::Error> {
    let mut tx = pool.begin().await?;

    let (next_seq,): (i64,) = sqlx::query_as(
        "SELECT COALESCE(MAX(seq), 0) + 1 FROM whiteboard_strokes WHERE board_id = ?1",
    )
    .bind(board_id)
    .fetch_one(&mut *tx)
    .await?;

    let row = sqlx::query(
        r#"
        INSERT INTO whiteboard_strokes (board_id, seq, client_id, color, width, points)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        RETURNING id, seq, client_id, color, width, points
        "#,
    )
    .bind(board_id)
    .bind(next_seq)
    .bind(client_id)
    .bind(color)
    .bind(width)
    .bind(points)
    .fetch_one(&mut *tx)
    .await?;

    let stroke = stroke_from_row(&row)?;
    tx.commit().await?;
    Ok(stroke)
}

/// Every live (not cleared) stroke on `board_id`, in `seq` order — the body of
/// the `Snapshot` message a joining client receives.
pub async fn board_snapshot(
    pool: &SqlitePool,
    board_id: i64,
) -> Result<Vec<StoredStroke>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT id, seq, client_id, color, width, points
        FROM whiteboard_strokes
        WHERE board_id = ?1 AND cleared_at IS NULL
        ORDER BY seq
        "#,
    )
    .bind(board_id)
    .fetch_all(pool)
    .await?;

    rows.iter().map(stroke_from_row).collect()
}

/// Move the `cleared_at` watermark: every live stroke is stamped, so the next
/// `Snapshot` is empty while the rows survive until T1.6 compacts them.
/// Returns how many strokes were cleared.
pub async fn clear_board(pool: &SqlitePool, board_id: i64) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        UPDATE whiteboard_strokes
        SET cleared_at = CURRENT_TIMESTAMP
        WHERE board_id = ?1 AND cleared_at IS NULL
        "#,
    )
    .bind(board_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

/// Remove the calling client's most recent live stroke, and only that client's
/// (undo must never reach across to another child's drawing). Returns the
/// removed `seq`, or `None` when that client has nothing left to undo.
pub async fn undo_last_stroke(
    pool: &SqlitePool,
    board_id: i64,
    client_id: &str,
) -> Result<Option<i64>, sqlx::Error> {
    let row: Option<(i64,)> = sqlx::query_as(
        r#"
        DELETE FROM whiteboard_strokes
        WHERE id = (
            SELECT id FROM whiteboard_strokes
            WHERE board_id = ?1 AND client_id = ?2 AND cleared_at IS NULL
            ORDER BY seq DESC
            LIMIT 1
        )
        RETURNING seq
        "#,
    )
    .bind(board_id)
    .bind(client_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|(seq,)| seq))
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

/// Read a server setting, or `None` when it has never been written.
pub async fn get_setting(pool: &SqlitePool, key: &str) -> Result<Option<String>, sqlx::Error> {
    let row: Option<(String,)> = sqlx::query_as("SELECT value FROM settings WHERE key = ?1")
        .bind(key)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|(value,)| value))
}

/// Write a server setting, replacing any previous value.
pub async fn set_setting(pool: &SqlitePool, key: &str, value: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO settings (key, value, updated_at)
        VALUES (?1, ?2, CURRENT_TIMESTAMP)
        ON CONFLICT (key) DO UPDATE SET
            value = excluded.value,
            updated_at = CURRENT_TIMESTAMP
        "#,
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}
