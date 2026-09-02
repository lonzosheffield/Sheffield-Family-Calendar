# RED TEAM — `docs/homeschool/PLAN_HOMESCHOOL.md` v1

**Verdict: REWORK.** 21 findings. Two (R-2, R-3) stop wave A compiling; R-1 publishes licensed content
the plan's own N1 forbids; R-6 means no occurrence can be given a date at all. Cites verified on `main` @ `5ab348e`.

---

## High

### R-1 — The plan itself violates N1, and HS8's own gate would fail on it
**Hits:** §0 N1, §2 H4, §3 HS2 (b)(c)(d)(e), §3 HS8.
`git ls-files docs/homeschool/` is empty, so N1 holds *today*. Committing this plan ends that. §H4's table is
AO's weekly subject rotation, and HS2's acceptance publishes the week→reading mapping verbatim: six
exact-string spot checks (weeks 1, 7, 12, 19, 25, 36), "<book> chapters 1…27", "<book> has 2
ordinals in every week except 15, 36". That mapping *is* the schedule AO licenses for family use and forbids
reposting, and it lands again in `tests/curriculum_tests.rs`. HS8's own gate — "grep the diff for AO strings,
zero hits outside the ignored path" — therefore fails against the plan that commissioned it.
**Fix:** move H4 and every spot-check string into a gitignored `curriculum/ao-year-1.expect.toml`;
`tests/curriculum_tests.rs` loads expectations from it and skips with a printed reason when absent — the
mechanism HS2 already specifies. Add a committed guard: `git ls-files` returns nothing under
`docs/homeschool/curriculum/`, and no tracked file contains `AmblesideOnline`.

### R-2 — `toml` is **not** a dependency; A4 and the wave note are false
**Hits:** §2 H5, §3 waves ("`toml` is already a dependency via `FamilyHubConfig`"), §6 A4.
`grep -n toml Cargo.toml` → no match. `src/server/config.rs:18-25` says the opposite in as many words:
*"Rather than pull in a full TOML dependency ahead of when one is actually needed … request the `toml`
crate via `docs/HANDOFF.md`."* The parser it uses instead, `struct TomlValues(BTreeMap<String,String>)`
(`config.rs:253`), is flat `key = "value"` only — it cannot parse `[[assignment]]` arrays-of-tables, i.e.
the entire H5 schema. Wave A starts with HS1 unable to compile its loader.
**Fix:** Boss pre-A micro-commit adding a **pinned** `toml` (PLAN §5.8 pins every dep) + `serde` derive under
the `server` feature only, plus its version-pin row. HS1 owns the `Deserialize` structs. Do not let an agent
hand-roll a second TOML parser.

