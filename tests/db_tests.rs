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

#[tokio::test]
async fn custom_task_stores_photo_on_disk() {
    let pool = memory_pool().await;
    let dir = std::env::temp_dir().join(format!("sheffield-uploads-{}", std::process::id()));
    // "hello" as base64, wrapped in a data URI like a browser would send.
    let photo = "data:image/jpeg;base64,aGVsbG8=";

    let id = db::insert_custom_task(&pool, 3, "Feed the dog", Some(photo), &dir)
        .await
        .expect("insert");
    assert!(id >= 1);

    let tasks = db::custom_tasks(&pool, 3).await.expect("tasks");
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].title, "Feed the dog");
    assert!(!tasks[0].is_completed);

    let path = tasks[0].photo_path.clone().expect("photo path");
    assert!(path.starts_with("/uploads/task-3-"));
    let stored = dir.join(path.trim_start_matches("/uploads/"));
    assert_eq!(std::fs::read(&stored).expect("stored photo"), b"hello");

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
    let dir = std::env::temp_dir().join("sheffield-uploads-unused");

    db::insert_custom_task(&pool, 1, "Read a chapter", None, &dir)
        .await
        .expect("insert");

    let tasks = db::custom_tasks(&pool, 1).await.expect("tasks");
    assert_eq!(tasks.len(), 1);
    assert!(tasks[0].photo_path.is_none());
}

#[tokio::test]
async fn user_ids_are_constrained_to_the_four_profiles() {
    let pool = memory_pool().await;

    let result = db::set_routine_completion(&pool, 9, 1, true, "2025-01-01").await;
    assert!(
        result.is_err(),
        "user_id 9 must violate the CHECK constraint"
    );
}
