# Homeschool ("house") tab — Plan v3 (v2 + owner's answers and additions of 2026-09-02)

**Status:** v3.1, 2026-09-02, awaiting the owner's single approval. v3 adds the owner's three additions
(§2 H8: Year view, Month view, parent-added tasks) and folds in the answers to §5; v3.1 applies the
delta review `reviews/DELTA_V3.md` (D-1…D-9). v1 findings and their disposition are
in `docs/homeschool/reviews/{RED,PURPLE,WHITE}_HS.md`; every High/Med finding is resolved below and
cross-referenced (R-n / P-n / W-n). Conventions inherited from `docs/PLAN.md` §4–§5: waves, disjoint
file ownership, two attempts then escalate a tier, never weaken an acceptance test, Boss merges and
pushes at wave boundaries, no mid-run questions.

---

## 0. Objective and non-negotiables

Add a sixth surface, **School** (house glyph 🏠), holding the boys' curriculum for the year, so that a
parent opening the phone in the morning sees **one list of exactly what to do today** — the family's
read-alouds once, each boy's own work under his name — and ticks things off as they happen. Parents
can also browse the whole year week by week (Mon–Fri), see the month, and add their own tasks
(copywork, reading, anything) to a boy's day. The TV shows each boy his own part of it, curriculum items
and parent-added tasks alike, tickable with the remote (shared read-alouds stay on the phone, W-16).

Inherited (from `docs/PLAN.md` §0):
- Rust whole stack; no new non-Rust code. Emoji glyphs only; local fonts only; no network at runtime.
- Server-local time; every mutation carries an explicit `date` (±1 day) and an idempotency key.
- The TV must render School for the active boy and let him tick his own items with the remote alone (D1).
- Enrollment, week pointer and plan edits are parent actions (session cookie, server-enforced).
- Every acceptance test is agent-executable on this PC; no screenshot review.

New:
- **N1 — Curriculum content is never committed** (R-1, W-20). The repo is public. AmblesideOnline's
  licence permits family use and forbids reposting its schedules. `docs/homeschool/curriculum/` is
  gitignored (done). This plan, the tests and the fixtures name **no** week→reading mapping; the
  AO-specific spread and test expectations live in gitignored files under that directory. A committed
  guard test asserts `git ls-files docs/homeschool/curriculum/` is empty and no tracked file under
  `src/`, `tests/`, `assets/` contains `Ambleside`.

---

## 1. What the source material is

Owner-supplied: a 36-week curriculum in three 12-week terms (optional exam weeks exist in the source and
are **not** modelled this phase — R-8, W-7). Four kinds of content:

| Kind | Count | Per-week text? | Cadence | Read aloud to all? |
| --- | --- | --- | --- | --- |
| **daily** work (math, copywork, phonics, recitation, language, physical activity, poetry) | 7 | poetry only (term book) | every school day | poetry yes; rest per boy |
| **reading** — assigned chapters/passages/stories from ~14 books | 14 | yes, "-----" = none | 1–2 days a week; "split the week's readings however works for your family" | yes |
| **weekly** work (art, picture study, geography concept, handicrafts, nature study, composer, folksong, hymn) | 8 | geography only (term concept) | once a week | yes |
| **free reads** | ~9/term | per term | none; reference list | — |

Every reading is followed by the child telling it back (narration). The app renders that as a prompt
on the row (W-5); it is not state.

---

## 2. Design decisions

### H1 — Data model (migration `0005_homeschool.sql`, append-only; P-13, R-6, R-16, W-2, W-13, W-14)

```sql
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
```

A `lesson_log` row is the occurrence's state (`done` or `skipped`); untick deletes it. Tick =
`INSERT … ON CONFLICT DO NOTHING`; untick = `DELETE … WHERE profile_id=?1 AND week=?2 AND subject_id=?3
AND IFNULL(assignment_id,0)=IFNULL(?4,0) AND scheduled_date=?5`. `connect_options` already sets
`foreign_keys = ON`; all tables are new, so no rebuild pattern is needed. A `lesson_extras` row carries
its own state (`status NULL` = to do), like `custom_tasks`; it is not an occurrence of the curriculum.

### H2 — Week pointer is manual and anchored (W-15, R-6, R-13, R-20)

`current_week` moves only when a parent taps **Finish week** / **Back a week**. Each move stamps
`week_started_on = today`. The week's calendar span is the 7 days from `week_started_on`; the
occurrence date for weekday `d` is the first date in that span whose weekday is `d`. The **last school
day** of the week is the last date in the span whose weekday ∈ `school_days`.

**Finish week** is offered (parent only) when every occurrence of the week is done or skipped, **or**
today ≥ the last school day. The Today footer nudges (never auto-advances): "Week 3 done — start week
4?" when complete; "You've been on week 3 for 15 days" once `today − week_started_on ≥ 14`.

`current_week > weeks` is the terminal **Year complete 🎉** state (not an error; Back returns to
`weeks`). `paused = 1` renders "School's out ⚽ — no school today" on every surface and hides every
occurrence without touching the log. No calendar-driven advance, no holiday configuration — that is
the failure mode of every homeschool app, and the source itself says to split weeks your own way.

### H3 — Occurrence rule (pure, shared, wasm-safe; R-7, P-11, W-12)

`src/shared/homeschool.rs`, no `chrono`:

1. `days` = `parse_days(assignment.days ∨ subject.days)` in fixed order `M T W R F S U`, deduplicated,
   intersected with `enrollment.school_days`. `parse_days` rejects any letter outside `MTWRFSU` or a repeat.
2. `rows` = this week's `assignments` for the subject in `ordinal` order.
3. `daily`: if `rows` is empty, one untitled occurrence per day in `days`; otherwise rows are spread per
   rule 5 (this is how "Math: lesson 14" per week works — W-6/Q3).
4. `weekly`: one occurrence on the **first** day of `days`, carrying `rows[0]` if any. Empty `days` → none.
5. `reading`: if `rows` or `days` is empty → none (checked first; no division). If
   `rows.len() ≤ days.len()`: chunk `days` into `rows.len()` contiguous groups (earlier groups take the
   extra day); one occurrence per (row, day-in-group); `part = Some((k, n))` when the group has `n > 1`
   days. Else one occurrence per row at `days[i % days.len()]`; `part = None`. Worked: 1 row × 2 days →
   part 1/2 Mon, part 2/2 Wed; 2 rows × 2 days → one each; 2 rows × 1 day → both Tuesday; 2 rows × 3
   days → row 0 on Mon+Wed (parts 1/2, 2/2), row 1 on Fri; 0 rows → none.
