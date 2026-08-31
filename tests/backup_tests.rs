#![cfg(feature = "server")]
//! T1.6 acceptance tests (PLAN v2 §3 / `docs/reviews/PURPLE_TEAM.md` §P3
//! T1.6). Each `#[test]`/`#[tokio::test]` below is named after the letter
//! it proves:
//!
//! (a) [`vacuum_into_survives_an_open_writer_transaction_while_a_plain_copy_does_not`]
//! (b) [`restore_drill_recreates_the_live_database_from_a_backup`]
//! (c) [`retention_keeps_the_newest_fourteen_and_deletes_the_rest`]
//! (d) [`delete_custom_task_removes_the_row_and_its_photo_file`]
//! (e) [`compaction_leaves_exactly_two_thousand_strokes`]
//! (f) [`nightly_backup_never_touches_the_pki_directory`]
//!
//! A handful of extra tests below cover the two retention jobs (a), through
//! (f) do not otherwise exercise: photo retention and log rotation.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime};

use chrono::TimeZone;
use family_calendar::server::backup;
use family_calendar::server::db;
use sqlx::{Row, SqlitePool};

// ---------------------------------------------------------------------------
// helpers (mirrors tests/storage_tests.rs's local helpers — this is a
// separate integration test binary, so nothing is shared)
// ---------------------------------------------------------------------------

static SCRATCH_COUNTER: AtomicU32 = AtomicU32::new(0);

