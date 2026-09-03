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

use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeZone};
use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Row, SqlitePool};
use tokio::sync::OnceCell;

use crate::server::config::FamilyHubConfig;
use crate::shared::types::{CustomTaskView, RoutineItemView, Stroke, StrokePoint};

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
            // HS1 handoff HS-1 (Boss post-A micro-commit, PLAN_HOMESCHOOL §3):
            // scan `curricula_dir()` for `*.toml`, insert only the rows that
            // are missing (never update, never delete), then seed the boys'
            // enrollment with `INSERT … ON CONFLICT (profile_id) DO NOTHING`
            // and the server-local run date. A bad curriculum file is
            // warned about and skipped; only a broken database can fail here.
            // Belongs to `pools()`, not `migrate()`, so tests that open their
            // own pools are untouched.
            crate::server::homeschool::load_and_seed(&pools.write, &FamilyHubConfig::load())
                .await?;
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
///
/// **QA round 1 (Q1-08):** generic over `impl sqlx::SqliteExecutor<'_>`
/// rather than pinned to `&SqlitePool`, so `api::routine`'s two toggles can
/// run this on the *same* `Transaction` as [`claim_mutation`] — one commit,
/// one rollback, atomically. Every existing caller still passes `&SqlitePool`
/// or `&pool`, both of which satisfy the bound unchanged.
pub async fn set_routine_completion(
    executor: impl sqlx::SqliteExecutor<'_>,
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
        .execute(executor)
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
        .execute(executor)
        .await?;
    }

    Ok(())
}

/// Insert a custom task. `photo_path` is a **web path that has already been
/// stored on disk** (`/uploads/<file>`) — never raw bytes to decode and write
/// here. A thin wrapper over [`insert_custom_task_with_due_date`] with no due
/// date, kept as its own function because most call sites (routine seeding,
/// the offline-queue replay) never care about a due date at all.
///
/// **Q1-06**: this used to take a base64 payload and decode-and-write it to
/// disk itself (`write_photo`), with no sniff, allowlist or re-encode — a
/// direct bypass of R-23c's server-side re-encode. The only real caller of
/// that path (the v1 `create_photo_task` server fn this replaced) is gone;
/// the multipart route (`api::photos::upload_photo_handler`) already sniffs,
/// downscales and re-encodes *before* calling this, via
/// [`insert_custom_task_with_due_date`] directly.
pub async fn insert_custom_task(
    pool: &SqlitePool,
    user_id: u32,
    title: &str,
    photo_path: Option<&str>,
) -> Result<u32, sqlx::Error> {
    insert_custom_task_with_due_date(pool, user_id, title, photo_path, None).await
}

/// Custom tasks belonging to `user_id`, newest first — **minus** any whose
/// `due_date` has already passed (T2.5's "daily auto-hide", PLAN v2 T2.5 /
/// PURPLE §P3 T2.5(f)): "a task with `due_date = yesterday` is absent from
/// today's list". A task with no `due_date` at all never expires, matching
/// the v1 behaviour (G12) for anything created without one.
///
/// `today` is computed here, server-side, from `chrono::Local::now()` —
/// never the caller's clock (PURPLE §P5.5 default 14: server-local time
/// everywhere) — so the filter cannot be fooled by a device with a wrong
/// clock the way a client-side filter could.
pub async fn custom_tasks(
    pool: &SqlitePool,
    user_id: u32,
) -> Result<Vec<CustomTaskView>, sqlx::Error> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let rows = sqlx::query(
        r#"
        SELECT id, user_id, title, photo_path, due_date, is_completed, created_at
        FROM custom_tasks
        WHERE user_id = ?1 AND (due_date IS NULL OR due_date >= ?2)
        ORDER BY created_at DESC, id DESC
        "#,
    )
    .bind(user_id)
    .bind(&today)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|row| {
            Ok(CustomTaskView {
                id: row.try_get::<i64, _>("id")? as u32,
                user_id: row.try_get::<i64, _>("user_id")? as u32,
                title: row.try_get("title")?,
                photo_path: row.try_get("photo_path")?,
                due_date: row.try_get("due_date")?,
                is_completed: row.try_get::<i64, _>("is_completed")? != 0,
                created_at: row.try_get("created_at")?,
            })
        })
        .collect()
}

