-- 0003_profiles — profiles table + FK-backed user_id columns (T1.4).
--
-- Replaces the two `CHECK (user_id BETWEEN 1 AND 4)` constraints from
-- 0001_init (`daily_routine_logs.user_id`, `custom_tasks.user_id`) with real
-- foreign keys to a new `profiles` table (W5 / PURPLE §P5.5 default 11).
-- SQLite has no `ALTER TABLE ... DROP CONSTRAINT`, so both tables are
-- rebuilt in place: create the replacement with the FK, copy every row
-- across (validated against the FK as it goes, since this connection runs
-- with `PRAGMA foreign_keys = ON` — see `db::connect_options`), drop the
-- old table, rename the replacement into the old table's name.
--
-- `profiles` is seeded with the four existing hardcoded "Boy 1".."Boy 4"
-- identities at ids 1..4 **before** the rebuild, so every pre-existing
-- `daily_routine_logs`/`custom_tasks` row — whose `user_id` the old CHECK
-- already limited to 1..4 — satisfies the new foreign key with no data loss,
-- whether the database is fresh or a baselined v1 `family.db`. A 5th, 6th, ...
-- profile can be added afterwards with a plain `INSERT INTO profiles`: the
-- row-count limit was the CHECK, and the CHECK is gone.

CREATE TABLE IF NOT EXISTS profiles (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT    NOT NULL,
    color       TEXT    NOT NULL DEFAULT '#3B82F6',
    avatar      TEXT,
    is_parent   INTEGER NOT NULL DEFAULT 0 CHECK (is_parent IN (0, 1)),
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT INTO profiles (id, name, color, is_parent, sort_order)
VALUES
    (1, 'Boy 1', '#EF4444', 0, 1),
    (2, 'Boy 2', '#F59E0B', 0, 2),
    (3, 'Boy 3', '#10B981', 0, 3),
    (4, 'Boy 4', '#3B82F6', 0, 4)
ON CONFLICT (id) DO NOTHING;

-- daily_routine_logs: drop CHECK (user_id BETWEEN 1 AND 4), gain FK -> profiles.
CREATE TABLE daily_routine_logs_new (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id      INTEGER NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    template_id  INTEGER NOT NULL REFERENCES routine_templates(id) ON DELETE CASCADE,
    date_logged  DATE    NOT NULL,
    completed_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (user_id, template_id, date_logged)
);

INSERT INTO daily_routine_logs_new (id, user_id, template_id, date_logged, completed_at)
SELECT id, user_id, template_id, date_logged, completed_at FROM daily_routine_logs;

DROP TABLE daily_routine_logs;
ALTER TABLE daily_routine_logs_new RENAME TO daily_routine_logs;

-- custom_tasks: drop CHECK (user_id BETWEEN 1 AND 4), gain FK -> profiles.
-- `due_date` already exists (0002_core), so it is carried across too.
CREATE TABLE custom_tasks_new (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id      INTEGER NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    title        TEXT    NOT NULL,
    photo_path   TEXT,
    is_completed BOOLEAN NOT NULL DEFAULT 0,
    due_date     DATE,
    created_at   TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT INTO custom_tasks_new (id, user_id, title, photo_path, is_completed, due_date, created_at)
SELECT id, user_id, title, photo_path, is_completed, due_date, created_at FROM custom_tasks;

DROP TABLE custom_tasks;
ALTER TABLE custom_tasks_new RENAME TO custom_tasks;