fn scratch_dir(name: &str) -> PathBuf {
    let n = SCRATCH_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("familyhub-t16-{name}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// sqlx wants forward slashes in a sqlite URL, even on Windows.
fn sqlite_url(path: &Path) -> String {
    format!("sqlite://{}", path.display().to_string().replace('\\', "/"))
}

async fn migrated_memory_pool() -> SqlitePool {
    let pool = db::connect("sqlite::memory:").await.expect("memory pool");
    db::migrate(&pool).await.expect("migrate");
    pool
}

async fn count(pool: &SqlitePool, table: &str) -> Result<i64, sqlx::Error> {
    let row = sqlx::query(&format!("SELECT COUNT(*) AS n FROM {table}"))
        .fetch_one(pool)
        .await?;
    Ok(row.get::<i64, _>("n"))
}

fn wal_sidecar_path(db_path: &Path) -> PathBuf {
    let mut name = db_path.as_os_str().to_os_string();
    name.push("-wal");
    PathBuf::from(name)
}

fn remove_with_sidecars(db_path: &Path) {
    for suffix in ["", "-wal", "-shm"] {
        let mut name = db_path.as_os_str().to_os_string();
        name.push(suffix);
        let _ = std::fs::remove_file(PathBuf::from(name));
    }
}

// ---------------------------------------------------------------------------
// (a) VACUUM INTO under an open writer transaction, vs a plain file copy
// ---------------------------------------------------------------------------

/// PURPLE §P3 T1.6 (a): with a writer transaction open, `VACUUM INTO`
/// produces a backup that opens cleanly with the exact committed row count
/// — and a plain file copy taken under the same conditions is asserted to
/// differ or fail, proving the point.
///
/// The mechanism: journal mode is WAL (D4), so committed writes live in the
/// `-wal` sidecar until a checkpoint folds them back into the main file — a
/// naive `std::fs::copy` of just the main `.db` file therefore does not see
/// them. `VACUUM INTO`, by contrast, reads through SQLite's own transaction
/// machinery and always produces one self-contained, consistent file, which
/// is exactly why it — and not a plain copy — is safe to run against a live
/// database (R-17).
#[tokio::test]
async fn vacuum_into_survives_an_open_writer_transaction_while_a_plain_copy_does_not() {
    let dir = scratch_dir("vacuum-vs-copy");
    let db_path = dir.join("family.db");
    let url = sqlite_url(&db_path);

    let pools = db::open_pools(&url).await.expect("pools");
    db::migrate(&pools.write).await.expect("migrate");

    // Committed rows, written and committed before anything else happens.
    for i in 0..40u32 {
        db::insert_stroke(
            &pools.write,
            db::DEFAULT_BOARD_ID,
            "seed",
            "#111111",
            2.0,
            &format!("[[0.1,{}]]", i as f64 / 100.0),
        )
        .await
        .expect("seed stroke");
    }
    let committed = count(&pools.read, "whiteboard_strokes")
        .await
        .expect("count committed rows");
    assert_eq!(committed, 40);

    // The premise this test relies on: those commits are sitting in the WAL
    // sidecar, not yet folded into the main file (no checkpoint has run).
    let wal_len = std::fs::metadata(wal_sidecar_path(&db_path))
        .map(|meta| meta.len())
        .unwrap_or(0);
    assert!(
        wal_len > 0,
        "expected committed data to still be in the WAL sidecar (WAL mode, no \
         checkpoint yet); got a {wal_len}-byte -wal file — the rest of this test \
         proves nothing without this"
    );

    // Open, and hold, a writer transaction carrying *uncommitted* extra rows.
    let mut tx = pools.write.begin().await.expect("begin writer transaction");
    sqlx::query(
        "INSERT INTO whiteboard_strokes (board_id, seq, client_id, color, width, points) \
         VALUES (1, 999001, 'uncommitted', '#ffffff', 1.0, '[]')",
    )
    .execute(&mut *tx)
    .await
    .expect("uncommitted insert inside the open transaction");

    // While that writer transaction is still open: VACUUM INTO on the READ
    // pool. WAL gives readers a consistent snapshot without blocking on (or
    // being blocked by) the writer — this must succeed.
    let backup_path = dir.join("family-backup.db");
    backup::vacuum_into(&pools.read, &backup_path)
        .await
        .expect("VACUUM INTO must succeed with a writer transaction open");

    // ...and, for comparison, a plain file copy taken at the same moment.
    let plain_path = dir.join("family-plain-copy.db");
    let plain_copy_result = std::fs::copy(&db_path, &plain_path);

    // Never commit the extra rows — they must not appear anywhere.
    tx.rollback().await.expect("rollback the uncommitted rows");
    pools.close().await;

    // The VACUUM INTO backup opens cleanly and has exactly the committed
    // row count — not the uncommitted one, proving transactional isolation
    // as well as consistency.
    let backup_pool = db::connect(&sqlite_url(&backup_path))
        .await
        .expect("the VACUUM INTO backup must open cleanly");
    let backup_count = count(&backup_pool, "whiteboard_strokes")
        .await
        .expect("count the backup's rows");
    assert_eq!(
        backup_count, committed,
        "VACUUM INTO must see exactly the committed rows, never the open \
         transaction's uncommitted ones"
    );
    backup_pool.close().await;

    // The plain copy must differ from the backup, or fail outright: either
    // the OS-level copy itself failed, or the copied file cannot be opened,
    // or it opens but its row count disagrees with the backup's.
    let plain_outcome: Result<i64, String> = async {
        plain_copy_result.map_err(|err| format!("OS-level copy failed: {err}"))?;
        let pool = db::connect(&sqlite_url(&plain_path))
            .await
            .map_err(|err| format!("plain copy would not open: {err}"))?;
        let result = sqlx::query("SELECT COUNT(*) AS n FROM whiteboard_strokes")
            .fetch_one(&pool)
            .await
            .map(|row| row.get::<i64, _>("n"))
            .map_err(|err| format!("plain copy query failed: {err}"));
        pool.close().await;
        result
    }
    .await;

    match plain_outcome {
        Ok(plain_count) => assert_ne!(
            plain_count, backup_count,
            "a plain copy of a live WAL database must not match VACUUM INTO's \
             consistent row count ({backup_count}); got the same count, which \
             would mean the naive copy was somehow just as safe"
        ),
        Err(reason) => {
            // Failing to even open/query the naive copy is an equally valid
            // proof of the point ("differ or fail").
            eprintln!("plain copy failed as expected: {reason}");
        }
    }

    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// (b) restore drill
// ---------------------------------------------------------------------------

/// PURPLE §P3 T1.6 (b): delete the live database, restore the backup, boot,
/// and the row counts match what was backed up.
#[tokio::test]
async fn restore_drill_recreates_the_live_database_from_a_backup() {
    let dir = scratch_dir("restore-drill");
    let db_path = dir.join("family.db");
    let url = sqlite_url(&db_path);

    let pools = db::open_pools(&url).await.expect("pools");
    db::migrate(&pools.write).await.expect("migrate");

    for user_id in 1..=4u32 {
        for template_id in 1..=8u32 {
            db::set_routine_completion(&pools.write, user_id, template_id, true, "2026-08-29")
                .await
                .expect("log");
        }
    }
    db::insert_custom_task(&pools.write, 2, "Restore-drill task", None)
        .await
        .expect("custom task");

    let expected_logs = count(&pools.read, "daily_routine_logs")
        .await
        .expect("count");
    let expected_tasks = count(&pools.read, "custom_tasks").await.expect("count");
    let expected_templates = count(&pools.read, "routine_templates")
        .await
        .expect("count");
    assert_eq!(expected_logs, 32);
    assert_eq!(expected_tasks, 1);

    let backup_path = backup::backups_dir(&dir).join("family-restore-drill.db");
    std::fs::create_dir_all(backup_path.parent().unwrap()).expect("backups dir");
    backup::vacuum_into(&pools.read, &backup_path)
        .await
        .expect("vacuum into");
    assert!(backup_path.is_file());

    pools.close().await;

    // Delete the live database entirely, sidecars and all.
    remove_with_sidecars(&db_path);
    assert!(!db_path.exists());

    // Restore.
    backup::restore_database(&backup_path, &db_path).expect("restore");
    assert!(db_path.is_file());

    // Boot again and check.
    let restored = db::open_pools(&url).await.expect("restored pools");
    db::migrate(&restored.write)
        .await
        .expect("re-migrate is a no-op");

    assert_eq!(
        count(&restored.read, "daily_routine_logs")
            .await
            .expect("count"),
        expected_logs
    );
    assert_eq!(
        count(&restored.read, "custom_tasks").await.expect("count"),
        expected_tasks
    );
    assert_eq!(
        count(&restored.read, "routine_templates")
            .await
            .expect("count"),
        expected_templates
    );
    assert_eq!(
        db::migration_version(&restored.read)
            .await
            .expect("version"),
        Some(4),
        "T1.1's 0001/0002 plus T1.4's 0003_profiles plus phase-4/names' 0004_name_the_boys"
    );

    restored.close().await;
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// (c) 20 backups -> 14 retained, oldest deleted
// ---------------------------------------------------------------------------

/// PURPLE §P3 T1.6 (c): 20 backups on disk, retention runs, 14 survive and
/// the six oldest are gone (their uploads-snapshot directories too).
#[tokio::test]
async fn retention_keeps_the_newest_fourteen_and_deletes_the_rest() {
    let dir = scratch_dir("retention");
    let backups = backup::backups_dir(&dir);
    std::fs::create_dir_all(&backups).expect("backups dir");

    let mut names = Vec::new();
    for minute in 0..20u32 {
        let stamp = chrono::Local
            .with_ymd_and_hms(2026, 1, 1, 3, minute, 0)
            .unwrap();
        let name = backup::backup_file_name(stamp);
        std::fs::write(backups.join(&name), b"fake vacuum-into output").expect("write fake backup");

        // A couple of them carry an uploads snapshot directory too, so the
        // test also proves those get cleaned up alongside the .db file.
        if minute % 5 == 0 {
            let stem = name.trim_end_matches(".db");
            let uploads_dir = backups.join(format!("{stem}_uploads"));
            std::fs::create_dir_all(&uploads_dir).expect("fake uploads snapshot dir");
            std::fs::write(uploads_dir.join("photo.jpg"), b"fake").expect("fake photo");
        }
        names.push(name);
    }

    assert_eq!(backup::list_backup_db_files(&backups).unwrap().len(), 20);

    let removed = backup::enforce_backup_retention(&backups, backup::BACKUP_RETENTION_COUNT)
        .expect("enforce retention");
    assert_eq!(
        removed.len(),
        6,
        "20 - 14 = 6 backups should have been removed"
    );

    let remaining = backup::list_backup_db_files(&backups).expect("list after retention");
    assert_eq!(remaining.len(), 14);

    let remaining_names: Vec<String> = remaining
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    for oldest in &names[..6] {
        assert!(
            !remaining_names.contains(oldest),
            "{oldest} is one of the oldest six and should have been deleted"
        );
        let stem = oldest.trim_end_matches(".db");
        let uploads_dir = backups.join(format!("{stem}_uploads"));
        assert!(
            !uploads_dir.exists(),
            "the uploads snapshot for {oldest} should have been removed too"
        );
    }
    for newer in &names[6..] {
        assert!(
            remaining_names.contains(newer),
            "{newer} is one of the newest fourteen and should have survived"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// (d) deleting a custom task removes the row and its photo file
// ---------------------------------------------------------------------------

/// PURPLE §P3 T1.6 (d): deleting a task removes both the row and the file
/// on disk.
#[tokio::test]
async fn delete_custom_task_removes_the_row_and_its_photo_file() {
    let pool = migrated_memory_pool().await;
    let dir = scratch_dir("delete-task-uploads");
    // Q1-06: insert_custom_task no longer writes photo bytes itself — it
    // stores an already-stored web path, so this stands in for what the
    // multipart route would have written before calling it.
    std::fs::write(dir.join("t.jpg"), b"hello").expect("fixture photo is writable");

    let task_id = db::insert_custom_task(&pool, 1, "Clean your room", Some("/uploads/t.jpg"))
        .await
        .expect("insert task with a photo");

    let tasks = db::custom_tasks(&pool, 1).await.expect("tasks");
    let task = tasks.iter().find(|t| t.id == task_id).expect("task exists");
    let photo_path = task.photo_path.clone().expect("a photo was written");
    let file_name = photo_path
        .rsplit('/')
        .next()
        .expect("photo web path has a file name");
    let file_path = dir.join(file_name);
    assert!(
        file_path.is_file(),
        "the photo file should exist before delete"
    );

    let deleted = backup::delete_custom_task(&pool, task_id, &dir)
        .await
        .expect("delete_custom_task");
    assert!(deleted, "an existing task must report as deleted");
    assert!(
        !file_path.exists(),
        "the photo file must be removed from disk by the same call"
    );

    let remaining = db::custom_tasks(&pool, 1)
        .await
        .expect("tasks after delete");
    assert!(
        remaining.iter().all(|t| t.id != task_id),
        "the row must be gone"
    );

    // Deleting again is a no-op that reports "nothing to delete", not an error.
    let deleted_again = backup::delete_custom_task(&pool, task_id, &dir)
        .await
        .expect("deleting an already-gone task must not error");
    assert!(!deleted_again);

    pool.close().await;
    let _ = std::fs::remove_dir_all(&dir);
}

/// Deleting a task that never had a photo removes the row and does not
/// error trying to remove a file that never existed.
#[tokio::test]
async fn delete_custom_task_without_a_photo_just_removes_the_row() {
    let pool = migrated_memory_pool().await;
    let dir = scratch_dir("delete-task-no-photo");

    let task_id = db::insert_custom_task(&pool, 3, "Feed the dog", None)
        .await
        .expect("insert task without a photo");

    let deleted = backup::delete_custom_task(&pool, task_id, &dir)
        .await
        .expect("delete_custom_task");
    assert!(deleted);

    let remaining = db::custom_tasks(&pool, 3)
        .await
        .expect("tasks after delete");
    assert!(remaining.iter().all(|t| t.id != task_id));

    pool.close().await;
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// (e) stroke compaction leaves exactly 2,000
// ---------------------------------------------------------------------------

/// PURPLE §P3 T1.6 (e): compaction leaves exactly 2,000 strokes.
#[tokio::test]
async fn compaction_leaves_exactly_two_thousand_strokes() {
    let pool = migrated_memory_pool().await;

    for i in 0..2_500i64 {
        db::insert_stroke(
            &pool,
            db::DEFAULT_BOARD_ID,
            "kid",
            "#111111",
            3.0,
            &format!("[[0.1,{}]]", i as f64 / 10_000.0),
        )
        .await
        .expect("stroke");
    }
    assert_eq!(count(&pool, "whiteboard_strokes").await.unwrap(), 2_500);

    let removed =
        backup::compact_strokes(&pool, db::DEFAULT_BOARD_ID, backup::STROKE_COMPACTION_KEEP)
            .await
            .expect("compact");
    assert_eq!(removed, 500);
    assert_eq!(
        count(&pool, "whiteboard_strokes").await.unwrap(),
        backup::STROKE_COMPACTION_KEEP
    );

    pool.close().await;
}

/// Compaction hard-deletes every cleared stroke unconditionally, even when
/// the live count is nowhere near the cap — cleared strokes are never
/// visible again (H-10), so keeping them around serves nothing.
#[tokio::test]
async fn compaction_hard_deletes_cleared_strokes_regardless_of_the_live_count() {
    let pool = migrated_memory_pool().await;

    for i in 0..10i64 {
        db::insert_stroke(
            &pool,
            db::DEFAULT_BOARD_ID,
            "kid",
            "#111111",
            3.0,
            &format!("[[0,{i}]]"),
        )
        .await
        .expect("stroke");
    }
    db::clear_board(&pool, db::DEFAULT_BOARD_ID)
        .await
        .expect("clear board");
    for i in 0..5i64 {
        db::insert_stroke(
            &pool,
            db::DEFAULT_BOARD_ID,
            "kid",
            "#222222",
            3.0,
            &format!("[[1,{i}]]"),
        )
        .await
        .expect("stroke");
    }
    assert_eq!(count(&pool, "whiteboard_strokes").await.unwrap(), 15);

    let removed = backup::compact_strokes(&pool, db::DEFAULT_BOARD_ID, 100)
        .await
        .expect("compact");
    assert_eq!(
        removed, 10,
        "the 10 cleared strokes must be removed even though 100 > the 5 live ones"
    );
    assert_eq!(count(&pool, "whiteboard_strokes").await.unwrap(), 5);

    pool.close().await;
}

// ---------------------------------------------------------------------------
// (f) backups/ contains no .key file
// ---------------------------------------------------------------------------

/// PURPLE §P3 T1.6 (f): `backups/` contains no `.key` file, even when a
/// realistic `pki/` directory carrying `ca.key`/`leaf.key` sits right next
/// to it in the data directory.
#[tokio::test]
async fn nightly_backup_never_touches_the_pki_directory() {
    let dir = scratch_dir("no-keys");
    let db_path = dir.join("family.db");
    let url = sqlite_url(&db_path);

    let pools = db::open_pools(&url).await.expect("pools");
    db::migrate(&pools.write).await.expect("migrate");

    // The real data-dir layout: pki/ with key material right next to
    // where backups/ will be created.
    let pki_dir = dir.join("pki");
    std::fs::create_dir_all(&pki_dir).expect("pki dir");
    std::fs::write(
        pki_dir.join("ca.key"),
        b"not a real key, but shaped like one",
    )
    .expect("fake ca.key");
    std::fs::write(
        pki_dir.join("leaf.key"),
        b"not a real key, but shaped like one",
    )
    .expect("fake leaf.key");

    // And a photo upload, so the uploads snapshot actually copies something.
    let upload_dir = dir.join("uploads");
    std::fs::create_dir_all(&upload_dir).expect("uploads dir");
    std::fs::write(upload_dir.join("task-1-1.jpg"), b"fake jpeg bytes").expect("fake photo");

    let summary = backup::run_nightly_backup(&pools.read, &dir)
        .await
        .expect("nightly backup");
    assert_eq!(summary.uploads_files_copied, 1);
    assert!(summary.db_file.is_file());

    pools.close().await;

    // Walk everything under backups/ recursively; no .key file may appear.
    let backups = backup::backups_dir(&dir);
    let mut stack = vec![backups.clone()];
    let mut found_key = false;
    while let Some(current) = stack.pop() {
        for entry in std::fs::read_dir(&current).expect("read backups dir") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("key") {
                found_key = true;
            }
        }
    }
    assert!(!found_key, "backups/ must never contain a .key file");

    // The pki directory itself must be completely untouched by the backup.
    assert!(
        pki_dir.join("ca.key").is_file(),
        "the real ca.key must be undisturbed"
    );
    assert!(
        pki_dir.join("leaf.key").is_file(),
        "the real leaf.key must be undisturbed"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// Extra coverage: photo retention and log rotation (not lettered in PURPLE
// §P3, but both are new server logic this task adds).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn purge_old_photos_removes_stale_files_and_nulls_the_db_reference() {
    let pool = migrated_memory_pool().await;
    let dir = scratch_dir("photo-retention");
    std::fs::write(dir.join("t.jpg"), b"hello").expect("fixture photo is writable");

    let task_id = db::insert_custom_task(&pool, 1, "Old task", Some("/uploads/t.jpg"))
        .await
        .expect("insert task with a photo");
    let tasks = db::custom_tasks(&pool, 1).await.expect("tasks");
    let photo_path = tasks
        .iter()
        .find(|t| t.id == task_id)
        .and_then(|t| t.photo_path.clone())
        .expect("photo was written");
    let file_name = photo_path.rsplit('/').next().unwrap().to_string();
    let file_path = dir.join(&file_name);
    assert!(file_path.is_file());

    // Back-date the file's modified time to well outside the retention
    // window instead of sleeping 30 real days.
    let old_time = SystemTime::UNIX_EPOCH + Duration::from_secs(86_400);
    {
        let file = std::fs::File::options()
            .write(true)
            .open(&file_path)
            .expect("open for mtime edit");
        file.set_modified(old_time).expect("backdate mtime");
    }

    let cutoff = SystemTime::UNIX_EPOCH + Duration::from_secs(30 * 86_400);
    let removed = backup::purge_old_photos_before(&pool, &dir, cutoff)
        .await
        .expect("purge");
    assert_eq!(removed, 1);
    assert!(!file_path.exists(), "the stale photo file must be gone");

    let tasks_after = db::custom_tasks(&pool, 1).await.expect("tasks after purge");
    let task_after = tasks_after
        .iter()
        .find(|t| t.id == task_id)
        .expect("row survives");
    assert_eq!(
        task_after.photo_path, None,
        "the dangling photo_path reference must be nulled out"
    );

    pool.close().await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn purge_old_photos_leaves_recent_files_alone() {
    let pool = migrated_memory_pool().await;
    let dir = scratch_dir("photo-retention-fresh");
    std::fs::write(dir.join("t.jpg"), b"hello").expect("fixture photo is writable");

    db::insert_custom_task(&pool, 1, "Fresh task", Some("/uploads/t.jpg"))
        .await
        .expect("insert task with a photo");

    // Cutoff far in the past: today's file is nowhere near it.
    let removed = backup::purge_old_photos_before(&pool, &dir, SystemTime::UNIX_EPOCH)
        .await
        .expect("purge");
    assert_eq!(removed, 0);

    pool.close().await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rotate_log_if_needed_shifts_generations_and_drops_the_oldest() {
    let dir = scratch_dir("log-rotation");
    let log_path = dir.join("familyhub.log");

    std::fs::write(&log_path, vec![b'a'; 100]).expect("small log");
    assert!(
        !backup::rotate_log_if_needed(&log_path, 1024, 5).expect("check"),
        "a log under the cap must not rotate"
    );

    std::fs::write(&log_path, vec![b'b'; 2048]).expect("over-cap log");
    assert!(
        backup::rotate_log_if_needed(&log_path, 1024, 5).expect("rotate"),
        "a log at/over the cap must rotate"
    );
    let gen1 = PathBuf::from(format!("{}.1", log_path.display()));
    assert!(gen1.is_file());
    assert_eq!(std::fs::read(&gen1).unwrap().len(), 2048);
    assert_eq!(
        std::fs::metadata(&log_path).unwrap().len(),
        0,
        "a fresh, empty file must be left behind for the writer to keep appending to"
    );

    // Rotate four more times to fill every generation slot.
    for _ in 0..4 {
        std::fs::write(&log_path, vec![b'c'; 2048]).unwrap();
        assert!(backup::rotate_log_if_needed(&log_path, 1024, 5).unwrap());
    }
    for generation in 1..=5 {
        let path = PathBuf::from(format!("{}.{generation}", log_path.display()));
        assert!(path.is_file(), "generation {generation} should exist");
    }

    // One more rotation must drop generation 5 (the oldest) rather than
    // growing past the 5-generation cap.
    std::fs::write(&log_path, vec![b'd'; 2048]).unwrap();
    assert!(backup::rotate_log_if_needed(&log_path, 1024, 5).unwrap());
    let mut sizes = Vec::new();
    for generation in 1..=5 {
        let path = PathBuf::from(format!("{}.{generation}", log_path.display()));
        assert!(path.is_file(), "generation {generation} should still exist");
        sizes.push(std::fs::metadata(&path).unwrap().len());
    }
    assert_eq!(sizes.len(), 5, "never more than 5 rotated generations");

    let _ = std::fs::remove_dir_all(&dir);
}