6. `free_read`: never an occurrence; `term_notes` of kind `free_read` render as a collapsed list.
7. Every occurrence gets `scheduled_date` from H2 and `shared = subject.shared`.
8. **Today view** for `(enrollment, date)`: `is_school_day = weekday(date) ∈ school_days ∧ !paused`;
   `due_today` = occurrences with `scheduled_date == date`; `catch_up` = occurrences with
   `scheduled_date < date` and no log row (**daily included** — R-13/P-11; they can be ticked or skipped);
   `done` = occurrences with a log row (`status` carried). Week complete ⇔ no occurrence lacks a log row.
9. `done` matches the index key exactly: `(profile, week, subject, IFNULL(assignment,0), scheduled_date)`.
10. **Extras** (H8) join the boy's lists by date, never Together, as `DayItem::Extra` (D-2; an extra is
    never a `LessonOccurrence`): `scheduled_date == date` → `due_today`; `date − 14 ≤ scheduled_date < date`
    with `status IS NULL` → `catch_up` (tagged with its date); status set → `done`. Extras count in
    `done_count`/`total_count` and in the Finish-week completeness check **only** when dated inside the
    current week's span, and drop out of `total_count` once the span has passed.

### H4 — Together and per-boy (W-1, W-2)

A **Together group** is every enrollment sharing `(curriculum_id, current_week)`. Occurrences of
`shared` subjects render **once** under **Together** with the boys it covers; ticking one fans out a
`lesson_log` row per boy in the group in one transaction (`toggle_lesson_together`). A Together row is
done when every covered boy has a row; partial shows "2 of 3". Non-shared subjects render under each
boy's name. Defaults: `reading`/`weekly` → shared, `daily` → not shared, poetry shared (the TOML says so).
With one boy enrolled, Together is simply his list.

### H5 — Content pipeline (N1, R-2, P-14, P-17, W-8)

- Dependency: **`toml` is not a dependency today** (`src/server/config.rs` deliberately hand-parses flat
  keys). Boss pre-A micro-commit adds `toml =1.1.5` (the current release on crates.io on 2026-09-02),
  `server` feature only, and records the pin in `docs/PLAN.md` §5.8.
- **File format** (`docs/homeschool/CURRICULUM_FORMAT.md`, committed; example uses invented books):
  `[curriculum] slug name weeks term_weeks? source_note?` · `[[subject]] name category source? days?
  shared? icon? sort_order?` · `[[assignment]] subject week ordinal? text detail? days?` ·
  `[[term_note]] term kind text sort_order?`. Rust types are `#[serde(deny_unknown_fields)]` with the
  defaults `term_weeks=12`, `days="MTWRF"`, `ordinal=1`, `shared` = (category ≠ daily). Validation,
  all-or-nothing, in order: slug `^[a-z0-9-]{1,64}$`; `1 ≤ weeks ≤ 104`; category/kind in set; days
  via `parse_days`; subject names unique; every `assignment.subject` resolves **by name**;
  `1 ≤ week ≤ weeks`; `(subject, week, ordinal)` unique; `1 ≤ term ≤ ceil(weeks/term_weeks)`.
  Errors carry the file name and `line N` from `toml::de::Error::span()`.
- **Loader** at boot scans `config.curricula_dir()` (env `FAMILY_HUB_CURRICULA_DIR`, default
  `<data>\curricula`, created if absent, absolute, logged at `info`): validate → insert **missing rows
  only** keyed on `(slug, subject name, week, ordinal)`. Parent edits survive. A bad file is logged and
  skipped; a good sibling still loads. The loader never deletes.
- `family-hub.exe import-curriculum <path> [--replace]`: validates and copies into the dir; `--replace`
  also **replaces** that slug's `subjects/assignments/term_notes` rows in one transaction (bulk fix path,
  W-8). Log rows survive a replace by name-matching subjects; rows whose subject vanished are deleted
  and counted in the output.
- Transcription is **HS2a/HS2b** (Sonnet; weeks 1–18 / 19–36), guided by the gitignored
  `curriculum/ao-year-1.notes.md`, output `curriculum/ao-year-1.toml` + `ao-year-1.expect.toml`, both
  gitignored, then copied into the data dir. The committed test loads expectations from the `.expect`
  file and **skips with a printed reason** when the files are absent.
- Tests everywhere else use the committed **synthetic** fixture `tests/fixtures/curricula/sample-year.toml`
  (P-15): 3 weeks, `term_weeks = 1`, slug `sample-year`, subjects `Sums` (daily MTWRF, not shared),
  `Copywork` (daily MTWRF), `Old Tales` (reading MW, 1 row/week — split), `Fables` (reading TF, 2 rows in
  weeks 1 and 3, **0 in week 2**), `Twice Told` (reading T, 2 rows in week 2 — both one day),
  `Painting` (weekly F), `Reading Basket` (free_read); three `term_note` rows, one per kind. Invented names only.

### H6 — Surfaces (W-1, W-3…W-6, W-9, W-10, W-16…W-18, R-15)

**Phone `/m`, tab 2 of 6:** `Routine · School · Calendar · Board · Remote · Settings` — "TV Remote" is
relabelled **Remote** so six columns fit at `text-xs` with no new phone type size; a
`mobile_tab_budget_px()` unit test proves the fit the way `tv_rail_budget_px()` does. **Today is the
tab** — no segmented control. Above the fold, in reading order:

1. Header chip: `Week 3 of 36 · Term 1 · 14 done · 2 skipped / 22` — tap opens **School settings**.
2. Nudge banner when applicable (H2), with the parent-only **Finish week →** button.
3. **Together** — shared occurrences due today (and their catch-up, tagged "from Mon" as a
   `bg-sheffield-accent` chip with `text-slate-800` ink — an existing pair), each row: category glyph →
   checkbox → subject (title register) → assignment text on its own line, with `(then tell it back)` on
   reading rows and `part 1 of 2` / `continue · 2 of 2` on splits.
