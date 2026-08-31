#![cfg(feature = "server")]

use family_calendar::server::db;
use family_calendar::shared::types::routine_progress;
use sqlx::SqlitePool;

async fn memory_pool() -> SqlitePool {
    let pool = db::connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite");
    db::migrate(&pool).await.expect("migrations");
    pool
}

/// Migrate `pool` through `migrations/0003_profiles.sql` only, stopping one
/// migration short of `0004_name_the_boys.sql`. Used to simulate a database
/// exactly as it stood before the owner's software upgraded: the four
/// profiles seeded but still named "Boy 1".."Boy 4", so a test can rename one
/// the way the phone's rename UI would, then apply 0004 on top and check that
/// rename survived.
///
/// Builds a scratch `Migrator` over copies of just the first three migration
/// files (byte-identical to the real ones, so their checksums match what the
/// full embedded `db::MIGRATOR` expects) rather than touching any migration
/// file itself.
async fn migrate_through_0003_profiles(pool: &SqlitePool) {
    let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    let scratch = std::env::temp_dir().join(format!(
        "familyhub-through-0003-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&scratch).expect("scratch migrations dir");
    for file in ["0001_init.sql", "0002_core.sql", "0003_profiles.sql"] {
        std::fs::copy(migrations_dir.join(file), scratch.join(file))
            .unwrap_or_else(|err| panic!("copy {file} into scratch migrations dir: {err}"));
    }

    let migrator = sqlx::migrate::Migrator::new(scratch.clone())
        .await
        .expect("build a 0001..0003-only migrator");
    migrator.run(pool).await.expect("apply 0001..0003");

    let _ = std::fs::remove_dir_all(&scratch);
}

#[tokio::test]
async fn migrations_seed_the_eight_sheffield_routine_items() {
    let pool = memory_pool().await;

    let items = db::daily_routine(&pool, 1, "2025-01-01")
        .await
        .expect("routine");

    assert_eq!(items.len(), 8);
    assert_eq!(items[0].title, "Wake up and thank God for the day!");
    assert_eq!(items[0].description, "Lamentations 3:23");
    assert_eq!(items[0].icon_name, "sun");
    assert_eq!(items[7].title, "Start your school work.");
    assert_eq!(items[7].icon_name, "graduation-cap");
    assert!(items.iter().all(|item| !item.completed));
    assert!(items.windows(2).all(|w| w[0].sort_order < w[1].sort_order));
}

#[tokio::test]
async fn migrations_are_idempotent() {
    let pool = memory_pool().await;
    db::migrate(&pool).await.expect("second migration");

    let items = db::daily_routine(&pool, 1, "2025-01-01")
        .await
        .expect("routine");
    assert_eq!(items.len(), 8);
}

#[tokio::test]
async fn toggling_a_task_is_scoped_to_user_and_date() {
    let pool = memory_pool().await;

    db::set_routine_completion(&pool, 2, 3, true, "2025-01-01")
        .await
        .expect("complete");

    let user_two = db::daily_routine(&pool, 2, "2025-01-01").await.unwrap();
    assert!(
        user_two
            .iter()
            .find(|i| i.template_id == 3)
            .unwrap()
            .completed
    );
    assert_eq!(user_two.iter().filter(|i| i.completed).count(), 1);
    assert!((routine_progress(&user_two) - 12.5).abs() < f64::EPSILON);

    // Another profile and another day are unaffected.
    let user_one = db::daily_routine(&pool, 1, "2025-01-01").await.unwrap();
    assert!(user_one.iter().all(|i| !i.completed));
    let tomorrow = db::daily_routine(&pool, 2, "2025-01-02").await.unwrap();
    assert!(tomorrow.iter().all(|i| !i.completed));
}

#[tokio::test]
async fn completing_twice_then_clearing_leaves_no_log() {
    let pool = memory_pool().await;

    for _ in 0..2 {
        db::set_routine_completion(&pool, 1, 1, true, "2025-01-01")
            .await
            .expect("complete");
    }
    let items = db::daily_routine(&pool, 1, "2025-01-01").await.unwrap();
    assert_eq!(items.iter().filter(|i| i.completed).count(), 1);

    db::set_routine_completion(&pool, 1, 1, false, "2025-01-01")
        .await
        .expect("clear");
    let items = db::daily_routine(&pool, 1, "2025-01-01").await.unwrap();
    assert!(items.iter().all(|i| !i.completed));
}

#[tokio::test]
async fn full_routine_reaches_one_hundred_percent() {
    let pool = memory_pool().await;

    for template_id in 1..=8 {
        db::set_routine_completion(&pool, 4, template_id, true, "2025-01-01")
            .await
            .expect("complete");
    }

    let items = db::daily_routine(&pool, 4, "2025-01-01").await.unwrap();
    assert!((routine_progress(&items) - 100.0).abs() < 1e-9);
}

/// **Q1-06**: `insert_custom_task` no longer decodes a base64 payload and
/// writes it to disk itself — it stores whatever already-stored web path it
/// is given, exactly like [`db::insert_custom_task_with_due_date`]. This test
/// used to prove the base64 decode-and-write; it now proves the given path is
/// stored verbatim and the file this test wrote (standing in for a real
/// upload the multipart route already re-encoded) is left untouched.
#[tokio::test]
async fn custom_task_stores_the_given_path_and_the_file_remains() {
    let pool = memory_pool().await;
    let dir = std::env::temp_dir().join(format!("sheffield-uploads-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("uploads dir is creatable");
    let stored = dir.join("t.jpg");
    std::fs::write(&stored, b"hello").expect("fixture photo is writable");

    let id = db::insert_custom_task(&pool, 3, "Feed the dog", Some("/uploads/t.jpg"))
        .await
        .expect("insert");
    assert!(id >= 1);

    let tasks = db::custom_tasks(&pool, 3).await.expect("tasks");
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].title, "Feed the dog");
    assert!(!tasks[0].is_completed);

    assert_eq!(tasks[0].photo_path.as_deref(), Some("/uploads/t.jpg"));
    assert_eq!(
        std::fs::read(&stored).expect("stored photo"),
        b"hello",
        "insert_custom_task must not touch the file, only record the given path"
    );

    db::set_custom_task_completion(&pool, tasks[0].id, true)
        .await
        .expect("toggle");
    let tasks = db::custom_tasks(&pool, 3).await.expect("tasks");
    assert!(tasks[0].is_completed);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn custom_task_without_photo_has_no_path() {
    let pool = memory_pool().await;

    db::insert_custom_task(&pool, 1, "Read a chapter", None)
        .await
        .expect("insert");

    let tasks = db::custom_tasks(&pool, 1).await.expect("tasks");
    assert_eq!(tasks.len(), 1);
    assert!(tasks[0].photo_path.is_none());
}

/// T1.4 / W5: `migrations/0003_profiles.sql` replaces the old
/// `CHECK (user_id BETWEEN 1 AND 4)` constraints on `daily_routine_logs` and
/// `custom_tasks` with real foreign keys to a new `profiles` table. This
/// replaces the CHECK-era test above: `user_id` 9999 must now fail because no
/// such profile exists — **not** because it falls outside a hardcoded 1..4
/// range — and, unlike the old CHECK, a profile created *beyond* the original
/// four (a 5th here) must be accepted.
#[tokio::test]
async fn set_routine_completion_violates_foreign_key_for_an_unknown_profile() {
    let pool = memory_pool().await;

    let result = db::set_routine_completion(&pool, 9999, 1, true, "2025-01-01").await;
    let err = result.expect_err("user_id 9999 has no matching profiles row");
    let message = err.to_string().to_lowercase();
    assert!(
        message.contains("foreign key"),
        "expected a foreign key violation, got: {message}"
    );

    // A 5th profile — impossible under the old CHECK — satisfies the FK and
    // the insert succeeds, proving the constraint is "must reference
    // profiles", not "must be 1..=4".
    sqlx::query("INSERT INTO profiles (id, name) VALUES (5, 'Guest')")
        .execute(&pool)
        .await
        .expect("insert a 5th profile");
    db::set_routine_completion(&pool, 5, 1, true, "2025-01-01")
        .await
        .expect("profile 5 exists, so this must succeed");
}

/// The same replacement, for `custom_tasks.user_id` — the *other* CHECK
/// `0003_profiles.sql` drops (task description: "drop BOTH CHECK ...
/// constraints").
#[tokio::test]
async fn insert_custom_task_violates_foreign_key_for_an_unknown_profile() {
    let pool = memory_pool().await;

    let result = db::insert_custom_task(&pool, 9999, "Feed the dog", None).await;
    let err = result.expect_err("user_id 9999 has no matching profiles row");
    assert!(
        err.to_string().to_lowercase().contains("foreign key"),
        "expected a foreign key violation, got: {err}"
    );
}

/// `migrations/0004_name_the_boys.sql`: a fresh database renames all four
/// seeded placeholders to the owner's chosen names, in `sort_order`.
#[tokio::test]
async fn fresh_database_names_the_four_seeded_profiles() {
    let pool = memory_pool().await;

    let names: Vec<String> = sqlx::query_scalar("SELECT name FROM profiles ORDER BY sort_order")
        .fetch_all(&pool)
        .await
        .expect("profile names");

    assert_eq!(names, vec!["Isaiah", "Nathaniel", "Simeon", "Ezekiel"]);
}

/// `migrations/0004_name_the_boys.sql` matches on the exact seeded placeholder
/// name, so a profile the owner already renamed on the phone (here, id 2 from
/// "Boy 2" to "Nate") is left alone — while the other three, still at their
/// placeholder names, are renamed normally.
#[tokio::test]
async fn a_profile_already_renamed_before_0004_keeps_its_name() {
    let pool = db::connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite");
    migrate_through_0003_profiles(&pool).await;

    sqlx::query("UPDATE profiles SET name = 'Nate' WHERE id = 2")
        .execute(&pool)
        .await
        .expect("rename Boy 2 to Nate before 0004 ever runs");

    // The owner's software upgrades; `db::migrate` now also applies 0004.
    db::migrate(&pool).await.expect("migrate through 0004");

    let names: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, name FROM profiles ORDER BY sort_order")
            .fetch_all(&pool)
            .await
            .expect("profile names");

    assert_eq!(
        names,
        vec![
            (1, "Isaiah".to_string()),
            (2, "Nate".to_string()),
            (3, "Simeon".to_string()),
            (4, "Ezekiel".to_string()),
        ],
        "id 2 kept its phone-given name; the other three were renamed by 0004"
    );
}
