//! Backup, retention and delete-with-file paths (PLAN v2 R-17/R-18, task
//! T1.6). Everything here is either sole-owned by this task (this file) or a
//! narrow addition to `src/server/db.rs` (permitted by
//! `docs/reviews/PURPLE_TEAM.md` §P4: "T1.6 (retention fns)").
//!
//! Five independent jobs live here:
//!
//! 1. **Nightly backup** ([`run_nightly_backup`]): `VACUUM INTO
//!    <data>\backups\family-YYYYMMDD-HHMM.db` plus a snapshot copy of
//!    `<data>\uploads`, run against the **read** pool so it never contends
//!    with the sole write connection (D4's two-pool split already gives WAL
//!    readers a consistent snapshot even while a writer transaction is
//!    open — that is the whole point of `VACUUM INTO` over a plain file
//!    copy; see the acceptance test in `tests/backup_tests.rs` for the
//!    proof).
//! 2. **Backup retention** ([`enforce_backup_retention`]): keep the newest
//!    [`BACKUP_RETENTION_COUNT`] backups, deleting the rest (database file
//!    and its uploads-snapshot directory together).
//! 3. **Delete-with-file** ([`delete_custom_task`]): remove a custom task's
//!    row and, if it had one, its photo file — R-18's "nothing is ever
//!    deleted" complaint, closed for the one entity a parent can delete
//!    today.
//! 4. **Stroke compaction** ([`compact_strokes`], delegating to `db::compact_board`) and **photo
//!    retention** ([`purge_old_photos`]): bound the two things the app
//!    writes continuously without any user-initiated delete.
//! 5. **Log rotation** ([`rotate_log_if_needed`]): a sized ring of log files
//!    for whatever writes to `<data>\logs` (T3.1's service host).
//!
//! **What must never appear in a backup** (`docs/HANDOFF.md` H-13): `<data>
//! \pki\` holds `ca.key` and `leaf.key`. Backups here touch exactly two
//! things — the database (via `VACUUM INTO`, which reads *rows*, not files)
//! and `<data>\uploads` (via [`snapshot_uploads`], a directory-scoped copy)
//! — so `pki\` is never read, let alone copied. That is a structural
//! guarantee, not a filter applied after the fact.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Local};
use sqlx::SqlitePool;

use crate::server::config::FamilyHubConfig;

/// Nightly backups kept before the oldest is deleted (PLAN v2 T1.6 / PURPLE
/// §P3: "N-day retention (default 14 backups)").
pub const BACKUP_RETENTION_COUNT: usize = 14;
/// Live whiteboard strokes kept per board after compaction.
pub const STROKE_COMPACTION_KEEP: i64 = 2_000;
/// A photo-task upload older than this many days is purged.
pub const PHOTO_RETENTION_DAYS: i64 = 30;
/// Rotate a log file once it reaches this size ("10 MB × 5").
pub const LOG_ROTATION_MAX_BYTES: u64 = 10 * 1024 * 1024;
/// Rotated log generations retained (`<name>.1` .. `<name>.LOG_ROTATION_MAX_FILES`),
/// on top of the currently-active file.
pub const LOG_ROTATION_MAX_FILES: usize = 5;

const BACKUP_DB_PREFIX: &str = "family-";
const BACKUP_DB_SUFFIX: &str = ".db";
const BACKUP_UPLOADS_SUFFIX: &str = "_uploads";

/// Everything this module can fail with. Kept concrete (not boxed) so a
/// caller can tell a database error from a filesystem one.
#[derive(Debug)]
pub enum BackupError {
    Sqlx(sqlx::Error),
    Io(io::Error),
}

impl std::fmt::Display for BackupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sqlx(err) => write!(f, "backup database error: {err}"),
            Self::Io(err) => write!(f, "backup filesystem error: {err}"),
        }
    }
}

impl std::error::Error for BackupError {}

impl From<sqlx::Error> for BackupError {
    fn from(err: sqlx::Error) -> Self {
        Self::Sqlx(err)
    }
}

impl From<io::Error> for BackupError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

// ---------------------------------------------------------------------------
// Nightly backup: VACUUM INTO + uploads snapshot + retention
// ---------------------------------------------------------------------------

/// `<data>\backups`.
pub fn backups_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("backups")
}