4. One block per enrolled boy — his non-shared occurrences due today + his catch-up, with a **Mark all
   done** control on the block heading (undoable: ticks only the unticked; a second tap does nothing).
5. **This term** collapsed card (geography concept, poetry book, free reads).
6. Row actions (parent): tap the text → inline edit of `assignment.text` for that week (creates the
   row for a daily subject that has none — "Math: lesson 14"); long-press → **Skip** / **Note**.

Section headings reuse the phone's existing `h3 text-lg font-bold text-sheffield-dark` pattern. Boy
chips at the top **filter** to one boy (his Together items included); default is everyone.

**School settings** (sheet, parent only): enrollments (boy → curriculum → starting week → school days;
Unenroll keeps the log), **Pause school**, per-subject days + shared toggles. **Empty state** when
nobody is enrolled: poster card "No school plan yet" → **Enroll a boy**.

**Year and Month** (owner's additions; H8): the header chip's neighbour is a two-way toggle
`Year · Month` that swaps Today for one of:

- **Year view** — a week picker 1…36 (current week preselected, done weeks stamped) over a
  subject × Mon–Fri grid built by the same occurrence rule, so the parent sees the whole year exactly
  as it will be dealt out. Cells show the category glyph and the first ~18 characters; tapping a cell
  opens a sheet with the full text, `part k of n`, and (parent) inline edit of `assignment.text` /
  `days` for that week. Rows are ≥ 44 px tall; the grid scrolls inside its own container (W-9's
  concern, answered with a sheet instead of in-cell editing). Read-only for non-parents.
- **Month view** — a Mon–Fri month grid (weekends collapsed to a thin strip). Each school-day cell
  shows `done/total` for the boy on that date (past and current-week days, from the log + extras),
  a 📌 when parent-added tasks exist, and the week number on Mondays. Tapping a day opens the **day
  sheet**: that date's curriculum items (only when the date lies in the current week's span — future
  weeks are not dealt out until the parent finishes the previous one, and the sheet says so), the
  boy's extras, and the parent-only **Add task** form: boy · title (≤ 80 chars) · category (`daily`
  ✏️ / `reading` 📖 / `weekly` 🎨) · optional text. Extras can be edited, deleted, ticked or skipped
  from the same sheet.

A boy chip filters Year exactly as it filters Today. **Month view always shows exactly one boy**: with
more than one enrolled, the first enrolled boy is preselected and the chip is a required selector (D-4).
Past days show a bare done count (the plan for a past week is not reconstructed); only days inside the
current week's span show `done/total`. In Year view, only the current week is **dated and tickable**;
other weeks show weekday columns without dates and offer text/days editing only (D-5).

**TV `/tv`, panel 4 of 4** (`Routine · Calendar · Whiteboard · School`, Left/Right wraps): for the
active boy, **his own (non-shared) occurrences due today + his catch-up**, routine-row anatomy,
tickable; header `🏠 School · Week 3`; section labels in tracked caps (`TV_BODY_TEXT font-bold
tracking-[0.35em] uppercase text-slate-800`, no new size). States: not enrolled → "No school plan for
Simeon"; non-school day / paused → "No school today ⚽"; year complete → "Year complete 🎉"; all done →
the routine's 8/8-style celebration chip. Parent-added extras render in the same list, with a 📌
prefix on the glyph slot, and are tickable. Shared read-alouds are ticked on the phone by whoever holds
the book (W-16). Routine item 8 "Start your school work 📚" stays the doorway.

**Realtime:** `ServerMessage::HomeschoolUpdated { user_ids: Vec<i64>, week: i64, date: String }` after
any log/enrollment change (Together ticks name every boy), `ServerMessage::CurriculumUpdated
{ curriculum_id: i64 }` after edits/replace. New `bus.homeschool_version`. Offline queue gains
`QueuedMutation::ToggleLesson { user_id, subject_id, assignment_id: Option<i64>, week, scheduled_date,
completed }` (label "School lesson"); Together ticks are **not** queued offline (they need the group
membership the server holds) — they show a toast on failure. `docs/PWA.md` states that the queue's
48 h expiry is longer than the server's ±1 day `date` window, so a replay after ~30 h is rejected.

### H7 — Authorization and validation (R-9, R-10, R-11)

| Action | Who |
| --- | --- |
| read Today / settings | anyone on the LAN (TV is unauthenticated, as for Routine) |
| tick / untick / skip / note a single boy's occurrence or extra | anyone — mirrors `toggle_routine_task` |
| add / edit / delete an extra | parent session |
| Together tick, Finish/Back week, enroll/unenroll, pause, edit text, subject days/shared | parent session: `auth: String` → `require_session_or_cookie` |

Every tick is validated server-side against the recomputed week: `(subject_id, assignment_id,
scheduled_date)` must be one of `occurrences(week_plan(enrollment))` for that boy's `current_week`, or
the mutation is rejected. That single check bounds `scheduled_date`, requires enrollment, and caps
`lesson_log` growth. `days` strings pass `parse_days` in the loader **and** in both server fns that write
them.

### H8 — Year view, Month view, parent-added tasks (owner, 2026-09-02)

Owner's words: "The parents should be able to see all 36 weeks, Monday through Friday … see the month
and be able to add days, like assignment, copy work, reading, all that, to the child's tasks. They can
see it and they can check off those tasks." Resolved as: the **Year view** and **Month view** of H6,
and **extras** — parent-authored tasks pinned to a boy and a date, stored in `lesson_extras`, merged
into his Today/TV lists by rule 10, ticked by him on the TV or by a parent on the phone. Extras are
independent of the curriculum pointer, so a parent can plan ahead into any date. The white team's
W-8/W-9 cuts are reversed by the owner; their friction points are met with sheets and ≥ 44 px targets.

---

## 3. Tasks

Tiers: **H** Haiku · **S** Sonnet · **O** Opus · **B** Boss. Format: Owns / Do / Accept. Acceptance is the
contract; agents never weaken it. A migration-version or key-count bump in an existing test is a
mechanical edit, not a weakening (P-4); every other assertion in those files stays byte-identical.

