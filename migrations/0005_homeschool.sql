-- 0005_homeschool — the School ("house") tab's storage (HS1, PLAN_HOMESCHOOL §2 H1).
--
-- Append-only: every statement below creates something that does not exist in
-- 0001..0004, so nothing is rebuilt and no existing row is touched. The
-- `IF NOT EXISTS` forms keep the migration re-runnable against a database that
-- somehow already carries part of it.
--
-- Shape notes, all from H1 / the reviews it cites:
--
-- * `enrollments.week_started_on` (R-6) is the anchor **every** occurrence date
--   derives from. The week pointer never moves on its own (H2); a parent's
--   "Finish week" stamps this to that day, and the week's calendar span is the
--   7 days from it.
-- * `lesson_log` holds one row per *occurrence* that has been dealt with —
--   `done` or `skipped`. Untick deletes the row; there is no third state and no
--   "not done" row. The expression unique index below is that occurrence's
--   identity, `IFNULL(assignment_id, 0)` included so a daily subject with no
--   per-week assignment row (the `NULL assignment_id` case) still dedupes.
-- * `lesson_extras` (H8) is a parent-authored task pinned to one boy and one
--   date. Unlike a `lesson_log` row it carries its own state — `status IS NULL`
--   means "still to do", exactly like `custom_tasks` — because it is not an
--   occurrence of any curriculum row.
--
-- `db::connect_options` already runs every connection with
-- `PRAGMA foreign_keys = ON`, so the `REFERENCES ... ON DELETE CASCADE` clauses
-- here are enforced from the moment this migration lands: deleting a profile
-- takes its enrollment, log rows and extras with it, and deleting a curriculum
-- takes its subjects, assignments and term notes.

CREATE TABLE IF NOT EXISTS curricula (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    slug         TEXT    NOT NULL UNIQUE,
    name         TEXT    NOT NULL,
    weeks        INTEGER NOT NULL CHECK (weeks BETWEEN 1 AND 104),
    term_weeks   INTEGER NOT NULL DEFAULT 12 CHECK (term_weeks >= 1),
    source_note  TEXT,
    created_at   TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP);

CREATE TABLE IF NOT EXISTS subjects (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    curriculum_id  INTEGER NOT NULL REFERENCES curricula(id) ON DELETE CASCADE,
    name           TEXT    NOT NULL,
    category       TEXT    NOT NULL CHECK (category IN ('daily','reading','weekly','free_read')),
    source         TEXT,
    days           TEXT    NOT NULL DEFAULT 'MTWRF' CHECK (days GLOB '[MTWRFSU]*'),
    shared         INTEGER NOT NULL DEFAULT 0 CHECK (shared IN (0,1)),
    icon_name      TEXT,
    sort_order     INTEGER NOT NULL DEFAULT 0,
    UNIQUE (curriculum_id, name));

CREATE TABLE IF NOT EXISTS assignments (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    subject_id  INTEGER NOT NULL REFERENCES subjects(id) ON DELETE CASCADE,
    week        INTEGER NOT NULL CHECK (week >= 1),
    ordinal     INTEGER NOT NULL DEFAULT 1 CHECK (ordinal >= 1),
    text        TEXT    NOT NULL,
    detail      TEXT,
    days        TEXT    CHECK (days IS NULL OR days GLOB '[MTWRFSU]*'),
    UNIQUE (subject_id, week, ordinal));

CREATE TABLE IF NOT EXISTS term_notes (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    curriculum_id  INTEGER NOT NULL REFERENCES curricula(id) ON DELETE CASCADE,
    term           INTEGER NOT NULL CHECK (term >= 1),
    kind           TEXT    NOT NULL CHECK (kind IN ('geography','free_read','poetry')),
    text           TEXT    NOT NULL,
    sort_order     INTEGER NOT NULL DEFAULT 0);

CREATE TABLE IF NOT EXISTS enrollments (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    profile_id       INTEGER NOT NULL UNIQUE REFERENCES profiles(id) ON DELETE CASCADE,
    curriculum_id    INTEGER NOT NULL REFERENCES curricula(id)      ON DELETE CASCADE,
    current_week     INTEGER NOT NULL DEFAULT 1 CHECK (current_week BETWEEN 1 AND 105),
    week_started_on  DATE    NOT NULL,          -- R-6: the anchor every occurrence date derives from
    school_days      TEXT    NOT NULL DEFAULT 'MTWRF' CHECK (school_days GLOB '[MTWRFSU]*'),
    paused           INTEGER NOT NULL DEFAULT 0 CHECK (paused IN (0,1)),   -- W-14: summer / break
    started_on       DATE    NOT NULL,
    updated_at       TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP);

CREATE TABLE IF NOT EXISTS lesson_log (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    profile_id      INTEGER NOT NULL REFERENCES profiles(id)    ON DELETE CASCADE,
    subject_id      INTEGER NOT NULL REFERENCES subjects(id)    ON DELETE CASCADE,
    assignment_id   INTEGER          REFERENCES assignments(id) ON DELETE CASCADE,
    week            INTEGER NOT NULL,
    scheduled_date  DATE    NOT NULL,
    status          TEXT    NOT NULL DEFAULT 'done' CHECK (status IN ('done','skipped')),
    note            TEXT,
    completed_on    DATE    NOT NULL,
    completed_at    TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP);

CREATE TABLE IF NOT EXISTS lesson_extras (            -- H8: parent-added tasks on a date
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    profile_id      INTEGER NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    scheduled_date  DATE    NOT NULL,
    title           TEXT    NOT NULL CHECK (length(title) BETWEEN 1 AND 80),
    category        TEXT    NOT NULL DEFAULT 'daily' CHECK (category IN ('daily','reading','weekly')),
    text            TEXT,
    sort_order      INTEGER NOT NULL DEFAULT 0,
    status          TEXT    CHECK (status IS NULL OR status IN ('done','skipped')),
    note            TEXT,
    completed_on    DATE,
    completed_at    TIMESTAMP,
    created_at      TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP);
CREATE INDEX IF NOT EXISTS lesson_extras_day ON lesson_extras (profile_id, scheduled_date);

CREATE UNIQUE INDEX IF NOT EXISTS lesson_log_occurrence
    ON lesson_log (profile_id, week, subject_id, IFNULL(assignment_id, 0), scheduled_date);
CREATE INDEX IF NOT EXISTS lesson_log_week ON lesson_log (profile_id, week);