/// `family-YYYYMMDD-HHMM.db` for `now`. Minute resolution is deliberate: a
/// nightly job runs at most once a minute, so the name is always unique and
/// always sorts chronologically as a plain string (`list_backup_db_files`
/// relies on this).
pub fn backup_file_name(now: DateTime<Local>) -> String {
    format!(
        "{BACKUP_DB_PREFIX}{}{BACKUP_DB_SUFFIX}",
        now.format("%Y%m%d-%H%M")
    )
}

/// The uploads-snapshot directory that goes with `db_file` (a path returned
/// by [`backup_file_name`] joined onto a `backups_dir`).
fn uploads_snapshot_dir_for(db_file: &Path) -> Option<PathBuf> {
    let stem = db_file.file_stem()?.to_str()?;
    Some(db_file.with_file_name(format!("{stem}{BACKUP_UPLOADS_SUFFIX}")))
}

/// Run `VACUUM INTO dest` against `pool`. `dest` must not already exist —
/// that is `VACUUM INTO`'s own rule, and it is what makes the minute-stamped
/// filename safe to rely on rather than silently overwriting a prior backup.
///
/// This is the one safe way to hot-copy a WAL-mode SQLite database (D4):
/// unlike a plain file copy, it reads through a transaction and always
/// produces a single, self-contained, consistent file — proved by the
/// side-by-side comparison in `tests/backup_tests.rs`'s acceptance (a).
pub async fn vacuum_into(pool: &SqlitePool, dest: &Path) -> Result<(), sqlx::Error> {
    if let Some(parent) = dest.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // `VACUUM INTO` takes its filename as plain text, not a URI, so the
    // native path separator is fine — unlike `sqlite://` connection URLs,
    // which do need forward slashes.
    let dest_str = dest.to_string_lossy().into_owned();
    sqlx::query("VACUUM INTO ?1")
        .bind(dest_str)
        .execute(pool)
        .await?;
    Ok(())
}

/// Copy every regular file directly inside `upload_dir` into `dest_dir`
/// (created if missing). Returns how many files were copied. Not recursive —
/// `uploads/` has never had subdirectories, and staying flat keeps this a
/// simple, auditable "snapshot the tree" rather than a general sync tool.
pub fn snapshot_uploads(upload_dir: &Path, dest_dir: &Path) -> io::Result<u64> {
    if !upload_dir.is_dir() {
        return Ok(0);
    }
    std::fs::create_dir_all(dest_dir)?;

    let mut copied = 0u64;
    for entry in std::fs::read_dir(upload_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            std::fs::copy(&path, dest_dir.join(entry.file_name()))?;
            copied += 1;
        }
    }
    Ok(copied)
}

/// Every `family-*.db` file directly inside `backups_dir`, oldest first.
/// Lexicographic order is chronological order here because
/// [`backup_file_name`] always produces `family-YYYYMMDD-HHMM.db`.
pub fn list_backup_db_files(backups_dir: &Path) -> io::Result<Vec<PathBuf>> {
    if !backups_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut files: Vec<PathBuf> = std::fs::read_dir(backups_dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| {
                        name.starts_with(BACKUP_DB_PREFIX) && name.ends_with(BACKUP_DB_SUFFIX)
                    })
                    .unwrap_or(false)
        })
        .collect();
    files.sort();
    Ok(files)
}

/// Delete every backup beyond the newest `keep`, along with each one's
/// uploads-snapshot directory. Returns the removed database file paths.
pub fn enforce_backup_retention(backups_dir: &Path, keep: usize) -> io::Result<Vec<PathBuf>> {
    let files = list_backup_db_files(backups_dir)?;
    let mut removed = Vec::new();

    if files.len() > keep {
        for file in &files[..files.len() - keep] {
            std::fs::remove_file(file)?;
            if let Some(uploads_dir) = uploads_snapshot_dir_for(file) {
                if uploads_dir.is_dir() {
                    std::fs::remove_dir_all(&uploads_dir)?;
                }
            }
            removed.push(file.clone());
        }
    }

    Ok(removed)
}

/// What one run of [`run_nightly_backup`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupSummary {
    pub db_file: PathBuf,
    pub uploads_files_copied: u64,
    pub backups_removed: Vec<PathBuf>,
}