**HS1 — Storage: migration + queries + loader + subcommand (tier O).**
Owns: `migrations/0005_homeschool.sql`; `src/server/homeschool/{mod,db,loader}.rs` (new);
`src/server/config.rs` (one **method** `curricula_dir()` + `ENV_CURRICULA_DIR`, no new field — R-4/P-1);
`src/server/health.rs` (one field `curricula: usize`); `src/server/service.rs` (`USAGE` + one `dispatch`
arm); `src/bin/family_hub.rs` (usage literal only); `tests/homeschool_db_tests.rs` (new);
`tests/fixtures/curricula/sample-year.toml` (new, per H5); `docs/homeschool/CURRICULUM_FORMAT.md`;
migration-version constants only in `tests/storage_tests.rs`, `tests/backup_tests.rs`; key count 8→9
and the `curricula` type assertion in `tests/health_tests.rs`, `tests/health_pool_closed_tests.rs`
(`curricula` is `0`, never absent, when the pool is closed); additions only in `tests/service_tests.rs`.
Handoff: one loader call in `db.rs::pools()` — Boss applies post-A.
Do: H1 verbatim; H5 types, validation, loader, `import-curriculum [--replace]`; queries
`week_plan(curriculum_id, week)`, `enrollment(profile_id)`, `together_group(curriculum_id, week)`,
`logs(profile_id, week)`, `set_occurrence(exec, …)` / `clear_occurrence(exec, …)` generic over
`SqliteExecutor` (share the `claim_mutation` transaction), `set_week`, `upsert_enrollment`,
`set_paused`, `unenroll`, `upsert_assignment`, `set_subject_schedule(subject_id, days, shared)`,
`count_curricula`; extras: `add_extra`, `update_extra`, `delete_extra`, `set_extra_status`,
`extras_between(profile_id, from, to)`; month: `log_counts_between(profile_id, from, to)`.
Accept: (a) `tests/fixtures/family_v1.db` migrates to version 5, `_sqlx_migrations` = `[1,2,3,4,5]`,
every `daily_routine_logs` row intact; a fresh DB reaches 5; `PRAGMA foreign_keys` = 1; (b) loading the
fixture twice yields identical row counts; editing an assignment then reloading keeps the edit;
`--replace` restores it and reports the count; (c) a TOML with `week > weeks`, unknown `category`/`kind`,
a day letter outside `MTWRFSU`, a duplicate `(subject, week, ordinal)`, or an `assignment.subject`
naming no subject is rejected with the file name and a `line N` substring, row counts unchanged;
(d) `set_occurrence` twice for one key = one row; `clear_occurrence` → zero; a `NULL assignment_id` key
dedupes (the daily case); the same subject/date in weeks 3 and 4 are two rows; (e) enrolling the same
boy twice replaces, never duplicates; deleting a profile cascades enrollment + log; (f)
`curricula_dir()` `is_absolute()`; a missing dir is created; `bad.toml` + `sample-year.toml` → exactly 1
curriculum loaded, `Ok`, path in an `info` line; (g) `import-curriculum` with a bad path or bad TOML
exits non-zero and writes nothing; (h) the N1 guard test (§0) passes; (i) an extra with an 81-char title
or an unknown category is rejected by the CHECK; `set_extra_status(None)` clears `completed_*`;
`extras_between` is inclusive on both ends and ordered by `(scheduled_date, sort_order, id)`; `add_extra`
assigns `sort_order = MAX(sort_order) + 1` within `(profile_id, scheduled_date)`; `update_extra` and
`set_extra_status` bump `updated_at`.

**HS2a / HS2b — Transcribe the year (tier S each; never H — Haiku fabricated transcripts in v2 QA).**
Owns: `docs/homeschool/curriculum/ao-year-1.toml`, `…/ao-year-1.expect.toml` (both gitignored);
HS2b also `tests/curriculum_tests.rs` (new). HS2a = weeks 1–18 (wave A), HS2b = weeks 19–36 (wave B),
appending to one file; acceptance runs once on HS2b.
Do: follow `curriculum/ao-year-1.notes.md` exactly; copy both files into the data dir.
Accept (test skips with a printed reason when the gitignored files are absent): (a) the `.expect`
counts hold — weeks, subject count, "every week has" list; (b) the chapter-sequence subject's chapters
`first…last` each appear exactly once in non-decreasing week order; (c) the two-ordinals rule holds
except in the listed weeks; (d) all six `[[spot]]` rows match by `contains`; (e) term-note counts hold;
(f) every subject's `days` ⊆ `MTWRF` and `shared` matches the notes table; (g) the loader accepts the
file; (h) no string from the TOML appears in any tracked file (guard).

