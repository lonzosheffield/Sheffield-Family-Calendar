-- 0001_init — the v1 schema, reproduced verbatim.
--
-- Every statement is `CREATE TABLE IF NOT EXISTS` so that this migration is a
-- no-op against a database that already carries the v1 schema. An existing v1
-- `family.db` (one that predates `_sqlx_migrations` entirely) is *baselined*
-- instead of executed — see `db::baseline_legacy_database` — so the two paths
-- agree: after 0001 the schema below exists exactly once, with the original
-- `CHECK (user_id BETWEEN 1 AND 4)` constraints still in place. T1.4's
-- `0003_profiles.sql` is what replaces those CHECKs with real foreign keys.

CREATE TABLE IF NOT EXISTS routine_templates (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    title       TEXT    NOT NULL UNIQUE,
    description TEXT    NOT NULL,
    icon_name   TEXT    NOT NULL,
    sort_order  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS daily_routine_logs (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id      INTEGER NOT NULL CHECK (user_id BETWEEN 1 AND 4),
    template_id  INTEGER NOT NULL REFERENCES routine_templates(id) ON DELETE CASCADE,
    date_logged  DATE    NOT NULL,
    completed_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (user_id, template_id, date_logged)
);

CREATE TABLE IF NOT EXISTS custom_tasks (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id      INTEGER NOT NULL CHECK (user_id BETWEEN 1 AND 4),
    title        TEXT    NOT NULL,
    photo_path   TEXT,
    is_completed BOOLEAN NOT NULL DEFAULT 0,
    created_at   TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