/// The nightly job: `VACUUM INTO` a fresh timestamped backup, snapshot
/// `uploads/` alongside it, then enforce [`BACKUP_RETENTION_COUNT`].
///
/// `pool` should be the **read** pool — `VACUUM INTO` only needs a read
/// transaction, and running it there means the nightly job can never queue
/// behind (or block) the single write connection.
pub async fn run_nightly_backup(
    pool: &SqlitePool,
    data_dir: &Path,
) -> Result<BackupSummary, BackupError> {
    run_nightly_backup_at(pool, data_dir, Local::now()).await
}

/// [`run_nightly_backup`] with an injectable timestamp, so tests do not
/// depend on wall-clock minute boundaries.
pub async fn run_nightly_backup_at(
    pool: &SqlitePool,
    data_dir: &Path,
    now: DateTime<Local>,
) -> Result<BackupSummary, BackupError> {
    let backups = backups_dir(data_dir);
    std::fs::create_dir_all(&backups)?;

    let db_file = backups.join(backup_file_name(now));
    vacuum_into(pool, &db_file).await?;

    let uploads_files_copied = match uploads_snapshot_dir_for(&db_file) {
        Some(dest) => snapshot_uploads(&data_dir.join("uploads"), &dest)?,
        None => 0,
    };

    let backups_removed = enforce_backup_retention(&backups, BACKUP_RETENTION_COUNT)?;

    Ok(BackupSummary {
        db_file,
        uploads_files_copied,
        backups_removed,
    })
}

// ---------------------------------------------------------------------------
// Restore drill
// ---------------------------------------------------------------------------