**HS3 — Shared types + scheduling core + protocol + realtime bus (tier O; R-3, R-18, P-6, P-11, P-12).**
Owns: `src/shared/homeschool.rs` (new); `src/shared/types.rs` (append-only: DTOs below,
`MaximizedView::Homeschool` **last**, two `ServerMessage` variants after `Health`); `docs/PROTOCOL.md` §4
(two rows); `src/client/realtime.rs` (`homeschool_version` + the two bump arms in `RealtimeBus::apply`);
`tests/realtime_tests.rs` (the two `*_variant_name` matches, `every_server_message()`, sample vectors).
Do — signatures are normative:
```rust
pub enum Weekday { Mon, Tue, Wed, Thu, Fri, Sat, Sun }      // Copy, Eq, Hash, Ord, Serde
impl Weekday { pub const ORDER: [Weekday; 7]; pub fn letter(self) -> char;
    pub fn from_letter(c: char) -> Option<Self>; pub fn index(self) -> usize; }
pub enum Category { Daily, Reading, Weekly, FreeRead }        // as_str / parse
pub enum LogStatus { Done, Skipped }
pub fn weekday(date: &str) -> Option<Weekday>;                // Sakamoto on YYYY-MM-DD
pub fn parse_days(letters: &str) -> Result<Vec<Weekday>, DayError>;   // ordered, no repeats
pub fn add_days(date: &str, delta: i32) -> Option<String>;
pub fn date_for(week_started_on: &str, day: Weekday) -> Option<String>; // first match in the 7-day span
pub fn last_school_day(week_started_on: &str, school_days: &[Weekday]) -> Option<String>;
pub struct AssignmentRow { assignment_id: i64, ordinal: i64, text: String, detail: Option<String>, days: Option<Vec<Weekday>> }
pub struct SubjectPlan { subject_id, name, category, source, icon_name, sort_order, days: Vec<Weekday>, shared: bool, rows: Vec<AssignmentRow> }
pub struct WeekPlan { curriculum_id, week, weeks, term, subjects: Vec<SubjectPlan>, term_notes: Vec<TermNote> }
pub struct Enrollment { profile_id, curriculum_id, current_week, weeks, term_weeks, week_started_on: String, school_days: Vec<Weekday>, paused: bool }
pub struct LogRow { subject_id, assignment_id: Option<i64>, scheduled_date: String, status: LogStatus, note: Option<String> }
pub fn occurrences(plan: &WeekPlan, enr: &Enrollment) -> Vec<LessonOccurrence>;
pub fn today_view(plan: &WeekPlan, enr: &Enrollment, logs: &[LogRow], date: &str) -> BoyToday;
pub fn together_view(groups: &[(Enrollment, Vec<LogRow>)], plan: &WeekPlan, date: &str) -> Vec<TogetherOccurrence>;
pub fn week_grid(plan: &WeekPlan, enr: &Enrollment, logs: &[LogRow], anchor: &str, dated: bool) -> WeekGrid;
pub fn month_view(enr: Option<&Enrollment>, plan: Option<&WeekPlan>, logs: &[LogRow],
                  extras: &[ExtraTask], year: i32, month: u32, today: &str) -> MonthView;
pub fn merge_extras(today: &mut BoyToday, extras: &[ExtraTask], date: &str, week_span: (&str, &str));
```
DTOs (append to `types.rs`; `Clone, PartialEq, Debug, Serialize, Deserialize`; dates `YYYY-MM-DD`):
`LessonOccurrence { subject_id, assignment_id: Option<i64>, week, scheduled_date, weekday, category,
title, text: Option<String>, detail: Option<String>, source: Option<String>, icon_name: Option<String>,
part: Option<(u32,u32)>, shared: bool, sort_order, status: Option<LogStatus>, note: Option<String> }`;
`BoyToday { user_id, name, due_today: Vec<LessonOccurrence>, catch_up: Vec<…>, done: Vec<…>, done_count,
skipped_count, total_count }`; `TogetherOccurrence { occurrence: LessonOccurrence, user_ids: Vec<i64>,
done_user_ids: Vec<i64> }`; `HomeschoolTodayView { date, is_school_day, anyone_enrolled: bool,
groups: Vec<TogetherGroup { curriculum_id, curriculum_name, week, weeks, term, week_started_on,
paused, year_complete: bool, can_finish_week: bool, days_on_week: u32, together: Vec<TogetherOccurrence>,
boys: Vec<BoyToday>, term_notes: Vec<TermNote> }> }`; `EnrollmentView { user_id, enrolled, curriculum_id,
curriculum_name, current_week, weeks, week_started_on, school_days: String, paused }`;
`CurriculumSummary { id, slug, name, weeks, term_weeks, subject_count }`; `SubjectSetting { subject_id,
name, category, days: String, shared }`; `ExtraTask { id, user_id, scheduled_date, title, category,
text: Option<String>, sort_order, status: Option<LogStatus>, note: Option<String> }`;
`enum DayItem { Lesson(LessonOccurrence), Extra(ExtraTask) }` — `BoyToday`'s three lists are
`Vec<DayItem>` (D-2; `LessonOccurrence` never represents an extra, `TogetherOccurrence` is untouched);
`WeekGrid { week, weeks, term, dated: bool, days: Vec<Weekday>, rows: Vec<WeekGridRow { subject_id, title,
category, shared, cells: Vec<Vec<LessonOccurrence>> }> }` (`cells.len() == days.len()`; `free_read`
subjects have no row); `MonthView { year, month, user_id, days: Vec<MonthDay { date, weekday,
is_school_day, in_current_week, week: Option<i64>, done: u32, total: Option<u32>, extras: u32 }> }`
(`total` is `Some` only when `in_current_week`).
Accept: (a) `weekday`: `2026-09-02`→Wed, `2000-02-29`→Tue, `2100-03-01`→Mon (2100 is not leap),
`1970-01-01`→Thu; (b) H3 rule-5 worked cases as unit tests, plus `days` empty → none, `rows` empty →
none; a Monday reading unticked on Wednesday is in `catch_up` and not `due_today`; a daily occurrence
appears in `catch_up` on a later day and never twice in `due_today`; Saturday with `MTWRF` →
`is_school_day = false`, `due_today` empty, `catch_up` = all unfinished; a skipped row leaves
`catch_up` and counts in `skipped_count`; `paused` → everything empty; (c) `date_for` with
`week_started_on` a Wednesday puts Mon/Tue in the following week; `last_school_day` correct for
`MTWRF` and `MTWR`; (d) `every_server_message().len()` = previous + 2; protocol doc test green;
(e) `cargo clippy --features web --target wasm32-unknown-unknown -- -D warnings` exits 0 and
`grep -rn chrono src/shared/` is empty; (f) `parse_days("Th")`, `"MM"`, `"X"` → `Err`; (g) `week_grid`
on the fixture week 2 has **6** rows (free_read excluded) × 5 cells, `Fables` row all empty, `Twice Told`
both rows in the Tuesday cell; with `dated = false` every `scheduled_date` is advisory and the grid
reports `dated = false`; (h) `month_view` with `week_started_on = "2026-09-28"`, `year = 2026,
month = 9` returns 30 days; exactly days 28–30 have `in_current_week = true` (the span's other four
fall in October); a day before the span has `total = None` and `done` = its log rows; a day inside the
span has `total = Some(n)`, `n` = merged occurrences + extras dated that day; an extra dated
`2026-09-10` gives that day `extras = 1, total = None`; with `enr = None, plan = None` every day has
`total = None`, `week = None`, `is_school_day = false`, and `extras` still counted; (i) `merge_extras`
with `week_started_on = date − 2`: an extra dated today → `due_today`; dated `date − 3` and unfinished
→ `catch_up`; dated `date − 14` → `catch_up`; dated `date − 15` → nowhere; done → `done`; dated
`date + 9` → in no list and absent from `total_count`; dated `date + 1` → in no list but counted in
`total_count`.

