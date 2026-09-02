# PURPLE TEAM — `PLAN_HOMESCHOOL.md` v1 → executable

**Date:** 2026-09-02 · **Input:** `docs/homeschool/PLAN_HOMESCHOOL.md` v1 · **Grounding:** `docs/PLAN.md` §3–§5, `docs/reviews/PURPLE_TEAM.md` §P3–§P5, `DESIGN_DIRECTION.md` §4, live tree.

**Verdict:** sound design, **not runnable**. Four ownership holes red the tree mid-run (P-1…P-4), one wave defect breaks compilation between waves A and B (P-5), H3 panics and contradicts itself (P-11), the migration block is pseudo-SQL (P-13). All fixable inside existing conventions; no design change.

---

**P-1 · HS1 Owns · `config.rs`.** `FamilyHubConfig` has no `Default` and is built with exhaustive struct literals in `config.rs` ×2 and **11 test binaries** (`calendar_`, `font_`, `health_`, `health_pool_closed_`, `http_`, `photo_`, `profiles_`, `pwa_`, `router_`, `routine_`, `screensaver_`, `tls_tests`). A `curricula_dir` **field** reds all of them. Replacement — add no field, add a method beside `db_path()`:

```rust
pub const ENV_CURRICULA_DIR: &str = "FAMILY_HUB_CURRICULA_DIR";
/// Absolute dir curriculum TOML is loaded from: env, else `<data_dir>\curricula`.
pub fn curricula_dir(&self) -> PathBuf {
    std::env::var(ENV_CURRICULA_DIR).map(PathBuf::from)
        .unwrap_or_else(|_| self.data_dir.join("curricula"))
}
```