/// `<db_path><suffix>` — the WAL/SHM sidecars SQLite keeps next to a
/// WAL-mode database.
fn sidecar_path(db_path: &Path, suffix: &str) -> PathBuf {
    let mut name = db_path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

/// Restore `backup_db` over `target_db`: copy the backup file into place and
/// remove any stale `-wal`/`-shm` sidecars so a leftover write-ahead log from
/// the database being replaced cannot resurrect data the backup does not
/// have.
pub fn restore_database(backup_db: &Path, target_db: &Path) -> io::Result<()> {
    if let Some(parent) = target_db.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(backup_db, target_db)?;
    for suffix in ["-wal", "-shm"] {
        let sidecar = sidecar_path(target_db, suffix);
        if sidecar.exists() {
            std::fs::remove_file(&sidecar)?;
        }
    }
    Ok(())
}

/// Restore an uploads snapshot over the live uploads directory. A thin,
/// intention-revealing wrapper over [`snapshot_uploads`] — the copy
/// direction is the only difference from taking one.
pub fn restore_uploads(backup_uploads_dir: &Path, target_uploads_dir: &Path) -> io::Result<u64> {
    snapshot_uploads(backup_uploads_dir, target_uploads_dir)
}

// ---------------------------------------------------------------------------
// Delete a custom task, and its photo file (R-18)
// ---------------------------------------------------------------------------

/// Delete `task_id` and, if it had one, its photo file on disk. Returns
/// whether a task actually existed to delete.
///
/// The row is removed first (via `db::delete_custom_task_row`, a single
/// `DELETE ... RETURNING`), and the file only after the row is confirmed
/// gone — so a crash between the two steps can only leave an orphaned file
/// (harmless, and photo retention will eventually reap it), never a
/// dangling database reference to a file that no longer exists.
pub async fn delete_custom_task(
    pool: &SqlitePool,
    task_id: u32,
    upload_dir: impl AsRef<Path>,
) -> Result<bool, sqlx::Error> {
    match crate::server::db::delete_custom_task_row(pool, task_id).await? {
        Some(photo_path) => {
            if let Some(photo_path) = photo_path {
                remove_uploaded_photo(upload_dir.as_ref(), &photo_path);
            }
            Ok(true)
        }
        None => Ok(false),
    }
}

/// `photo_path` is the web path stored on the row (`/uploads/<file>`); the
/// file itself lives at `upload_dir/<file>`. Missing is not an error — the
/// goal state (no file on disk) already holds.
fn remove_uploaded_photo(upload_dir: &Path, photo_path: &str) {
    let file_name = photo_path.rsplit('/').next().unwrap_or(photo_path);
    if file_name.is_empty() {
        return;
    }
    let path = upload_dir.join(file_name);
    if let Err(err) = std::fs::remove_file(&path) {
        if err.kind() != io::ErrorKind::NotFound {
            tracing::warn!(
                path = %path.display(),
                %err,
                "failed to remove a custom task's photo file during delete"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Stroke compaction — thin re-export; the query lives in db.rs. T2.3 landed
// the query shape as `db::compact_board` (its acceptance test needed it)
// before this branch merged, so T1.6 delegates to it rather than keeping a
// second copy of the same SQL (H-10: never duplicate SQL outside db.rs).
// ---------------------------------------------------------------------------

/// Compact `board_id`'s stroke history: hard-delete every already-cleared
/// stroke, then trim live strokes to the newest `keep`. See
/// `db::compact_board` for the two-pass rule.
pub async fn compact_strokes(
    pool: &SqlitePool,
    board_id: i64,
    keep: i64,
) -> Result<u64, sqlx::Error> {
    crate::server::db::compact_board(pool, board_id, keep).await
}

// ---------------------------------------------------------------------------
// Photo retention (30 days)
// ---------------------------------------------------------------------------

/// Delete every file directly inside `upload_dir` whose modified time is
/// older than `older_than_days`, and null out the `custom_tasks.photo_path`
/// of any row that pointed at it — a purged file must never be a dangling
/// reference.
///
/// Returns the number of files removed.
pub async fn purge_old_photos(
    pool: &SqlitePool,
    upload_dir: &Path,
    older_than_days: i64,
) -> Result<u64, BackupError> {
    purge_old_photos_before(pool, upload_dir, cutoff(older_than_days, SystemTime::now())).await
}

fn cutoff(older_than_days: i64, now: SystemTime) -> SystemTime {
    let age = Duration::from_secs(older_than_days.max(0) as u64 * 86_400);
    now.checked_sub(age).unwrap_or(SystemTime::UNIX_EPOCH)
}

/// [`purge_old_photos`] with an injectable cutoff instant, so a test can
/// purge a freshly-written fixture file without sleeping 30 days.
pub async fn purge_old_photos_before(
    pool: &SqlitePool,
    upload_dir: &Path,
    cutoff: SystemTime,
) -> Result<u64, BackupError> {
    if !upload_dir.is_dir() {
        return Ok(0);
    }

    let mut removed = 0u64;
    for entry in std::fs::read_dir(upload_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let modified = entry.metadata().and_then(|meta| meta.modified());
        let Ok(modified) = modified else { continue };
        if modified >= cutoff {
            continue;
        }

        let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let web_path = format!("/uploads/{file_name}");

        std::fs::remove_file(&path)?;
        removed += 1;

        sqlx::query("UPDATE custom_tasks SET photo_path = NULL WHERE photo_path = ?1")
            .bind(&web_path)
            .execute(pool)
            .await?;
    }

    Ok(removed)
}

// ---------------------------------------------------------------------------
// Log rotation (10 MB × 5)
// ---------------------------------------------------------------------------

/// `<name>.<n>` next to `log_path` — the same convention every rotating
/// logger uses (`app.log.1` is the most recent rotation).
fn rotated_log_path(log_path: &Path, generation: usize) -> PathBuf {
    let mut name = log_path.as_os_str().to_os_string();
    name.push(format!(".{generation}"));
    PathBuf::from(name)
}

/// If `log_path` is at least `max_bytes`, rotate it: drop the oldest
/// generation beyond `max_files`, shift every other generation up by one,
/// move the current file to `.1`, and leave a fresh empty file at `log_path`
/// for the caller's writer to keep appending to.
///
/// Returns whether a rotation happened. A missing `log_path` is not an error
/// — nothing has been written yet, so there is nothing to rotate.
pub fn rotate_log_if_needed(log_path: &Path, max_bytes: u64, max_files: usize) -> io::Result<bool> {
    let size = match std::fs::metadata(log_path) {
        Ok(meta) => meta.len(),
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(err),
    };
    if size < max_bytes {
        return Ok(false);
    }

    let max_files = max_files.max(1);

    let oldest = rotated_log_path(log_path, max_files);
    if oldest.exists() {
        std::fs::remove_file(&oldest)?;
    }
    for generation in (1..max_files).rev() {
        let src = rotated_log_path(log_path, generation);
        if src.exists() {
            std::fs::rename(&src, rotated_log_path(log_path, generation + 1))?;
        }
    }
    std::fs::rename(log_path, rotated_log_path(log_path, 1))?;
    std::fs::File::create(log_path)?;

    Ok(true)
}

// ---------------------------------------------------------------------------
// Wiring the nightly sweep onto the midnight tick
// ---------------------------------------------------------------------------

/// Register the nightly backup/retention sweep on `realtime`'s day-rolled
/// hook (`docs/HANDOFF.md`'s T1.1→T1.6 note: "register work on the midnight
/// tick with `realtime::on_day_rolled(hook)` rather than editing the loop").
///
/// This function only *registers* the hook; something on the startup path
/// has to call it once. `src/server/router.rs::run` is the natural place
/// (next to `realtime::ensure_background_tasks()`), but that file is
/// T0.6/T1.3-owned — see `docs/HANDOFF.md`'s T1.6 entry for the handoff.
pub fn register_nightly_hooks() {
    crate::server::api::realtime::on_day_rolled(std::sync::Arc::new(|_date: String| {
        Box::pin(async move {
            if let Err(err) = nightly_maintenance().await {
                tracing::warn!(%err, "nightly backup/retention sweep failed");
            }
        })
    }));
}

/// The full nightly sweep: backup, stroke compaction, photo retention, log
/// rotation — everything this module owns, run once per day-roll.
async fn nightly_maintenance() -> Result<(), BackupError> {
    let config = FamilyHubConfig::load();

    let read_pool = crate::server::db::read_pool().await?;
    let summary = run_nightly_backup(read_pool, &config.data_dir).await?;
    tracing::info!(
        db_file = %summary.db_file.display(),
        uploads_files_copied = summary.uploads_files_copied,
        backups_removed = summary.backups_removed.len(),
        "nightly backup complete"
    );

    let write_pool = crate::server::db::pool().await?;
    let compacted = compact_strokes(
        write_pool,
        crate::shared::types::DEFAULT_BOARD_ID,
        STROKE_COMPACTION_KEEP,
    )
    .await?;
    if compacted > 0 {
        tracing::info!(compacted, "whiteboard stroke compaction complete");
    }

    let purged = purge_old_photos(write_pool, &config.upload_dir(), PHOTO_RETENTION_DAYS).await?;
    if purged > 0 {
        tracing::info!(purged, "photo retention purge complete");
    }

    let log_path = config.log_dir().join("familyhub.log");
    if rotate_log_if_needed(&log_path, LOG_ROTATION_MAX_BYTES, LOG_ROTATION_MAX_FILES)? {
        tracing::info!(log_path = %log_path.display(), "log file rotated");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_file_name_has_minute_resolution_and_sorts_chronologically() {
        use chrono::TimeZone;
        let earlier = Local.with_ymd_and_hms(2026, 3, 1, 1, 2, 0).unwrap();
        let later = Local.with_ymd_and_hms(2026, 3, 1, 1, 3, 0).unwrap();
        let a = backup_file_name(earlier);
        let b = backup_file_name(later);
        assert_eq!(a, "family-20260301-0102.db");
        assert_eq!(b, "family-20260301-0103.db");
        assert!(a < b, "names must sort chronologically as plain strings");
    }

    #[test]
    fn uploads_snapshot_dir_matches_the_db_file_stem() {
        let db_file = Path::new("C:/data/backups/family-20260301-0102.db");
        let dir = uploads_snapshot_dir_for(db_file).expect("stem exists");
        assert_eq!(
            dir,
            Path::new("C:/data/backups/family-20260301-0102_uploads")
        );
    }

    #[test]
    fn cutoff_thirty_days_back_is_thirty_days_earlier() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(40 * 86_400);
        let c = cutoff(30, now);
        assert_eq!(c, SystemTime::UNIX_EPOCH + Duration::from_secs(10 * 86_400));
    }

    #[test]
    fn rotated_log_path_appends_a_dotted_generation() {
        let log = Path::new("C:/data/logs/familyhub.log");
        assert_eq!(
            rotated_log_path(log, 1),
            Path::new("C:/data/logs/familyhub.log.1")
        );
        assert_eq!(
            rotated_log_path(log, 5),
            Path::new("C:/data/logs/familyhub.log.5")
        );
    }

    #[test]
    fn rotate_log_if_needed_is_a_no_op_when_the_file_is_missing() {
        let dir = std::env::temp_dir().join(format!(
            "familyhub-backup-unit-rotate-missing-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let log = dir.join("familyhub.log");

        assert!(!rotate_log_if_needed(&log, 10, 5).expect("no-op succeeds"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
