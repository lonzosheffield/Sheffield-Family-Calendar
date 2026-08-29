use std::path::{Path, PathBuf};
use std::str::FromStr;

use base64::Engine;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use tokio::sync::OnceCell;

use crate::server::config::FamilyHubConfig;
use crate::shared::types::{CustomTaskView, RoutineItemView};

static POOL: OnceCell<SqlitePool> = OnceCell::const_new();

/// Absolute directory photo tasks are written to (T0.5: resolved from
/// `FamilyHubConfig`, never a path relative to the process's CWD).
pub fn upload_dir() -> PathBuf {
    FamilyHubConfig::load().upload_dir()
}

/// The process wide connection pool, created (and migrated) on first use.
///
/// `DATABASE_URL`, when set, wins outright (integration tests use this to
/// point every test process at its own throwaway sqlite file). Otherwise the
/// URL is derived from [`FamilyHubConfig`], which resolves an **absolute**
/// path under `FAMILY_HUB_DATA_DIR` (default `%ProgramData%\FamilyHub`) —
/// never a bare `family.db` relative to the current working directory
/// (G23/R-14: under a Windows service the CWD is `C:\Windows\System32`).
pub async fn pool() -> Result<&'static SqlitePool, sqlx::Error> {
    POOL.get_or_try_init(|| async {
        let url = resolve_database_url().map_err(sqlx::Error::Io)?;
        let pool = connect(&url).await?;
        migrate(&pool).await?;
        Ok(pool)
    })
    .await
}

fn resolve_database_url() -> std::io::Result<String> {
    if let Ok(url) = std::env::var("DATABASE_URL") {
        return Ok(url);
    }

    let config = FamilyHubConfig::load();
    config.ensure_dirs_and_log()?;
    Ok(config.database_url())
}

/// Open a pool against `url`, creating the SQLite file when missing.
pub async fn connect(url: &str) -> Result<SqlitePool, sqlx::Error> {
    let options = SqliteConnectOptions::from_str(url)?
        .create_if_missing(true)
        .foreign_keys(true);

    SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
}

/// Create the schema when absent and seed the Sheffield morning routine.
pub async fn migrate(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS routine_templates (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            title       TEXT    NOT NULL UNIQUE,
            description TEXT    NOT NULL,
            icon_name   TEXT    NOT NULL,
            sort_order  INTEGER NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS daily_routine_logs (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id      INTEGER NOT NULL CHECK (user_id BETWEEN 1 AND 4),
            template_id  INTEGER NOT NULL REFERENCES routine_templates(id) ON DELETE CASCADE,
            date_logged  DATE    NOT NULL,
            completed_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
            UNIQUE (user_id, template_id, date_logged)
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS custom_tasks (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id      INTEGER NOT NULL CHECK (user_id BETWEEN 1 AND 4),
            title        TEXT    NOT NULL,
            photo_path   TEXT,
            is_completed BOOLEAN NOT NULL DEFAULT 0,
            created_at   TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
        );
        "#,
    )
    .execute(pool)
    .await?;

    seed_routine_templates(pool).await
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