### R-3 — Two new `ServerMessage` variants break two exhaustive matches nobody owns
**Hits:** §3 HS3 (Owns, Accept c), wave A→C ordering.
`src/client/realtime.rs:307-361` matches `ServerMessage` **with no `_` arm**; `tests/realtime_tests.rs:1294-1311`
(`server_variant_name`) is likewise exhaustive. HS3 lands the variants in wave A and owns neither file
(`client/realtime.rs` is HS5's, two waves later) → `main` red at the wave-A gate → PLAN §5.5 whole-run halt.
HS3 (c) also mis-cites the test: the protocol doc test is in `tests/realtime_tests.rs`, not
`src/server/api/realtime.rs`, and it is driven by a hand-written `every_server_message()` vec (`:1338`), so it
passes **vacuously** if the variants are never added there.
**Fix:** HS3 Owns += `src/client/realtime.rs` (two bump arms + `homeschool_version`, settling R-18) and
`tests/realtime_tests.rs`. Add acceptance: `every_server_message().len() == 16`.

### R-4 — `FamilyHubConfig.curricula_dir` breaks 13 test binaries HS1 does not own
**Hits:** §3 HS1 Owns, Accept (f).
The struct is built by literal — every field named — at `tests/{calendar,font,health,health_pool_closed,http,photo,profiles,pwa,router,routine,screensaver,tls}_tests.rs` and `src/server/config.rs:435,505`
(e.g. `tests/routine_tests.rs:66-73`). A new field is a compile error in all of them.
**Fix:** add no field. Add a method beside `db_path()` returning
`env::var("FAMILY_HUB_CURRICULA_DIR")` else `self.data_dir.join("curricula")`. Absolute, loggable, zero
call-site churn, and acceptance (f) still holds.

### R-5 — `/health` is asserted to have **exactly 8 keys**
**Hits:** §2 H5, §3 HS2 (g), Owner Checklist row.
`tests/health_tests.rs:113-117`: *"/health must carry exactly 8 keys"*. Three separate acceptance criteria
require a ninth (`curricula`), and **no HS task owns `src/server/health.rs` or `tests/health_tests.rs`**.
**Fix:** HS1 Owns += both files; bump the count to 9 in the same commit (raising a count is not weakening a
test) and add the type assertion for the new key.

### R-6 — The week has no anchor, so no occurrence has a date — and daily rows collide across weeks
**Hits:** §2 H2, H3, H1 (`lesson_log` unique key).
H2 makes the pointer manual, so a "week" may span three days or three weeks. H3 nonetheless derives
occurrences by **weekday letter**, while `scheduled_date DATE NOT NULL`, `due_today` and `catch_up` ("an
earlier school day of the same week") all need real dates. Nothing in `enrollments` supplies the mapping:
`started_on` is the *enrollment* date, not the current week's.
Concrete: a parent taps **Finish week** on Monday afternoon. Daily subjects carry `assignment_id = NULL`, so
week 3's Monday Math and week 4's Monday Math share the key `(profile, subject, IFNULL(NULL,0), 2026-09-07)`.
Week 4 opens with Math already ticked; unticking it deletes week 3's history. Every "Monday" item of the new
week is also instantly `catch_up` "from Mon", for a Monday that already passed.
**Fix:** add `enrollments.week_started_on DATE NOT NULL`, stamped by `enroll` and every `set_week`. Occurrence
date = the first day on/after `week_started_on` whose weekday matches, never earlier. Put the week in the
index: `CREATE UNIQUE INDEX lesson_log_occurrence ON lesson_log(profile_id, week, subject_id, IFNULL(assignment_id,0), scheduled_date);`

### R-7 — The H3 occurrence formula divides by zero and duplicates readings
**Hits:** §2 H3, §3 HS3 Accept (b).
(a) `days[j % days.len()]` panics when `subject.days ∩ school_days` is empty — a Saturday-only subject under
`MTWRF`, or any bad `days` string (R-11). This is `src/shared/homeschool.rs`, compiled to wasm: the panic
aborts the render on the phone *and* the TV, not just the row.
(b) `rows = 2, days = 3` → `j ∈ 0..3` yields `(r0,d0), (r1,d1), (r0,d2)`: the same reading scheduled twice,
labelled "part 1 of 2" on both days, with two independent log rows. The formula matches H3's prose only when
`rows == days` or `rows == 1`; it conflates *split one reading over N days* with *spread N readings over N days*.
**Fix:** `if days.is_empty() { return Vec::new(); }`. Then if `rows.len() <= days.len()`, chunk `days` into
`rows.len()` contiguous groups, one occurrence per (row, day-in-group), `part i of n` from group size — 1×2 →
parts 1/2, 2×2 → one each, 2×3 → r0 on d0,d1 and r1 on d2. Otherwise one occurrence per row at
`days[i % days.len()]` (2 rows, 1 day → both Tuesday). Unit-test `rows=2,days=3` and `days=0`.

### R-8 — Exam weeks are unimplementable against the H1 schema
**Hits:** §2 H1/H2, §3 HS4 Accept (e).
H2's exam week is "a free-text checklist the parent types", but nothing stores those items: `assignments`
requires `subject_id` and a `week ≤ curricula.weeks` (HS1 (c) rejects `week > weeks`), and
`lesson_log.subject_id` is a NOT NULL FK. HS4 (e) tests only the pointer, so the hole ships untested.
**Fix:** cut exam weeks this phase — keep `exam_weeks_enabled`/`in_exam_week` as reserved columns, toggle
hidden — or add `exam_items (id, enrollment_id FK, week, ordinal, text)` plus a nullable
`lesson_log.exam_item_id`, with acceptance that a typed item survives a restart.

---

## Med

### R-9 — Unauthenticated, unvalidated `scheduled_date` on a kids' LAN
**Hits:** §2 H7, §3 HS4 (a)(d), §7 R4.
`toggle_lesson` is deliberately cookie-free (the TV path) and reachable by any device on plain HTTP :8080.
`date` is bounded by `db::date_within_window` (`src/server/db.rs:623`), but `scheduled_date` is a *separate*
free parameter that must point backwards for catch-up — and nothing bounds it, checks the triple is a real
occurrence, or checks the boy is enrolled. Unbounded `lesson_log` growth; §7 R4's "≤ 40 rows/boy/week" is
enforced by nothing.
**Fix:** in `set_occurrence`, recompute the week's occurrences from `week_plan(enrollment)` server-side and
reject any `(subject_id, assignment_id, scheduled_date)` outside that set — one check bounds the date,
enforces enrollment, and kills the write amplification.

### R-10 — HS4's privileged signatures have nowhere to put the session
**Hits:** §2 H7, §3 HS4 Accept (c).
Every existing privileged fn takes `auth: String` and calls `require_session_or_cookie`
(`src/server/api/profiles.rs:44-61`): bearer token, falling back to the `fh_session` cookie. As written,
`set_school_week`, `enroll`, `unenroll`, `update_assignment`, `set_subject_days` have no `auth` parameter, so
a phone holding its session in `localStorage` (HANDOFF H-19) cannot authenticate — acceptance (c) would be
satisfied by an endpoint no real client can call.
**Fix:** append `auth: String` to all five; route through `require_session_or_cookie`.

### R-11 — `days` is validated only in the loader, not in the mutations that set it
**Hits:** §3 HS1 (c), HS4.
HS1 (c) rejects bad day letters in TOML; `update_assignment(.., days)` and `set_subject_days` have no
validation case at all, so "Th" typed in the Plan view is written straight to the column and then panics
R-7(a) on every client.
**Fix:** `parse_days(&str) -> Result<DaySet, DayError>` in `src/shared/homeschool.rs` (HS3), called by the
loader *and* both server fns; `CHECK(days GLOB '[MTWRFSU]*')` in 0005; HS4 acceptance: `days = "Th"` → error,
nothing written.

### R-12 — Nobody owns `palette.rs`, yet HS5 must satisfy the palette suite
**Hits:** §3 HS5 (f)(g), HS6 (e).
`tests/palette_tests.rs:431-454` scans **all** of `components/**` for colour utilities (widened by QA Q1-15),
and `t3_4_a_every_pair_the_kiosk_actually_paints_is_in_the_table` walks the rendered kiosk markup. Catch-up,
"part n of m" and done treatments will want a pair; `src/client/components/palette.rs` is in no task's Owns.
**Fix:** add `palette.rs` to HS5's Owns (HS6 reuses), or state normatively that School reuses only existing
pairs and prove it with the same suite.

### R-13 — Catch-up plus "daily never rolls forward" makes a week un-finishable
**Hits:** §2 H2/H3, §4 default 7.
H3: daily occurrences are never in `catch_up`. H2: Finish week appears "when every occurrence of the week is
done, or on the last school day". One sick Tuesday leaves six daily occurrences permanently un-tickable, the
footer chip stuck at 16/22 — and "the last school day" of a manually-pointed week is undefined (R-6).
**Fix:** with `week_started_on` in place, define the last school day explicitly; then either count only
reachable occurrences in the chip denominator, or show missed daily items in a "Missed" group that is not
counted. Test: tick nothing Tuesday, assert Friday still offers Finish week.

### R-14 — The queued lesson mutation cannot round-trip a catch-up tick
**Hits:** §2 H6 (Realtime), §3 HS5 (d).
`QueuedMutation` (`mobile/queue.rs:56-86`) carries only the body; the day lives on `QueuedMutationEntry.date`
(`:94`). A lesson tick needs **two** dates — `date` (today, ±1 day) and `scheduled_date` (days back for
catch-up). The plan names only the variant; `user_id()` and `label()` are exhaustive matches needing arms.
**Fix:** `ToggleLesson { user_id, subject_id, assignment_id: Option<i64>, scheduled_date, completed }`, label
"School lesson"; acceptance: replay one whose `scheduled_date` is four days before `date`. Note too that the
queue's 48 h expiry outlives the server's ±1 day `date` window — a replay at 30 h is already rejected; write
that into `docs/PWA.md` rather than discovering it.

### R-15 — Six tabs: the mitigation invents a size the design system freezes
**Hits:** §2 H6, §7 R1, §3 HS5 (a).
`DESIGN_DIRECTION.md` §2.1: *"Phone scale also unchanged."* R1 proposes `text-[11px]`, a new phone size. The
bar today is `grid-cols-5 … px-1 py-2 text-xs` (`mobile/mod.rs:220-225`); at 360 px, six columns give 60 px,
52 px usable, and "TV Remote" in Nunito ExtraBold 11 px is ~56 px. The acceptance counts buttons and
characters, neither of which proves fit.
**Fix:** keep `text-xs`, rename the tab "Remote", and assert a pixel budget the way
`tv::style::tv_rail_budget_px()` does (§2.7) — a `mobile_tab_budget_px()` unit test — not a character count.

---

## Low

- **R-16 — the `lesson_log` unique key is invalid SQLite as written.** An `IFNULL(...)` expression cannot be
  an inline table constraint, only a `CREATE UNIQUE INDEX`; and a plain `UNIQUE(...)` over the nullable
  `assignment_id` would not dedupe at all (SQLite treats NULLs as distinct) — precisely the daily-subject
  case. Emit it as a separate statement (R-6); `set_occurrence` becomes `INSERT … ON CONFLICT DO NOTHING`.
- **R-17 — HS3 (a) names dates but not answers**, so the agent writes both sides. Pin: `2026-09-02` Wed,
  `2000-02-29` Tue, `2100-03-01` Mon, `1970-01-01` Thu. 2100 is *not* a leap year — the case that catches a
  naive `y % 4`.
- **R-18 — the pre-wave-C micro-commit is one symbol short.** It pre-commits `HOMESCHOOL_GLYPH` but not
  `bus.homeschool_version`, which HS6's shell resource keys on and HS5 adds in the same wave
  (`tv/shell.rs:151-155`). Fold it into HS3 per R-3.
- **R-19 — `MaximizedView::Homeschool` reaches two unowned, non-compiler-checked sites:**
  `screensaver.rs:310-316` (variants iterated by hand) and `tests/tv_tests.rs:388-392` (view→panel slug).
  Both pass vacuously with the variant missing, so the phone-remote path ships untested. Add to HS6's Owns.
- **R-20 — `current_week = 37` has no view and no bound.** No `CHECK`, `week_plan(37)` returns nothing, the
  Plan picker is 1–36, so the only exit is *Back a week*. Add `CHECK(current_week BETWEEN 1 AND 60)` and an
  explicit `year_complete` arm in `HomeschoolTodayView`, tested.
- **R-21 — HS7's docs test is stricter than implied.** `tests/docs_tests.rs:792-802` hard-codes the PLAN v2
  id vec; adding HS ids is fine, but the same test forbids any `| FAIL |` row *and* requires every quoted
  `test result:` line to contain "0 failed" (`:819-841`). A blocked HS task goes to `docs/BLOCKED.md`.

---

**Gate:** R-1…R-8 are approval blockers. R-2, R-3, R-4, R-5 must land as Boss micro-commits *before* wave A
opens, or wave A ends with `main` red and the run halts under PLAN §5.5.
