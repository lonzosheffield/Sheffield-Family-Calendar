-- 0002_core — everything Phase 1 and Phase 2 need that v1 never had.
--
-- PLAN v2 D4 / PURPLE §P3 T1.1. Tables added here:
--   events              calendar events, local + Google (T2.4)
--   whiteboard_strokes  one row per *stroke* (never per segment — R-18a),
--                       ordered by `seq` within `board_id`, soft-cleared by
--                       the `cleared_at` watermark (T2.3, T1.6 compaction)
--   google_sync_state   per-calendar polling window + last poll outcome (T2.4, T1.7)
--   settings            flat key/value server settings (T1.4, T2.7)
--   mutation_log        idempotency keys for every client mutation (T1.5)
-- plus `custom_tasks.due_date` (T1.5 / T2.5).
--
-- `user_id` columns are deliberately *not* foreign keys yet: the `profiles`
-- table arrives in T1.4's `0003_profiles.sql`, which adds the FKs and drops
-- v1's two `CHECK (user_id BETWEEN 1 AND 4)` constraints.

-- Calendar events. Google events are stored expanded (one row per occurrence)
-- and replaced wholesale per polling window; local events keep `rrule` +
-- `tzid` and are expanded at read time by `rrule::all(limit)`.
CREATE TABLE IF NOT EXISTS events (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    source      TEXT    NOT NULL DEFAULT 'local' CHECK (source IN ('local', 'google')),
    external_id TEXT,
    calendar_id TEXT,
    title       TEXT    NOT NULL,
    description TEXT,
    location    TEXT,
    starts_at   TEXT    NOT NULL,
    ends_at     TEXT,
    all_day     INTEGER NOT NULL DEFAULT 0 CHECK (all_day IN (0, 1)),
    tzid        TEXT,
    rrule       TEXT,
    user_id     INTEGER,
    color       TEXT,
    created_at  TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at  TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_events_starts_at ON events (starts_at);

-- A Google event is identified by (calendar, remote id); a full-window
-- replace upserts on this key so a deleted remote event simply disappears.
CREATE UNIQUE INDEX IF NOT EXISTS idx_events_external
    ON events (source, calendar_id, external_id)
    WHERE external_id IS NOT NULL;

-- One row per stroke. `points` is a JSON array of normalised [x, y] pairs so
-- the board is resolution independent (the TV and a phone draw the same
-- stroke at different pixel sizes). `seq` is dense and monotonic per board and
-- is what `Snapshot` orders by. Clearing the board stamps `cleared_at` on
-- every live stroke rather than deleting rows, so a late joiner and the
-- clearing client agree on the watermark; T1.6's compaction hard-deletes
-- everything already stamped and keeps the last 2,000 live strokes.
CREATE TABLE IF NOT EXISTS whiteboard_strokes (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    board_id   INTEGER NOT NULL DEFAULT 1,
    seq        INTEGER NOT NULL,
    client_id  TEXT    NOT NULL DEFAULT '',
    color      TEXT    NOT NULL DEFAULT '#111111',
    width      REAL    NOT NULL DEFAULT 4.0,
    points     TEXT    NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    cleared_at TIMESTAMP,
    UNIQUE (board_id, seq)
);

CREATE INDEX IF NOT EXISTS idx_strokes_live
    ON whiteboard_strokes (board_id, seq)
    WHERE cleared_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_strokes_cleared
    ON whiteboard_strokes (cleared_at);

-- Windowed Google polling state (R-19: no syncToken; the window is replaced
-- in full on every poll, so deletions are handled by construction).
CREATE TABLE IF NOT EXISTS google_sync_state (
    calendar_id     TEXT PRIMARY KEY,
    window_start    TEXT,
    window_end      TEXT,
    last_polled_at  TIMESTAMP,
    last_success_at TIMESTAMP,
    last_error      TEXT
);

CREATE TABLE IF NOT EXISTS settings (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Idempotency ledger (T1.5). A client stamps every mutation with a key; the
-- server records it here inside the mutating transaction, so a replayed
-- offline queue produces exactly one effect.
CREATE TABLE IF NOT EXISTS mutation_log (
    idempotency_key TEXT PRIMARY KEY,
    kind            TEXT NOT NULL,
    user_id         INTEGER,
    payload         TEXT,
    result          TEXT,
    created_at      TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_mutation_log_created_at ON mutation_log (created_at);

-- Custom tasks gain an explicit due date (v1 tasks never expired — G12).
-- NULL keeps v1 rows visible exactly as before.
ALTER TABLE custom_tasks ADD COLUMN due_date DATE;