**P-2 · HS1 Do · `import-curriculum` has no owned home.** Subcommands live in `src/server/service.rs::dispatch()` (T3.1's file), the `USAGE` const at `service.rs:65`, and a **duplicate** usage literal at `src/bin/family_hub.rs:40`; `tests/service_tests.rs` exercises `dispatch`. None is owned. Replacement — HS1 Owns gains `src/server/service.rs` (`USAGE` + one arm only), `src/bin/family_hub.rs` (literal only), `tests/service_tests.rs` (additions only):

```rust
const USAGE: &str = "usage: family-hub.exe <install|uninstall|start|stop|status|run|tv-probe|import-curriculum <path>>";
Some("import-curriculum") => run_and_report(cmd_import_curriculum(args.get(1))),
```

`cmd_import_curriculum` itself lives in HS1's own `homeschool/loader.rs` and returns `Result<String, ServiceError>`.

**P-3 · HS1(f)/HS2(g) · `/health` `curricula`.** `HealthBody` (`health.rs:49`) is T1.7's, and `health_tests.rs:114` asserts `object.len() == 8` **exactly**. Replacement — HS1 Owns gains `src/server/health.rs` (one field `pub curricula: usize`), `tests/health_tests.rs` and `tests/health_pool_closed_tests.rs` (key count 8→9; `curricula` is `0`, never absent, when the pool is closed).

**P-4 · §2 H1 · migration 0005 breaks four pinned assertions.** `storage_tests.rs:114` `Some(4)`, `:184` `vec![1,2,3,4]`, `:189` `Some(4)`; `backup_tests.rs:281` `Some(4)`; `health_tests.rs:183` `json!(4)` → all become 5 / `[1,2,3,4,5]`. Replacement — HS1 Owns gains those three files, **migration-version constants only**; add to §5: *"a migration-version bump is a mechanical edit, not a weakened test (`PLAN.md` §5.2); every other assertion in those files stays byte-identical."*

**P-5 · §3 Waves · wave A reds the tree.** HS3 (wave A) adds `MaximizedView::Homeschool` + two `ServerMessage` variants. Two exhaustive matches with **no `_` arm** sit in files HS5/HS6 do not land until wave C: `src/client/realtime.rs:307-361` (`RealtimeBus::apply`) and `src/client/components/tv/model.rs:114-123` (`TvPanel::from_view`). Waves B and D would not compile. Replacement — new Boss micro-commit, **post-A / pre-B**:

```rust
// realtime.rs — HS5 replaces with homeschool_version bumps
ServerMessage::HomeschoolUpdated { .. } | ServerMessage::CurriculumUpdated { .. } => {}
// tv/model.rs — HS6 replaces with TvPanel::Homeschool
MaximizedView::Homeschool => TvPanel::Routine,
```

`remote.rs`'s `VIEWS: [(MaximizedView, &str); 4]` is a sized array, not a match — HS5 widens it to 5; no break.

**P-6 · HS3 Owns names the wrong file.** There is no exhaustive-match test in `src/server/api/realtime.rs`. The gate is `tests/realtime_tests.rs`: `client_variant_name`/`server_variant_name` (1282–1305) and `t1_2_protocol_doc_names_every_message_variant` (1391). Replacement — swap `src/server/api/realtime.rs` for `tests/realtime_tests.rs` (**those two matches and their sample vectors only**).

**P-7 · HS4 · signatures omit the session parameter.** Every privileged fn in this tree takes `auth: String` and calls `api::profiles::require_session_or_cookie(&auth)` (empty ⇒ `fh_session` cookie fallback). Without it, Accept (c) is untestable in-process. Replacement — all `#[server(endpoint = "…")] -> Result<T, ServerFnError>`:

```rust
get_homeschool_today(user_id: i64, date: String)                      -> HomeschoolTodayView
get_homeschool_week(user_id: i64, week: i64)                          -> WeekPlanView
toggle_lesson(user_id: i64, subject_id: i64, assignment_id: Option<i64>,
              scheduled_date: String, completed: bool,
              date: String, idempotency_key: String)                  -> ()
set_school_week(user_id: i64, week: i64, in_exam: bool, auth: String)  -> EnrollmentView
enroll(user_id: i64, curriculum_id: i64, week: i64, school_days: String,
       exam_weeks: bool, auth: String)                                -> EnrollmentView
unenroll(user_id: i64, auth: String)                                  -> ()
update_assignment(assignment_id: i64, text: String, detail: Option<String>,
                  days: Option<String>, auth: String)                 -> ()
set_subject_days(subject_id: i64, days: String, auth: String)         -> ()
list_curricula()                                                      -> Vec<CurriculumSummary>
```

State explicitly so nobody edits `router.rs`: **`#[server(endpoint = "x")]` auto-registers `POST /api/x`; HS4 adds no route**, and the `same_origin_or_absent` gate belongs only to the hand-rolled `/api/{setup,login,logout,session}` handlers.

**P-8 · wave C · `glyphs.rs` claimed by HS5 while HS6 needs it.** Pre-committing only `HOMESCHOOL_GLYPH` is half a fix. Replacement — drop `glyphs.rs` from HS5's Owns; Boss pre-commits the whole set:

```rust
pub const HOMESCHOOL_GLYPH: &str = "🏠"; pub const READING_GLYPH: &str = "📖";
pub const DAILY_WORK_GLYPH: &str = "✏️"; pub const WEEKLY_WORK_GLYPH: &str = "🎨";
pub const FREE_READ_GLYPH: &str = "📚";
pub fn category_glyph(category: &str) -> &'static str; // unknown -> "✅"
```

**P-9 · HS6(a) · the golden block has a required position.** `tv_tests.rs::golden_models()` builds sections from `TvPanel::ALL` then the overlay and compares names pairwise with a length assert — "new block present" is insufficient. Replacement: *"`tests/golden/tv_focus_order.txt` gains exactly one section, `[panel:homeschool]`, **after `[panel:whiteboard]` and before `[overlay:join-qr]`**; the four existing sections are byte-identical; `t2_1_a` and `t2_1_c` pass unchanged."* HS6(b)'s "12 rows in ≤ 12 presses" holds exactly (Enter + 11 Down) with zero slack, so pin it: *"the homeschool fixture day has exactly 12 focusable lesson rows."*

**P-10 · vague Accept clauses, rewritten.**

| Clause | Gap | Replacement |
| --- | --- | --- |
| HS1(a) | fixture unnamed | "`tests/fixtures/family_v1.db` migrates to version 5, `_sqlx_migrations` = `[1,2,3,4,5]`, every `daily_routine_logs` row intact; a fresh DB reaches 5; `PRAGMA foreign_keys` stays `1`." |
| HS1(c) | "line-numbered" — `toml` gives a `Span` | "…rejected; the error `Display` carries the file name and a `line N` substring derived from `toml::de::Error::span()`; row counts unchanged after the failed load." |
| HS1(f) | "logged" is prose | "`config.curricula_dir()` `is_absolute()`; a missing dir is created at boot; a dir holding `bad.toml` (invalid) + `sample-year.toml` loads exactly 1 curriculum and returns `Ok`; the resolved path appears in an `info` line." |
| HS2(g) | cross-wave dep on HS1's loader inside wave A | move to HS7 (see amended table). |
| HS3(d) | "clippy clean" isn't an HS assertion | "`cargo clippy --target wasm32-unknown-unknown --all-targets -- -D warnings` exits 0 **and** `grep -rn 'chrono' src/shared/` returns nothing." |
| HS5(b) | `session::is_parent()` reads `try_consume_context`; SSR needs a provider | see amended table. |
| HS5(g) | duplicates (f) | delete. |
| HS7(d) | unnamed commands | "`cargo test --features server` twice consecutively, both `0 failed`; clippy both targets `-D warnings`; `dx build --platform web --release` exit 0." |

**P-11 · §2 H3 is ambiguous and panics.** (i) `rows[j % rows.len()]` divides by zero on the "-----" week; (ii) intersection order undefined (string vs weekday order); (iii) `part k of n` unspecified when `rows.len() > 1 && days.len() > rows.len()`; (iv) **contradiction** — "daily occurrences never roll forward" vs HS3(b) "Saturday → `catch_up` = all unfinished". Replacement, normative:

1. `days` is materialised in fixed order `M,T,W,R,F,S,U`, deduplicated, then intersected with `school_days`.
2. If `rows.is_empty() || days.is_empty()` → zero occurrences (checked **before** the loop).
3. `n = max(rows.len(), days.len())`; `for j in 0..n`: `(rows[j % rows.len()], days[j % days.len()])`. `part` is computed **after** the loop by grouping on `assignment_id`: size 1 → `None`; size `m > 1` → `Some((k + 1, m))` in day order.
4. `weekly` → the **first** element of the ordered day list; empty → no occurrence.
5. `done` iff a log row matches `(profile_id, subject_id, assignment_id, scheduled_date)` exactly — the index key. Delete "any date".
6. `catch_up` = not-done occurrences on a **strictly earlier school day of the current week**, `daily` **included**. Delete "Daily occurrences never roll forward"; amend HS3(b)'s daily case to *"a daily occurrence appears in `catch_up` on a later day and never twice in `due_today`."*

**`src/shared/homeschool.rs` — exact signatures** (no `chrono`, wasm-safe):

```rust
pub enum Weekday { Mon, Tue, Wed, Thu, Fri, Sat, Sun }   // Copy, Eq, Hash, Serde
impl Weekday { pub const ORDER: [Weekday; 7]; pub fn letter(self) -> char;
    pub fn from_letter(c: char) -> Option<Self>; pub fn index(self) -> usize; }
pub enum Category { Daily, Reading, Weekly, FreeRead }    // as_str / parse
pub fn weekday(date: &str) -> Option<Weekday>;            // Sakamoto on YYYY-MM-DD
pub fn parse_days(letters: &str) -> Option<Vec<Weekday>>; // ordered, deduped
pub fn add_days(date: &str, delta: i32) -> String;
pub fn date_on(week_monday: &str, day: Weekday) -> String;

pub struct AssignmentRow { pub assignment_id: i64, pub ordinal: i64, pub text: String,
    pub detail: Option<String>, pub days: Option<Vec<Weekday>> }
pub struct SubjectPlan { pub subject_id: i64, pub name: String, pub category: Category,
    pub source: Option<String>, pub icon_name: Option<String>, pub sort_order: i64,
    pub days: Vec<Weekday>, pub rows: Vec<AssignmentRow> }
pub struct WeekPlan { pub curriculum_id: i64, pub week: i64, pub term: i64,
    pub week_monday: String, pub subjects: Vec<SubjectPlan>, pub term_notes: Vec<TermNote> }
pub struct Enrollment { pub profile_id: i64, pub curriculum_id: i64, pub current_week: i64,
    pub weeks: i64, pub term_weeks: i64, pub in_exam_week: bool,
    pub exam_weeks_enabled: bool, pub school_days: Vec<Weekday> }
pub struct LogRow { pub subject_id: i64, pub assignment_id: Option<i64>, pub scheduled_date: String }

pub fn occurrences(plan: &WeekPlan, enr: &Enrollment) -> Vec<LessonOccurrence>;
pub fn today_view(plan: &WeekPlan, enr: &Enrollment, logs: &[LogRow], date: &str) -> HomeschoolTodayView;
pub fn week_view(plan: &WeekPlan, enr: &Enrollment, logs: &[LogRow]) -> WeekPlanView;
```

**P-12 · HS3 · exact DTO shapes** (appended to `src/shared/types.rs`; `Clone, PartialEq, Debug, Serialize, Deserialize`; dates `YYYY-MM-DD`):

```rust
pub struct LessonOccurrence { pub subject_id: i64, pub assignment_id: Option<i64>,
    pub scheduled_date: String, pub weekday: Weekday, pub category: Category,
    pub title: String, pub text: Option<String>, pub detail: Option<String>,
    pub source: Option<String>, pub icon_name: Option<String>,
    pub part: Option<(u32, u32)>, pub sort_order: i64, pub done: bool }
pub struct HomeschoolTodayView { pub enrolled: bool, pub user_id: i64, pub date: String,
    pub week: i64, pub weeks: i64, pub term: i64, pub in_exam_week: bool,
    pub is_school_day: bool, pub can_finish_week: bool,
    pub due_today: Vec<LessonOccurrence>, pub catch_up: Vec<LessonOccurrence>,
    pub done: Vec<LessonOccurrence>, pub free_reads: Vec<String>,
    pub done_count: u32, pub total_count: u32 }
pub struct WeekPlanRow { pub subject_id: i64, pub title: String, pub category: Category,
    pub cells: Vec<Vec<LessonOccurrence>> }            // cells.len() == days.len()
pub struct WeekPlanView { pub user_id: i64, pub week: i64, pub weeks: i64, pub term: i64,
    pub days: Vec<Weekday>, pub rows: Vec<WeekPlanRow> }
pub struct EnrollmentView { pub user_id: i64, pub enrolled: bool, pub curriculum_id: Option<i64>,
    pub curriculum_name: Option<String>, pub current_week: i64, pub weeks: i64,
    pub in_exam_week: bool, pub exam_weeks_enabled: bool, pub school_days: String,
    pub started_on: Option<String> }
pub struct CurriculumSummary { pub id: i64, pub slug: String, pub name: String,
    pub weeks: i64, pub term_weeks: i64, pub subject_count: i64 }
```

`MaximizedView::Homeschool` appends **last** (serde tag `homeschool`); the two `ServerMessage` variants append after `Health`.

**P-13 · §2 H1 is pseudo-SQL.** SQLite has no `UNIQUE(expr)` **table constraint** — `IFNULL(assignment_id, 0)` must be a standalone `CREATE UNIQUE INDEX` (expression indexes are legal, `IFNULL` deterministic, `0` never collides with an `AUTOINCREMENT` id). `FK` and `id FK UNIQUE` are not syntax. `source_note`/`created_at`/`updated_at` have no types; `sort_order` is nullable. Pragmas need no change (`db::connect_options` already sets `foreign_keys = ON`; all tables are new, so 0003's rebuild pattern is unnecessary). Normative shape:

```sql
CREATE TABLE IF NOT EXISTS enrollments (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    profile_id         INTEGER NOT NULL UNIQUE REFERENCES profiles(id)  ON DELETE CASCADE,
    curriculum_id      INTEGER NOT NULL REFERENCES curricula(id)        ON DELETE CASCADE,
    current_week       INTEGER NOT NULL DEFAULT 1,
    in_exam_week       INTEGER NOT NULL DEFAULT 0 CHECK (in_exam_week IN (0,1)),
    exam_weeks_enabled INTEGER NOT NULL DEFAULT 0 CHECK (exam_weeks_enabled IN (0,1)),
    school_days        TEXT    NOT NULL DEFAULT 'MTWRF',
    started_on         DATE,
    updated_at         TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP);
CREATE TABLE IF NOT EXISTS lesson_log (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    profile_id     INTEGER NOT NULL REFERENCES profiles(id)    ON DELETE CASCADE,
    subject_id     INTEGER NOT NULL REFERENCES subjects(id)    ON DELETE CASCADE,
    assignment_id  INTEGER          REFERENCES assignments(id) ON DELETE CASCADE,
    week           INTEGER NOT NULL, scheduled_date DATE NOT NULL,
    completed_on   DATE    NOT NULL,
    completed_at   TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP);
CREATE UNIQUE INDEX IF NOT EXISTS lesson_log_occurrence
    ON lesson_log (profile_id, subject_id, IFNULL(assignment_id, 0), scheduled_date);
CREATE INDEX IF NOT EXISTS lesson_log_week ON lesson_log (profile_id, week);
```

Also: `subjects.sort_order INTEGER NOT NULL DEFAULT 0`, `subjects.days TEXT NOT NULL DEFAULT 'MTWRF'`, `curricula.source_note TEXT`, `curricula.created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP`. Tick = `INSERT OR IGNORE`; untick = `DELETE … WHERE profile_id=?1 AND subject_id=?2 AND IFNULL(assignment_id,0)=IFNULL(?3,0) AND scheduled_date=?4`.

**P-14 · §2 H5 · the TOML schema needs a type.** Optionality, defaults and the subject↔assignment join key are all undefined. Replacement — normative in `CURRICULUM_FORMAT.md` and in the loader, all `#[derive(Deserialize)] #[serde(deny_unknown_fields)]`:

```rust
struct CurriculumFile { curriculum: CurriculumHeader,
    #[serde(default)] subject: Vec<SubjectEntry>,
    #[serde(default)] assignment: Vec<AssignmentEntry>,
    #[serde(default)] term_note: Vec<TermNoteEntry> }
struct CurriculumHeader { slug: String, name: String, weeks: i64,
    #[serde(default = "default_term_weeks")] term_weeks: i64,      // 12
    #[serde(default)] source_note: Option<String> }
struct SubjectEntry { name: String, category: String,
    #[serde(default)] source: Option<String>,
    #[serde(default = "default_days")] days: String,               // "MTWRF"
    #[serde(default)] icon: Option<String>, #[serde(default)] sort_order: Option<i64> }
struct AssignmentEntry { subject: String, week: i64,
    #[serde(default = "default_ordinal")] ordinal: i64,            // 1
    text: String, #[serde(default)] detail: Option<String>,
    #[serde(default)] days: Option<String> }
struct TermNoteEntry { term: i64, kind: String, text: String,
    #[serde(default)] sort_order: Option<i64> }
```

Validation, in order, all-or-nothing: `slug` matches `^[a-z0-9-]{1,64}$`; `weeks ∈ 1..=104`; `term_weeks ≥ 1`; `category ∈ {daily,reading,weekly,free_read}`; `kind ∈ {geography,free_read,poetry}`; `days` letters ∈ `MTWRFSU`, no repeats; subject names unique; every `assignment.subject` resolves **by name within the file** (never by index); `1 ≤ week ≤ weeks`; `(subject, week, ordinal)` unique; `1 ≤ term ≤ ceil(weeks / term_weeks)`. Absent `sort_order` = file order.

**P-15 · fixtures named but unspecified.** `tests/fixtures/curricula/sample-year.toml` is load-bearing for HS1, HS3, HS4, HS5, HS6, HS7 — six agents would build six fixtures. Pin it: *"3 weeks, `term_weeks = 1`, slug `sample-year`, name `Sample Year`. Subjects: `Sums` (daily `MTWRF`), `Copywork` (daily `MTWRF`), `Old Tales` (reading `MW`, 1 row/week — the split case), `Fables` (reading `TF`, 2 rows in weeks 1 and 3, **0 rows in week 2** — the "-----" case), `Twice Told` (reading `T`, 2 rows in week 2 — both-on-one-day), `Painting` (weekly `F`), `Reading Basket` (free_read). Three `term_note` rows, one per kind. Every book name invented; no AmblesideOnline string appears."* Also pin HS6's 12-row TV day (P-9) and HS5's SSR enrollment (`week = 2`, `weeks = 3`, `Old Tales` split present, `Fables` absent), so `part 1 of 2` and the `Week 2 of 3` chip are deterministic.

**P-16 · HS7 · `docs_tests` will *not* break mid-run.** `t3_3_every_task_id_appears_exactly_once_in_verification` (`docs_tests.rs:793`) uses a **hardcoded `Vec` of 27 ids**, not a parse of `PLAN.md` — that is exactly how v2 avoided this — and it never reads `PLAN_HOMESCHOOL.md`. HS ids in the plan are inert. The real requirement is the converse. Replacement (HS7 Accept c): *"`docs_tests.rs`'s id list gains `HS1…HS7` and `docs/VERIFICATION.md` gains their rows **in the same commit**; `HS8` is excluded exactly as `T3.5` is; no row is `FAIL`; `t3_2_every_internal_doc_link_resolves` still passes with `docs/homeschool/**` in scope."* Note: the link checker walks **all** `docs/**/*.md`, so no markdown link may point into the gitignored `docs/homeschool/curriculum/`.

**P-17 · HS2 · cannot seed, and the timebox is wrong.** Q1 says HS2 seeds enrollments, but HS2 owns only a gitignored TOML and one test file; seeding needs a DB write and migrations are Boss-numbered/append-only. Replacement: *"Q1 answered ⇒ **Boss** applies the enrollment as a post-wave-A micro-commit calling `homeschool::db::upsert_enrollment` — not a migration, not HS2."* Timebox: ~36 weeks × ~14 subjects ≈ 500 assignment rows against Sonnet's 90-min stop-loss (`PLAN.md` §5.3). Replacement: *"split into **HS2a** (weeks 1–18, wave A) and **HS2b** (weeks 19–36, wave B), same tier, appending to the same gitignored file; acceptance runs once, on HS2b."*

**P-18 · wave residue.** `toml` and `serde` derive are already dependencies — delete the `Cargo.toml` micro-commit line so no agent touches it. The `db.rs::pools()` loader call must land **post-A/pre-B** (HS4's tests need loaded rows), not merely "post-A". Add the P-5 placeholder commit.

**P-19 · implicit defaults, made explicit** (append to §4 as 10–20). 10. `FAMILY_HUB_CURRICULA_DIR`, default `<data>\curricula`, created if absent, absolute, logged at `info`. 11. Letters `M T W R F S U`; `R` = Thursday, `U` = Sunday; ordering is always this order. 12. Homeschool weeks are **Monday-first** for the Week view and `catch_up`, even though the Calendar tab starts Sunday (§P5.5 default 14 governs the calendar only). 13. `term = ((current_week - 1) / term_weeks) + 1`. 14. `current_week > weeks` is the terminal "Year complete" state, not an error; Back returns to `weeks`. 15. Unenroll deletes the `enrollments` row and keeps `lesson_log`. 16. Deleting a profile cascades to enrollment and log. 17. `toggle_lesson` is unauthenticated (Q2 default); every other homeschool mutation needs the parent session, server-checked. 18. The loader never deletes a curriculum; a TOML removed from the dir leaves its rows. 19. Free reads are `term_note` rows of kind `free_read`; the TV never renders them. 20. `completed_on` = server-local date of the tick; `scheduled_date` = date due; they differ on a catch-up tick and only `scheduled_date` is in the unique index.

---

## Amended task table (changed lines, verbatim)

**HS1 Owns:** `migrations/0005_homeschool.sql`, `src/server/homeschool/{mod,db,loader}.rs` (new), `src/server/config.rs` (one **method** `curricula_dir()`, no new field), `src/server/health.rs` (one field `curricula`), `src/server/service.rs` (`USAGE` + one `dispatch` arm only), `src/bin/family_hub.rs` (usage literal only), `tests/homeschool_db_tests.rs` (new), `tests/fixtures/curricula/sample-year.toml` (new, synthetic, per P-15), `docs/homeschool/CURRICULUM_FORMAT.md` (new); **migration-version constants and key-count only** in `tests/storage_tests.rs`, `tests/backup_tests.rs`, `tests/health_tests.rs`, `tests/health_pool_closed_tests.rs`; additions only in `tests/service_tests.rs`.

**HS1 Accept (a):** `tests/fixtures/family_v1.db` migrates to version 5 with `_sqlx_migrations` = `[1,2,3,4,5]` and every `daily_routine_logs` row intact; a fresh DB reaches version 5; `PRAGMA foreign_keys` stays `1`.

**HS1 Accept (c):** a TOML with `week > weeks`, an unknown `category` or `kind`, a day letter outside `MTWRFSU`, a duplicate `(subject, week, ordinal)`, or an `assignment.subject` naming no declared subject is rejected; the error `Display` carries the file name and a `line N` substring from `toml::de::Error::span()`; row counts are unchanged after the failed load.

**HS1 Accept (f):** `config.curricula_dir()` `is_absolute()`; a missing dir is created at boot; a dir holding `bad.toml` (invalid) plus `sample-year.toml` loads exactly 1 curriculum and returns `Ok`; the resolved path appears in an `info` log line.

**HS2 → HS2a (wave A, weeks 1–18) / HS2b (wave B, weeks 19–36)** per P-17; acceptance runs once, on HS2b. **HS2 Accept (g) moves to HS7 Accept (e):** "`/health` reports `curricula: 1` when the AO file is present in the data dir and `0` when it is not; the test skips with a printed reason if the gitignored file is absent."

**HS3 Owns:** `src/shared/homeschool.rs` (new), `src/shared/types.rs` (append-only: DTOs, `MaximizedView::Homeschool`, two `ServerMessage` variants), `docs/PROTOCOL.md` §4 (two rows), **`tests/realtime_tests.rs`** (the two `*_variant_name` matches and their sample vectors only). *(`src/server/api/realtime.rs` removed — P-6.)*

**HS3 Accept (d):** `cargo clippy --target wasm32-unknown-unknown --all-targets -- -D warnings` exits 0 **and** `grep -rn "chrono" src/shared/` returns nothing.

**HS4 Do:** the nine signatures of P-7 verbatim; every privileged fn calls `require_session_or_cookie(&auth)` before touching the database; **no route is added to `src/server/router.rs`** — `#[server(endpoint = …)]` registers `POST /api/<endpoint>`, and `same_origin_or_absent` is not applied to server fns.

**HS5 Owns:** `src/client/components/homeschool/{mod,today,week,plan,enroll}.rs` (new), `mobile/mod.rs` (`MobileTab` + bar), `mobile/queue.rs` (`ToggleLesson`), `mobile/remote.rs` (`VIEWS` row), `src/client/realtime.rs` (`homeschool_version`), `tests/glyph_tests.rs`. *(`glyphs.rs` removed — Boss pre-commits the set per P-8.)*

**HS5 Accept (b):** SSR of Today for the P-15 enrollment fixture, rendered inside a wrapper that `provide_context(Signal::new(Some(SessionState::SignedOut)))`, shows the section headings, `part 1 of 2` on the `Old Tales` split reading, the `Week 2 of 3` chip, and contains no `Finish week`; the identical render under `Some(SessionState::Parent)` contains exactly one `Finish week`.

**HS5 Accept (g):** *deleted* (subsumed by (f)).

**HS6 Accept (a):** `tests/golden/tv_focus_order.txt` gains exactly one section, `[panel:homeschool]`, positioned after `[panel:whiteboard]` and before `[overlay:join-qr]`; the four existing sections are byte-identical; `t2_1_a` and `t2_1_c` pass unchanged.

**HS6 Accept (b):** with a homeschool fixture day of exactly 12 focusable lesson rows, a BFS over the pure key handler reaches every `FocusId::Lesson(i)` from `TvState::initial()` in ≤ 12 presses.

**HS7 Accept (c):** per P-16, verbatim.

**Waves and micro-commits — replacement paragraph:** `A: HS1 HS2a HS3` → `B: HS4 HS2b` → `C: HS5 HS6` → `D: HS7` → `E: HS8`. Boss micro-commits: `CURRICULUM_FORMAT.md` skeleton (pre-A); **post-A/pre-B — the two placeholder match arms of P-5, the `db.rs::pools()` loader call, and the Q1 enrollment seed if answered**; the glyph set of P-8 (pre-C). `Cargo.toml` needs no change.