**HS4 — Server functions (tier S, deps HS1 HS3; R-9, R-10, P-7).**
Owns: `src/server/api/homeschool.rs` (new); `src/server/api/mod.rs` (module-table row);
`tests/homeschool_tests.rs` (new, on the `routine_tests.rs` harness); `src/server/api/profiles.rs`
(**visibility only**: `require_session_or_cookie` is private today — make it `pub(crate)`); `docs/HANDOFF.md` entries.
Do — `#[server(endpoint = …)]` auto-registers `POST /api/<endpoint>`; **no edit to `router.rs`**;
`same_origin_or_absent` does not apply to server fns:
```rust
get_homeschool_today(date: String) -> HomeschoolTodayView          // all groups, all boys
get_enrollments() -> Vec<EnrollmentView>; list_curricula() -> Vec<CurriculumSummary>
get_subject_settings(curriculum_id: i64) -> Vec<SubjectSetting>
toggle_lesson(user_id, subject_id, assignment_id: Option<i64>, week, scheduled_date, completed: bool,
              status: LogStatus, note: Option<String>, date, idempotency_key) -> ()
toggle_lesson_together(curriculum_id, week, subject_id, assignment_id: Option<i64>, scheduled_date,
              completed: bool, date, idempotency_key, auth: String) -> ()
mark_all_done(user_id, week, date, idempotency_key) -> ()          // unticked due_today + catch_up only
set_school_week(user_id, week, date, auth) -> EnrollmentView        // stamps week_started_on = date
enroll(user_id, curriculum_id, week, school_days, date, auth) -> EnrollmentView
unenroll(user_id, auth) -> (); set_paused(user_id, paused: bool, auth) -> EnrollmentView
upsert_assignment(subject_id, week, ordinal, text, detail: Option<String>, auth) -> ()
set_subject_schedule(subject_id, days: String, shared: bool, auth) -> ()
get_week_grid(user_id, week) -> WeekGrid      // anchor = add_days(week_started_on, (week − current_week) × 7); dated = (week == current_week)
get_month(user_id, year: i32, month: u32) -> MonthView
add_extra(user_id, scheduled_date, title, category, text: Option<String>, date, idempotency_key, auth) -> ExtraTask
update_extra(extra_id, title, category, text: Option<String>, scheduled_date, auth) -> ()
delete_extra(extra_id, auth) -> ()
toggle_extra(extra_id, completed: bool, status: LogStatus, note: Option<String>, date, idempotency_key) -> ()
```
Every mutation: date window → `begin` → `claim_mutation` → validate against recomputed occurrences
(H7) → write → commit → `publish`.
Accept: (a) toggle with yesterday's `date` writes yesterday's row; 3 days ago rejected; (b) same
idempotency key twice = one change; (c) each `auth` fn without a cookie → error; with one → 200 and
`HomeschoolUpdated` / `CurriculumUpdated` on a second WS client within 1 s; (d) `toggle_lesson` without
a cookie succeeds; (e) a `scheduled_date` / triple not in the boy's current-week occurrences → rejected,
nothing written; an unenrolled boy → rejected; (f) `toggle_lesson_together` on the fixture with two boys
on week 2 and one on week 1 writes exactly two rows and names the two in `HomeschoolUpdated.user_ids`;
(g) `set_school_week` from `weeks` → `weeks + 1` (`year_complete = true`), Back returns to `weeks`;
`week_started_on` = the `date` passed; (h) `mark_all_done` ticks only unticked due/catch-up and is
idempotent; (i) `set_subject_schedule(days = "Th")` → error, nothing written; (j)
`get_homeschool_today` with nobody enrolled → `anyone_enrolled = false`, not an error; paused → empty
lists with `paused = true`; (k) `add_extra` without a cookie → error; with one → row returned and
`HomeschoolUpdated` names the boy; `scheduled_date` must parse as `YYYY-MM-DD` and lie within
`[today − 365, today + 365]`, else rejected with nothing written (the ±1 day window still applies to
the mutation's own `date`); `toggle_extra` without a cookie succeeds and honours the ±1 day `date`
window; `delete_extra` of a ticked extra removes it; (l) `get_week_grid`: `week != current_week` →
`dated = false`; `week = 0` and `week > weeks` error; an unenrolled `user_id` errors; `get_month`
fetches exactly `enrollment`, the current `week_plan` only if its span intersects the month,
`log_counts_between` and `extras_between` over the month — for an unenrolled boy it returns
extras-only days; (m) `toggle_lesson` with `subject_id <= 0` is rejected before any write.

**HS5 — Phone tab (tier O, deps HS3 HS4; R-14, R-15, W-*).**
Owns: `src/client/components/homeschool/{mod,today,row,settings,enroll,year,month,day_sheet}.rs` (new);
`src/client/components/mobile/mod.rs` (`MobileTab` + bar + budget fn); `mobile/queue.rs`
(`ToggleLesson` and `ToggleExtra { user_id, extra_id, completed }` + their `user_id()`/`label()` arms,
labels "School lesson" / "School task" — D-7); `mobile/remote.rs` (`VIEWS` → 5); `tests/glyph_tests.rs`;
`docs/PWA.md` (one paragraph). Uses only palette pairs already in `PALETTE_PAIRS` (R-12); if a new pair
is truly needed the task writes `docs/HANDOFF.md` and uses an existing pair meanwhile.
Do: H6 phone spec in full; six tabs `grid-cols-6`, `text-xs`, label `Remote`; existing five-tab test
becomes the six-tab test in the new order.
Accept: (a) SSR of the bar: 6 buttons in H6 order, `🏠` aria-hidden, label `School`, label `Remote`;
`mobile_tab_budget_px()` ≤ 60 for the widest label at the bar's font metrics table; (b) SSR of Today
for the fixture (two boys on week 2, one on week 1) inside a wrapper providing
`Signal::new(Some(SessionState::SignedOut))`: a `Together` heading, one `Old Tales` row with `part 1 of 2`,
a per-boy block per enrolled boy, `Week 2 of 3` chip, `(then tell it back)` on the reading row, and no
`Finish week` / `Mark all done` / edit affordance; the identical render under `SessionState::Parent`
has exactly one `Finish week` per complete group and one `Mark all done` per boy; (c) nobody enrolled →
`Enroll a boy`; paused → `No school today`; year complete → `Year complete`; (d) queue: a failed
`toggle_lesson` whose `scheduled_date` is four days before `date` enqueues and replays once on
reconnect, idempotent; a failed `toggle_extra` likewise enqueues, replays once and is idempotent; (e) `VIEWS` has Homeschool and sending it yields
`ClientMessage::SetView { view: Homeschool }`; (f) palette suite green; (g) catch-up chip uses
`bg-sheffield-accent` + `text-slate-800` and no `text-sheffield-accent` appears in the module;
(h) SSR of Year view for the fixture at week 2: a week picker with 3 entries, week 2 marked current,
6 subject rows (free_read excluded; DELTA_V3 D-5) plus the weekday header row × 5 day columns, the `Twice Told` Tuesday cell holding two entries; every grid row
carries `min-h-[44px]`; the grid sits in an `overflow-x-auto` container; (i) SSR of Month view for
September 2026 with `week_started_on = 2026-09-07` and one fixture extra dated `2026-09-10`: 30 day
cells, `📌` on `2026-09-10`, an `n/m` badge on `2026-09-08`, a bare count on `2026-09-01`, and a weekend
strip carrying `data-month-weekend`; (j) day sheet SSR under `SessionState::Parent` shows `Add task`
with the three category options; under `SignedOut` it does not; a future date outside the current
week's span shows exactly `Not dealt out yet — finish week 2 first.`, no curriculum rows, extras only;
(k) Year view for a non-current week renders no checkbox and no date in the column headers.

**HS6 — TV panel (tier O, deps HS3 HS4; R-19, P-9).**
Owns: `src/client/components/tv/{model,surface,shell,fixture}.rs`; `src/client/components/screensaver.rs`
(the hand-iterated view list); `tests/golden/tv_focus_order.txt` (one new section only); `tests/tv_tests.rs`
(additions + the view→slug list).
Do: `TvPanel::Homeschool` (`ALL` → 4, `body_lens: [usize; 4]`), `to_view/from_view` arms (replacing the
Boss placeholder), `surface::homeschool_panel` per H6 TV spec, shell resource keyed on
`homeschool_version`, `FocusId::Lesson(String)` keyed `s{subject}-a{assignment|0}-{date}` and `FocusId::Extra(i64)` keyed on
the extra's id (string/id keys, not indices — the list refetches on `homeschool_version` and a tick
reorders it; D-2), `dom_id()` → `tv-lesson-{key}` / `tv-extra-{id}`; dispatch to `toggle_lesson` /
`toggle_extra` (status `done`); tracked-caps section labels; the four state cards; fixture day with
**exactly 11 lesson rows and 1 extra** = 12 focusable body rows (D-3).
Accept: (a) `tv_focus_order.txt` gains exactly one section `[panel:homeschool]` after
`[panel:whiteboard]` and before `[overlay:join-qr]`; the four existing sections byte-identical; `t2_1_a`
and `t2_1_c` pass unchanged; (b) BFS over the pure key handler reaches every `FocusId::Lesson(_)` **and the `FocusId::Extra(_)`**
of the 12-row fixture from `TvState::initial()` in ≤ 12 presses (expected worst case 8: School is one
`Left` wrap from Routine); (c) `a_boy_can_tick_every_lesson_with_the_remote_alone`
pure-handler test; (d) injected `SetView(Homeschool)` renders `data-tv-panel="homeschool"`;
`SetActiveProfile` to an unenrolled boy renders the not-enrolled card; paused → `No school today`;
(e) type-scale golden unchanged; overscan walker, hover grep, rail-budget and palette tests green;
(f) Left/Right wrap over 4 panels unit test; (g) SSR of the panel contains no shared subject's row;
(h) the fixture's extra renders `📌`, is focusable as `FocusId::Extra(extra_id)`, and `Enter` yields
`TvAction::Activate(FocusId::Extra(extra_id))`, dispatched to `toggle_extra`.