/// Insert a custom task whose photo (if any) has **already been re-encoded
/// and written to disk** at `photo_path` — the shared insert body both
/// [`insert_custom_task`] (no due date) and the multipart upload route
/// (`api::photos::upload_photo_handler`, which sniffs, downscales and
/// re-encodes the image itself before calling this) go through. Neither
/// caller ever hands this function raw bytes to decode.
pub async fn insert_custom_task_with_due_date(
    pool: &SqlitePool,
    user_id: u32,
    title: &str,
    photo_path: Option<&str>,
    due_date: Option<&str>,
) -> Result<u32, sqlx::Error> {
    let id = sqlx::query(
        r#"
        INSERT INTO custom_tasks (user_id, title, photo_path, due_date, is_completed, created_at)
        VALUES (?1, ?2, ?3, ?4, 0, CURRENT_TIMESTAMP)
        RETURNING id
        "#,
    )
    .bind(user_id)
    .bind(title)
    .bind(photo_path)
    .bind(due_date)
    .fetch_one(pool)
    .await?
    .try_get::<i64, _>("id")?;

    Ok(id as u32)
}

/// **QA round 1 (Q1-08):** generic over `impl sqlx::SqliteExecutor<'_>` for
/// the same reason as [`set_routine_completion`] — `api::routine`'s
/// `toggle_custom_task` runs this inside the same transaction as its
/// `claim_mutation` call.
pub async fn set_custom_task_completion(
    executor: impl sqlx::SqliteExecutor<'_>,
    id: u32,
    completed: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE custom_tasks SET is_completed = ?2 WHERE id = ?1")
        .bind(id)
        .bind(completed as i64)
        .execute(executor)
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
///
/// **QA round 1 (Q1-08):** generic over `impl sqlx::SqliteExecutor<'_>`
/// rather than `&SqlitePool`. A claim used to commit in its own statement
/// *before* the write it guards — if the write then failed (a bad
/// `user_id`, a constraint violation), the key stayed claimed forever and
/// every retry or replay of that exact request quietly did nothing while
/// still reporting success. Callers now run the claim and the write inside
/// one `pool.begin()` transaction (`api::routine`'s two toggles) so a failed
/// write rolls the claim back with it, and the next delivery of the same key
/// gets a fresh chance to apply.
pub async fn claim_mutation(
    executor: impl sqlx::SqliteExecutor<'_>,
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
    .execute(executor)
    .await?;

    Ok(result.rows_affected() == 1)
}

/// Delete one custom task row, returning its `photo_path` when the task
/// existed (`Some(None)` for a task with no photo, `None` when there was no
/// such task at all) — T1.6's `backup::delete_custom_task` is what actually
/// removes the file from disk; this just does the row half atomically with
/// the read of the path it needs to do that (R-18).
pub async fn delete_custom_task_row(
    pool: &SqlitePool,
    id: u32,
) -> Result<Option<Option<String>>, sqlx::Error> {
    let row = sqlx::query("DELETE FROM custom_tasks WHERE id = ?1 RETURNING photo_path")
        .bind(id)
        .fetch_optional(pool)
        .await?;

    match row {
        Some(row) => Ok(Some(row.try_get::<Option<String>, _>("photo_path")?)),
        None => Ok(None),
    }
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

/// Insert a stroke at an **already-minted** `seq`, with no `SELECT MAX` — a
/// single `INSERT`, run against whatever executor the caller hands in.
///
/// T2.3's write-behind design (`server::api::realtime`'s module doc comment)
/// mints `seq` from an in-process counter so publishing a `Draw` never waits
/// on the write connection, then hands the row to the single ordered
/// persistence task, which inserts a whole drained batch inside one
/// transaction (Q1-09) — this is the per-row write half of that split, taking
/// `impl SqliteExecutor` (rather than `&SqlitePool`) precisely so the caller
/// can pass `&mut *tx` and keep every insert in a burst inside the same
/// transaction. `UNIQUE (board_id, seq)` still catches a real bug (two
/// callers minting the same number) exactly as it would have for
/// [`insert_stroke`]'s derived one.
pub async fn insert_stroke_at_seq(
    executor: impl sqlx::sqlite::SqliteExecutor<'_>,
    board_id: i64,
    seq: i64,
    client_id: &str,
    color: &str,
    width: f64,
    points: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO whiteboard_strokes (board_id, seq, client_id, color, width, points)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
    )
    .bind(board_id)
    .bind(seq)
    .bind(client_id)
    .bind(color)
    .bind(width)
    .bind(points)
    .execute(executor)
    .await?;
    Ok(())
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

/// The highest `seq` ever allocated on `board_id` (live or cleared), or `0`
/// when the board has never been drawn on. This is the watermark
/// `Snapshot`/`BoardCleared` carry — a plain `MAX(seq)` rather than a second
/// counter column, since [`insert_stroke`] already derives the next `seq` the
/// same way (T2.3, per H-10: "add another query shape to `db.rs` in your own
/// branch rather than duplicating SQL elsewhere").
pub async fn board_max_seq(pool: &SqlitePool, board_id: i64) -> Result<i64, sqlx::Error> {
    let (seq,): (i64,) =
        sqlx::query_as("SELECT COALESCE(MAX(seq), 0) FROM whiteboard_strokes WHERE board_id = ?1")
            .bind(board_id)
            .fetch_one(pool)
            .await?;
    Ok(seq)
}

/// Encode a stroke's points as the JSON array [`insert_stroke`] stores.
pub fn stroke_points_json(stroke: &Stroke) -> String {
    let raw: Vec<[f64; 2]> = stroke.points.iter().map(|p| [p.x, p.y]).collect();
    serde_json::to_string(&raw).unwrap_or_else(|_| "[]".to_string())
}

impl StoredStroke {
    /// Reconstruct the wire [`Stroke`] this row represents, from its JSON
    /// `points` column.
    pub fn into_stroke(self) -> Result<Stroke, serde_json::Error> {
        let raw: Vec<[f64; 2]> = serde_json::from_str(&self.points)?;
        Ok(Stroke {
            points: raw.into_iter().map(|[x, y]| StrokePoint { x, y }).collect(),
            color: self.color,
            width: self.width,
        })
    }
}

/// Hard-delete every cleared stroke on `board_id`, then trim the live strokes
/// down to the newest `keep_last` — the retention sweep §5/D4 describe
/// ("keep the last 2,000") and `docs/HANDOFF.md` H-10 reserves for T1.6's
/// midnight-tick hook. T2.3 lands the query shape now because its own
/// acceptance test ("the rows are gone after compaction") needs it to exist;
/// T1.6 registers the call with `realtime::on_day_rolled` rather than
/// duplicating the SQL (H-10). Returns the number of rows removed.
pub async fn compact_board(
    pool: &SqlitePool,
    board_id: i64,
    keep_last: i64,
) -> Result<u64, sqlx::Error> {
    let mut tx = pool.begin().await?;

    let cleared = sqlx::query(
        "DELETE FROM whiteboard_strokes WHERE board_id = ?1 AND cleared_at IS NOT NULL",
    )
    .bind(board_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    let trimmed = sqlx::query(
        r#"
        DELETE FROM whiteboard_strokes
        WHERE board_id = ?1 AND cleared_at IS NULL AND id NOT IN (
            SELECT id FROM whiteboard_strokes
            WHERE board_id = ?1 AND cleared_at IS NULL
            ORDER BY seq DESC
            LIMIT ?2
        )
        "#,
    )
    .bind(board_id)
    .bind(keep_last)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    tx.commit().await?;
    Ok(cleared + trimmed)
}

/// Test-only: forget every stroke on `board_id`, live or cleared, so a fresh
/// process-wide board can be asserted against a clean slate (mirrors the v1
/// in-memory `reset_board`'s role in `tests/realtime_tests.rs`). `seq` is
/// derived from `MAX(seq)`, so deleting every row is enough to restart it.
pub async fn hard_reset_board(pool: &SqlitePool, board_id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM whiteboard_strokes WHERE board_id = ?1")
        .bind(board_id)
        .execute(pool)
        .await?;
    Ok(())
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