**HS7 — Cross-surface loop + verification + docs (tier S, deps HS5 HS6; P-16, R-21).**
Owns: `tests/homeschool_loop_tests.rs` (new); `docs/VERIFICATION.md` (HS rows + transcripts);
`tests/docs_tests.rs` (id list gains HS1–HS7 **in the same commit** as the VERIFICATION rows; HS8
excluded as T3.5 is); `docs/OWNER_CHECKLIST.md` (one row); `docs/homeschool/README.md` (usage);
`docs/RESIDUAL.md`. No markdown link may point into the gitignored directory (the link checker walks
`docs/**`).
Accept: (a) two WS clients: phone-authed `set_school_week` reaches the TV client < 1 s; a TV-side
`toggle_lesson` reaches the phone < 1 s; a Together tick reaches both; (b) kill + restart → both
resync; (c) `docs_tests` green with HS ids, no `FAIL` row, every transcript `0 failed`; link checker
green; (d) `cargo test --features server` twice consecutively, both `0 failed`; clippy both targets
`-D warnings`; `dx build --platform web --release` exit 0; (e) `/health` reports `curricula: 1` when
the AO file is present in the data dir and `0` when not (skips with reason if absent).

**HS8 — Fresh-context Fable QA loop (Fable + original tiers), as `docs/PLAN.md` T3.5.**
Audits every HS task against its contract and the standard (correctness, Rust idiom, LAN security,
24/7 robustness, no weakened tests, no undeclared non-Rust), **design-direction compliance** (local
fonts, declared palette pairs, four-size TV scale, overscan, emoji-only, section-heading treatment),
and **N1** (the guard test plus a manual grep of the diff). Writes `docs/qa/QA_HS_ROUND_<n>.md`; loop to
PASS or 3 rounds; leftovers to `docs/RESIDUAL.md` with solutions.

### Waves, micro-commits, roster

`A: HS1 HS2a HS3` → `B: HS4 HS2b` → `C: HS5 HS6` → `D: HS7` → `E: HS8`.

Boss micro-commits (each a one-purpose commit, baseline re-run after):
- **pre-A:** pinned `toml` dep + `docs/PLAN.md` §5.8 row (R-2); `CURRICULUM_FORMAT.md` skeleton;
  the glyph set in `glyphs.rs` — `HOMESCHOOL_GLYPH 🏠`, `READING_GLYPH 📖`, `DAILY_WORK_GLYPH ✏️`,
  `WEEKLY_WORK_GLYPH 🎨`, `FREE_READ_GLYPH 📚`, `fn category_glyph(&str) -> &'static str` (unknown → ✅),
  `EXTRA_TASK_GLYPH 📌`, and `fn subject_glyph(icon_name: Option<&str>, category) -> &'static str` with
  specials `math 🔢`, `nature 🌿`, `music 🎵`, `bible 📜`, `poetry 🪶`, `body 🏃🏾` keyed by the subject's
  `icon` in the TOML (P-8, W-17). **Landed** in `glyphs.rs` before wave A.
- **post-A / pre-B:** `db.rs::pools()` loader call; placeholder `MaximizedView::Homeschool =>
  TvPanel::Routine` in `tv/model.rs::from_view` (HS6 replaces; P-5); the **Isaiah enrollment seed** (Q1 answered; D-6): a boot-time `seed_enrollments()` beside the loader
  that runs `INSERT … ON CONFLICT (profile_id) DO NOTHING` (never upsert — a reboot must not reset
  the week) for the profile found by `SELECT id FROM profiles WHERE name = 'Isaiah'` (exactly one row,
  else `warn` and skip), curriculum by slug `ao-year-1` (absent → `info` and skip), `current_week = 1`,
  `school_days = MTWRF`, `week_started_on = started_on = the run date` (no Monday arithmetic; `date_for`
  pushes Mon/Tue into the following week of the span — consistent with H2 and default 2). Unit-tested
  in `tests/homeschool_db_tests.rs`: second boot changes nothing; renamed profile → skipped.
- **pre-C:** nothing expected (glyphs already landed).

Roster: Opus HS1 HS3 HS5 HS6; Sonnet HS2a HS2b HS4 HS7; Haiku fmt sweeps only; Fable HS8 + Boss.
Scale: 9 build tasks + QA ≈ 12–18 agent invocations; 5–9 h wall clock unattended. Autonomy policy
`docs/PLAN.md` §5 applies unchanged (two attempts, escalate, BLOCKED.md, never weaken, branch per task).

---

## 4. Stated defaults (normative; P-19)

1. **Isaiah** is enrolled at week 1 by the Boss during the run; Nathaniel is enrolled from the phone
   next year (new enrollment row; the schema needs nothing new).
2. School days `MTWRF`; week pointer starts at 1; `week_started_on = started_on` at enrollment.
3. Letters `M T W R F S U` (`R` Thursday, `U` Sunday), always in that order. Homeschool weeks are
   **anchored on `week_started_on`**, not on Sunday like the calendar tab.
4. `term = (current_week − 1) / term_weeks + 1`.
5. `reading`/`weekly` subjects are shared, `daily` are per boy, unless the TOML or settings say otherwise.
6. Single-boy ticks are open to anyone on the LAN — a boy on the TV can tick an extra a parent created
   but cannot create, edit or delete one; Together ticks and everything else need the parent cookie.
7. Untick deletes the row; skip writes `status = 'skipped'`; both keep nothing else. No reports this phase.
8. Catch-up rolls forward only within the current week; Finish week leaves unfinished items unlogged.
9. `current_week > weeks` = Year complete; `paused` = School's out; both render on every surface.
10. Tab order `Routine · School · Calendar · Board · Remote · Settings`; TV panel order appends School.
11. `FAMILY_HUB_CURRICULA_DIR` default `<data>\curricula`; loader is boot-time, insert-only, never deletes;
    `import-curriculum --replace` is the bulk-edit path.
12. Free reads and term notes are read-only reference; the TV never renders them.
13. `completed_on` is the tick date; `scheduled_date` is the due date; only the latter is in the unique index.
14. Unenroll deletes the enrollment and keeps the log; deleting a profile cascades both.
15. The tab glyph is 🏠 because the owner asked for a house; the white team preferred 📚 to echo routine
    item 8 — a one-line flip if wanted.
16. Extras are per boy and per date, never Together; unfinished extras roll into catch-up for 14 days
    and are excluded from `total_count` once outside the current week's span.
17. Year view shows every week 1…36 regardless of the pointer, but only the current week is dated and
    tickable; Month view deals out curriculum items only inside the current week's span, extras anywhere,
    and always shows exactly one boy.
18. Both phones hold the parent cookie (owner confirmed); Finish week and Together ticks are parent-only.

## 5. Owner's answers (2026-09-02) — no open questions remain

- **Q1** Only Isaiah this year, from week 1; parents sign off week 1 → 2 with Finish week. Nathaniel
  next year via the phone enrollment flow. → default 1, the Boss seed micro-commit.
- **Q2** Not asked back; with one boy enrolled, Together is simply Isaiah's list and `shared` costs
  nothing. Default kept (shared read-alouds) so next year's second enrollment works without a change.
- **Q3** Monday–Friday, not a four-day week. → `MTWRF`.
- **Q4** Both phones are signed in as parent; only a parent can finish a week. → default 18.
- **Additions** "see all 36 weeks, Monday through Friday", "see the month", "add … copy work, reading,
  all that, to the child's tasks; they can see it and check off" → H8.

Questions from v1 now settled as defaults: boys tick their own items on the TV (yes); daily subjects
carry an optional per-week text and a per-tick note (yes, both).

## 6. Assumptions

- A1 Repo stays public; N1 stands regardless.
- A2 One curriculum this year; the schema is year-agnostic (next year = a new TOML, a new enrollment).
- A3 The chart's blank daily/weekly rows carry no per-week text; parents add it inline when they want.
- A4 `toml` lands as a Boss micro-commit before wave A (it is **not** a dependency today).

## 7. Residual risks

- R1 Six tabs at 360 px: mitigated by the `Remote` relabel and the pixel-budget test, not by a new size.
- R2 Transcription fidelity: structural counts, six spot checks and inline editing; `--replace` for bulk fixes.
- R3 Together fan-out when boys are on different weeks: they are simply different groups; the UI shows
  one Together section per group, labelled with the week.
- R4 `lesson_log` growth is bounded by server-side occurrence validation (H7).
- R5 A parent forgetting Finish week: the 14-day nudge; never auto-advance.
