VERDICT: FAIL

# QA — Homeschool ("School") wave, round 4

**Auditor:** Fable 5 (HS8, fresh context, no prior knowledge of the run) · **Date:** 2026-09-03 ·
**Tree:** `main` @ `fcef441`, audited as the full diff `abb202b..HEAD` (84 files, +21,223 / −116) with every
changed Rust, SQL, config and doc file read in full · **Contract:** `docs/homeschool/PLAN_HOMESCHOOL.md` v3.1
§0 (N1), §2 H1–H8, §3 HS1–HS7 Owns/Do/Accept including the QH3-04 amendment of HS4 "Do", §4 defaults;
reviews `RED_HS`, `PURPLE_HS`, `WHITE_HS`, `DELTA_V3`; `docs/PLAN.md` §5; `docs/design/DESIGN_DIRECTION.md`
§2–§3; `docs/PROTOCOL.md`; `docs/VERIFICATION.md`; `docs/BLOCKED.md`; `docs/HANDOFF.md` (the round 3 close,
HS5-qa3b, the round 4 merge, HS9); `docs/BACKLOG.md`; `docs/RESIDUAL.md`; `docs/qa/QA_HS_ROUND_3.md` (every
QH3 item re-verified on `main` by reading the code and running its test, never taken on trust).

Verdict is **FAIL** on one Med finding (QH4-01): H3 rule 10 says a parent-added task dated inside the current
week's span counts "in the Finish-week completeness check", and nothing implements that half of the rule —
`get_homeschool_today` computes `can_finish_week` from occurrences and log rows alone, so the phone nudges
"Week 2 done — start week 3?" and offers **Finish week** while the header chip beside it still reads
`n done / n+1` for the unticked task. Everything else holds: every round 3 finding is genuinely fixed on
`main` (not merely claimed), HS9's guard rail is real and was exercised with the data directory unset, both
full runs are green and identical, both clippy gates and `cargo fmt --check` exit 0, the committed
`assets/tailwind.css` is byte-identical to a fresh rebuild with the pinned binary, and N1 holds on the
tracked tree. Two Low findings are recorded for completeness; neither affects the verdict.

## What was run on this machine (all exit 0 unless stated)

Shell preamble for every command: `PATH` (cargo/scoop/npm), `RUST_BACKTRACE=1`,
`FAMILY_HUB_DATA_DIR=%TEMP%\familyhub-test`, `FAMILY_HUB_REFUSE_SYSTEM_DIR=1`. Stale `%TEMP%\familyhub-*`
directories were wiped before each full run (`familyhub-test` kept); the two full runs were consecutive,
never concurrent; `C:\ProgramData\FamilyHub` was never opened, listed or resolved by anything run here.

| Gate | Result |
| --- | --- |
| `cargo test --features server` (run 1) | 33 `test result:` lines, **628 passed, 0 failed**, 2 ignored (the two pre-existing `#[ignore]` fixture regenerators) |
| `cargo test --features server` (run 2, consecutive) | 33 `test result:` lines, **628 passed, 0 failed**, 2 ignored — identical to run 1 (HS7 (d)) |
| `cargo clippy --features server --all-targets -- -D warnings` | exit 0 |
| `cargo clippy --features web --target wasm32-unknown-unknown -- -D warnings` | exit 0 (HS3 (e) as amended by QH1-09) |
| `cargo fmt --check` | exit 0 |
| `git ls-files docs/homeschool/curriculum/` | empty |
| `git grep -il ambleside` | `.gitignore`, the plan, three of the four reviews, `docs/HANDOFF.md`, `docs/VERIFICATION.md`, the three earlier QA reports — **nothing under `src/`, `tests/`, `assets/`** |
| Every `name` / `source` / `text` / `detail` / `source_note` value ≥ 6 chars from the gitignored `ao-year-1.toml` (414 needles) grepped over every tracked text file | 10 needles hit, all generic plan vocabulary, structural subject labels or review citations (the §1 category words in `today.rs` / `shared/homeschool.rs` / `tv/fixture.rs` / `calendar.rs`, the `chapter N` doc example in `tests/curriculum_tests.rs`, two quotations in `WHITE_HS.md`, one subject label quoted in round 3's screenshot note). **No passage, book title or week→reading string is tracked anywhere.** `curriculum_tests::no_assignment_text_from_the_toml_appears_in_any_tracked_file` (every assignment text ≥ 15 chars against every tracked file) ran green in both full runs with the gitignored file present |
| `Cargo.toml` / `Cargo.lock` / `.github/workflows/ci.yml` / `input.css` / `tailwind.config.js` since `abb202b` | untouched; `toml = "=1.1.5"` server-only (PLAN §5.8) |
| Non-Rust added since `abb202b` | two JPEG photographs under `docs/` (one owner TV photo for BACKLOG B-1, one verification screenshot); no scripts, no new binaries |
| `tailwindcss.exe` v3.4.17 `-i input.css -o <scratch> --minify` vs the committed `assets/tailwind.css` | **identical** — 20,991 B both, SHA-256 `BE12B54A…E42AC` both; `.grid-cols-6{` present; `git status` clean after the rebuild (QH3-01 closed for real) |
| `ci_tests::every_tailwind_utility_named_under_components_has_a_rule_in_the_committed_css` | green, in both full runs and on its own |

### Accept clauses executed by hand (beyond the two full runs)

1. **HS1 (g)** — the real `target\debug\family-hub.exe import-curriculum` with `FAMILY_HUB_DATA_DIR` at a scratch directory: a missing path → `I/O error … cannot find the file`, exit 1, no `curricula\`, no `family.db`; a `days = "Th"` file → `bad.toml: line 6: subject "Stories" days "Th": 'h' is not one of the day letters MTWRFSU (R = Thursday, U = Sunday)`, exit 1, nothing copied, no `family.db`; the committed fixture → `7 subjects, 9 assignments, 3 term notes inserted`, copied, exit 0; a second import of the same file → `0 subjects, 0 assignments, 0 term notes inserted`; `--bogus` → the `IMPORT_USAGE` line, exit 2. Every run printed `data directory: <scratch>` first (HS9).
2. **HS9 canary (BACKLOG B-3 acceptance)** — with `FAMILY_HUB_DATA_DIR` **unset** and `FAMILY_HUB_REFUSE_SYSTEM_DIR=1`: `import-curriculum <fixture>` exits 1 with the `refusing to use the system data directory C:\ProgramData\FamilyHub because FAMILY_HUB_REFUSE_SYSTEM_DIR=1 is set …` line and prints no `data directory:` line (refused inside `try_load`, before any read, copy or pool). Then, in the same unset environment, `cargo test --features server --lib every_integration` (`1 passed`) and `--test whiteboard_tests --test loop_tests --test config_tests --test service_tests --test health_tests --test homeschool_db_tests --test homeschool_loop_tests --test realtime_tests --test routine_tests`: every binary green — with the refuse flag on, a harness that failed to pin its own data directory would have panicked with the refusal rather than opened the live one, so a green run **is** the canary. The nine-binary set includes the three suites HANDOFF H-HS9-1 found drawing on the family's live whiteboard.
3. **HS7 (e)** — `family-hub.exe run` on `127.0.0.1:8181` / `:8182` against a fresh scratch data directory whose `curricula\` held only the gitignored `ao-year-1.toml`: `/health` → `"migration_version":5,"curricula":1`; `familyhub.log` in order: `resolved data directory` → `scanning the curricula directory` → `loaded a curriculum file … curriculum_id=1` → `finished loading curricula loaded=1 skipped=0` → `enrollment seed: enrolled Isaiah at week 1 profile_id=1 curriculum_id=1 week_started_on="2026-09-03"`; `POST /api/get_enrollments` → profile 1 enrolled at week 1 of 36, `MTWRF`, profiles 2–4 `enrolled: false`; `POST /api/get_homeschool_today` for today → one group, week 1 of 36, `can_finish_week: false`, 4 Together items, 10 due today for the boy. Process stopped; the live `FamilyHub` service and its ports were never touched.
4. **HS7 (a)(b)** — `--test homeschool_loop_tests`: TV-side `toggle_lesson` → phone < 1 s, phone-authed `set_school_week` → TV < 1 s, Together tick → both, kill + restart → both reconnect inside the backoff budget and every pre-restart write survives. `1 passed`.
5. **HS5 (b)** SSR — `--test glyph_tests hs5_`: `26 passed`, including Today for the fixture under `SignedOut` (one `Together`, one `Old Tales` with `part 1 of 2`, `(then tell it back)`, `Week 2 of 3`, three boy blocks, no `Finish week` / `Mark all done` / `Edit text` / `Back a week`) and under `Parent` (exactly one `Finish week`, three `Mark all done`), (a) the six-tab bar and its 60 px budget, (c) the three state cards with QH3-03's `Back a week`, (d) both queued School toggles replaying once, (e) `VIEWS`, (g) the palette pair, (h)(k) Year, (i) Month, (j) the day sheet, and the QH3-02/04/05 guards.
6. **HS6 (a)–(i)** — `--test tv_tests hs6_`: `10 passed` (golden section placement with the four earlier sections byte-identical, BFS ≤ 12 presses with worst case ≤ 8, tick-every-row-with-the-remote, `SetView(Homeschool)` + the four state cards + loading, type-scale/overscan/tracked-caps label, 4-panel wrap, no shared row, the 📌 extra activatable through `toggle_extra`, QH1-04's celebrate-over-rows).
7. **HS3 (a)(b)(c)(f)(g)(h)(i)** — `--lib hs3_`: `34 passed`; **HS3 (d)** `--test realtime_tests hs3_`: `1 passed` (16 = 14 + 2).
8. **HS4 (a)–(n)** — `--test homeschool_tests`: `18 passed`, the new `hs4_i_upsert_assignment_rejects_a_bad_days_string_and_writes_nothing` (QH3-04's server half) included.
9. **HS1 (a)–(f)(h)(i)** — `--test homeschool_db_tests`: `26 passed`, the N1 guard and the QH3-02 `assignment_detail` assertion included. **HS2 (a)–(h)** — `--test curriculum_tests` with the gitignored files present: `9 passed`.

## Round-3 findings re-verified on `main`

| Item | Status | How verified |
| --- | --- | --- |
| QH3-01 | **FIXED** | `342c0a6` rebuilt `assets/tailwind.css`; a fresh rebuild here with the pinned 3.4.17 binary is byte-identical (SHA-256 match, 20,991 B); `.grid-cols-6{`, `.min-h-\[44px\]`, `.overflow-x-auto`, `.min-w-\[38rem\]`, `.max-h-\[85vh\]` all present; the prescribed guard `ci_tests::every_tailwind_utility_named_under_components_has_a_rule_in_the_committed_css` is on `main` (`tests/ci_tests.rs:160-234`) and green. |
| QH3-02 | **FIXED** | `SchoolAction::EditAssignment` carries `detail` and `days` (`homeschool/mod.rs:136-145`); the dispatcher passes both through to `upsert_assignment` (`:550-572`); `today.rs:437,479` / `:665,708` and `year.rs:364,400,432` pass `detail: detail.clone()`; `homeschool_db_tests::a_parent_edit_survives_a_reload_and_replace_restores_the_file_text` now asserts `assignment_detail == Some("stop at the bridge")` after the edit; guard `hs5_qa3_an_inline_text_edit_carries_the_rows_detail_and_days_through` green. |
| QH3-03 | **FIXED** | `today.rs:280-309`: the `year_complete` arm renders the card **and**, for a parent, `Back a week` dispatching `SetWeek { week: group.weeks }`; `hs5_c_a_paused_group_says_school_is_out_and_a_finished_year_celebrates` asserts exactly one `>Back a week<` in the finished group's slice and none signed out. |
| QH3-04 | **FIXED** | Contract amended (`PLAN_HOMESCHOOL.md:475`, HANDOFF round 3 close); server `upsert_assignment(…, days: Option<String>, auth)` runs `parse_days` **before** any write and `hs::upsert_assignment` writes `days = excluded.days` (`api/homeschool.rs:1315-1362`, `homeschool/db.rs:658-686`); `hs4_i_upsert_assignment_rejects_a_bad_days_string_and_writes_nothing` covers reject / write `MW` / clear to `NULL`; the Year cell sheet's days control is per entry, prefilled from `entry_days` (`year.rs:90-106, 408-439`), sends `EditAssignment { days: Some(days()) }`, and `year.rs` no longer names `SetSubjectSchedule` (guarded); `hs5_qa3_the_year_cell_sheet_edits_the_days_of_one_week_not_of_every_week` renders the sheet and asserts the prefill, the per-entry count and the week label. |
| QH3-05 | **FIXED** | `homeschool/mod.rs:334` `nobody` memo; both the `Year` and `Month` arms render `NoSchoolPlan` before falling back to `LoadingCard` (`:738-742`, `:759-763`); guard `hs5_qa3_the_year_and_month_panes_offer_enrollment_when_nobody_is_enrolled` green. |
| QH3-06 | **FIXED** | `docs/homeschool/CURRICULUM_FORMAT.md` "Loading" (`--replace` bullet) and `docs/homeschool/README.md` "After replacing a curriculum file" both carry the stop/start sentence. |
| HS9 (B-3) | **VERIFIED** | `FamilyHubConfig::from_sources` → `Result<_, ConfigError>`, refusal checked on the **resolved** value under `cfg(test)` or `FAMILY_HUB_REFUSE_SYSTEM_DIR=1` (`config.rs:156-206, 369-391`), `is_system_data_dir` normalised (separators, trailing slash, case); `run` and `import-curriculum` use `try_load` and exit 1 with one line; `import_curriculum` refuses the live directory without `--yes` before reading the file or opening a pool (`loader.rs:1085-1093`); every harness in `tests/` pins `FAMILY_HUB_DATA_DIR` itself and the four WS suites do so in `hub_lock()`, the first line of every test; `every_integration_test_suite_sets_the_data_dir_itself` audits ≥ 15 suites; the canary in item 2 above ran with the variable unset. |

Round-1 and round-2 items were re-verified by rounds 2 and 3; each was re-read here on `main` as well (QH1-01
clear-then-set, QH1-02 memos read inside the resources, QH1-03 `merge_extras` order, QH1-04 `celebrate`,
QH1-05 `BoyChips` on Year/Month, QH1-06 scrubbed transcript, QH1-07 `before_span`, QH1-08 pool-before-copy,
QH1-09 the amended (e), QH1-10 `MAX_NOTE_CHARS`, QH2-01 Together Skip/Note fan-out, QH2-02 `UpdateExtra`,
QH2-03 the notice banner, QH2-04 clear-then-set in the Together tick, QH2-05 the truthful import count,
QH2-06 the `user_id` stamp, QH2-07 "School's out for Nathaniel") — all still in place.

## Contract items checked by reading, all satisfied

`migrations/0005_homeschool.sql` is H1 verbatim. H3 rules 1–9 in `src/shared/homeschool.rs` match the
normative text (rule-5 chunking with the empty check first, `weekly` on the first day, `free_read` never,
`date_for` on the 7-day span, `last_school_day`, `parse_days` in `M T W R F S U` order, Sakamoto `weekday`
with the four pinned answers and `2100-02-29` rejected, `today_view` keeping ticked rows in `due_today` and
daily work in `catch_up`, `together_view` collapsing to one slot with "n of m", `week_grid` re-anchored with
`free_read` excluded, `month_view` with `total` only inside the span); rule 10's list placement, 14-day floor
and span-scoped counts hold — its completeness clause does not (QH4-01). No date crate in `src/shared/`. H5
loader: ordered all-or-nothing validation with file name + `line N` from `toml::Spanned`, insert-missing-only
keyed `(slug, subject, week, ordinal)`, `--replace` updating in place and counting orphaned log rows,
`curricula_dir()` a method, absolute, created at boot, logged at `info`; the Isaiah seed is `ON CONFLICT
(profile_id) DO NOTHING` by name with warn/skip. H7: every `auth` fn calls `require_session_or_cookie` before
touching the pool; `toggle_lesson` rejects `subject_id ≤ 0` first, then the ±1 day window, then
`week ≠ current_week`, then any triple not in the boy's recomputed occurrences; `toggle_extra` is bounded by
the row's existence and the window; both LAN-open fns cap `note` at 500 chars; `add_extra` bounds
`scheduled_date` to ±365 days; `enroll`, `set_subject_schedule` and now `upsert_assignment` pass `days`
through `parse_days` before any write. Idempotency: every dated mutation claims its key inside the same
transaction as its write. Realtime: `homeschool_version` bumped by both variants and by `bump_all`; the TV
resource is keyed on `homeschool_version` and the date; the phone's resources read the memos they key on;
`FocusId::Lesson(String)` / `Extra(i64)` keyed on identity. Design direction: no external font or URL in any
new file; every ground/ink pair on both surfaces is in `PALETTE_PAIRS` (palette suite green in both runs;
`text-sheffield-accent` absent from the module); the TV type-scale golden is unchanged; overscan walker,
hover grep and rail budget green; glyphs are emoji constants in `glyphs.rs` (`YEAR_COMPLETE_GLYPH` now lives
there, HS-8); TV section labels are the §2.6 tracked-caps device on `TV_BODY_TEXT` as a `<p>`; phone headings
are the existing `text-lg font-bold text-sheffield-dark`. No test weakened: the migration/key-count bumps,
the five→six tab rename, the `nav.rs`/`tv_tests.rs` panel-count bumps, the golden-length bump and the
`config.rs` unit-test rewrites onto a scratch data dir (HS9: `from_sources` now returns `Result`, every
existing assertion kept) are the §3 mechanical class and are recorded in `docs/HANDOFF.md`; the `hs4_i`
second half and the `assignment_detail` assertion are additions. `docs_tests` id list carries HS1–HS7; link
checker green; no `| FAIL |` row; every VERIFICATION transcript `0 failed`. HS7 (d)'s `dx build --platform
web --release` was not re-run in this round (HS7's transcript stands and the wasm clippy gate, the compile
half, passed here); it cannot detect anything in this round's findings.

## Findings

Tier = the tier of the agent originally assigned (§3 roster: HS1 HS3 HS5 HS6 HS9 = O; HS2a HS2b HS4 HS7 = S).
Severity: Critical / High / Med / Low. PASS requires zero Critical/High/Med.

| id | task | tier | file:line | severity | description | solution (apply verbatim) |
| --- | --- | --- | --- | --- | --- | --- |
| QH4-01 | HS3 (+ one call site in HS4) | O | `src/shared/homeschool.rs:712-736` (`week_complete`, `can_finish_week`); `src/server/api/homeschool.rs:398-424` (`get_homeschool_today`) | **Med** | H3 rule 10 (normative): "Extras count in `done_count`/`total_count` **and in the Finish-week completeness check** only when dated inside the current week's span". The counts half is implemented (`merge_extras`); the completeness half is not: `can_finish_week(plan, enrollment, logs, date)` looks only at occurrences and log rows, and `get_homeschool_today` calls it before/independently of the extras it fetches for `merge_extras`, so `TogetherGroup.can_finish_week` is `true` the moment every curriculum occurrence is logged, whatever the boy's parent-added tasks in the span say. Visible on the phone: with every lesson ticked and one task a parent added for Thursday still open, the header chip reads `21 done · 0 skipped / 22` while the nudge under it says `Week 2 done — start week 3?` and offers **Finish week**; on the TV the boy's list still shows the 📌 row. Tapping Finish week moves the pointer and the task rolls into catch-up for 14 days (rule 10), so nothing is lost — but the surface contradicts itself and offers the week as done when the contract says it is not. No test exercises `can_finish_week` with an extra present (`hs3_i_*` cover `merge_extras` only; `hs4_h_*` cover `mark_all_done`). | (1) `src/shared/homeschool.rs`, directly after `can_finish_week`, add: `/// H3 rule 10: a parent-added task counts in the Finish-week completeness check **only** while it is dated inside the current week's span.` `pub fn extras_complete(extras: &[ExtraTask], user_id: i64, week_span: (&str, &str)) -> bool { extras.iter().filter(|extra| extra.user_id == user_id && extra.scheduled_date.as_str() >= week_span.0 && extra.scheduled_date.as_str() <= week_span.1).all(|extra| extra.status.is_some()) }` and `/// [`can_finish_week`] with H3 rule 10 applied: complete only when every occurrence **and** every extra dated inside the span is logged; the last-school-day clause is unchanged.` `pub fn can_finish_week_with_extras(plan: &WeekPlan, enrollment: &Enrollment, logs: &[LogRow], extras: &[ExtraTask], today: &str) -> bool { if enrollment.year_complete() { return false; } let extras_done = week_span(&enrollment.week_started_on).is_some_and(|(from, to)| extras_complete(extras, enrollment.profile_id, (&from, &to))); if week_complete(plan, enrollment, logs) && extras_done { return true; } last_school_day(&enrollment.week_started_on, &enrollment.school_days).is_some_and(|last| today >= last.as_str()) }`. (2) `src/server/api/homeschool.rs` `get_homeschool_today`: hoist the extras out of the `if let Some((span_from, span_to))` block — `let mut extras: Vec<ExtraTask> = Vec::new();` before it, inside it `extras = extra_rows.iter().map(to_extra_task).collect::<Result<Vec<_>, _>>()?;` (replacing the `let extras = …` binding) — and replace `if !sched::can_finish_week(&plan, enrollment, logs, &date) {` with `if !sched::can_finish_week_with_extras(&plan, enrollment, logs, &extras, &date) {`. (3) `--lib` test beside `hs3_i_*`: `#[test] fn hs3_i_an_unfinished_extra_inside_the_span_holds_finish_week_back() { let plan = sample_week(2); let enrollment = sample_enrollment(2, "2026-09-07"); let every_log: Vec<LogRow> = occurrences(&plan, &enrollment).into_iter().map(|o| LogRow { subject_id: o.subject_id, assignment_id: o.assignment_id, scheduled_date: o.scheduled_date, status: LogStatus::Done, note: None }).collect(); let midweek = "2026-09-09"; assert!(can_finish_week_with_extras(&plan, &enrollment, &every_log, &[], midweek)); assert!(!can_finish_week_with_extras(&plan, &enrollment, &every_log, &[extra(1, "2026-09-10", None)], midweek), "rule 10: an unfinished extra dated inside the span is part of the week"); assert!(can_finish_week_with_extras(&plan, &enrollment, &every_log, &[extra(1, "2026-09-10", Some(LogStatus::Done))], midweek)); assert!(can_finish_week_with_extras(&plan, &enrollment, &every_log, &[extra(2, "2026-09-18", None)], midweek), "an extra outside the span is not this week's"); assert!(can_finish_week_with_extras(&plan, &enrollment, &[], &[extra(1, "2026-09-10", None)], "2026-09-11"), "the last-school-day clause is untouched"); }`. (4) `tests/homeschool_tests.rs` (add `HomeschoolTodayView` to the `shared::types` import): `#[tokio::test] async fn hs4_h_an_unfinished_extra_inside_the_span_holds_finish_week_back() { let _guard = hs4_lock().await; let pool = db::pool().await.expect("pool"); reset_homeschool_state(pool).await; let curriculum_id = load_fixture(pool).await; const BOY: i64 = 1; let today = today_string(); // anchored on today with MTWRFSU, so the last school day is six days off and only completeness can offer Finish week` `enroll_direct(pool, BOY, curriculum_id, 1, "MTWRFSU", &today).await; let grid = api::get_week_grid(BOY, 1).await.expect("grid"); for occurrence in grid.rows.iter().flat_map(|row| row.cells.iter().flatten()) { hs::set_occurrence(pool, &hs::OccurrenceKey::new(BOY, 1, occurrence.subject_id, occurrence.assignment_id, occurrence.scheduled_date.clone()), "done", None, &today).await.expect("tick"); } let can_finish = |view: HomeschoolTodayView| view.groups.into_iter().next().expect("the boy's group").can_finish_week; assert!(can_finish(api::get_homeschool_today(today.clone()).await.expect("today")), "every occurrence is logged"); let token = parent_session().await; let extra = api::add_extra(BOY, today.clone(), "Tidy the schoolroom".to_string(), Category::Daily, None, today.clone(), format!("hs4-h-extra-{}", uuid_ish()), token).await.expect("an extra dated inside the span"); assert!(!can_finish(api::get_homeschool_today(today.clone()).await.expect("today")), "H3 rule 10: an unfinished extra inside the span holds Finish week back"); api::toggle_extra(extra.id, true, LogStatus::Done, None, today.clone(), format!("hs4-h-extra-{}", uuid_ish())).await.expect("tick the extra"); assert!(can_finish(api::get_homeschool_today(today).await.expect("today"))); }`. (5) Record the two new `pub fn`s in `docs/HANDOFF.md` (HS3's file gains a helper; the normative signature list is untouched). |
| QH4-02 | HS4 | S | `src/server/api/homeschool.rs:877-888` (`mark_all_done`) | Low | H6 item 4 places **Mark all done** on the boy's own block, which holds "his non-shared occurrences due today + his catch-up", and HS4's signature comment reads "unticked due_today + catch_up only". The server ticks every unlogged item of `today_view` — shared read-alouds included, because `today_view` hands back the boy's shared occurrences too and the loop filters only on `status.is_none()`. With two boys enrolled, one tap under Isaiah's name ticks the family's read-alouds for Isaiah alone and the **Together** row flips to `1 of 2` for a book nobody has opened; with one boy enrolled the button under his name also ticks every Together row above it. `hs4_h_*` uses `Sums`/`Copywork` only and cannot see it. | In `mark_all_done`, change `if occurrence.status.is_none() {` to `// H6 item 4: the control sits on the boy's own block; a shared read-aloud is Together's to tick.` `if occurrence.status.is_none() && !occurrence.shared {`. In `hs4_h_mark_all_done_ticks_only_unticked_items_and_is_idempotent`, after the first `mark_all_done`, add `let (shared_rows,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM lesson_log l JOIN subjects s ON s.id = l.subject_id WHERE l.profile_id = ?1 AND s.shared = 1").bind(BOY).fetch_one(pool).await.expect("count shared rows"); assert_eq!(shared_rows, 0, "H6 item 4: Mark all done is the boy's own block, never the Together rows");` (the fixture's `Old Tales`, `Fables` and `Painting` are all shared and all in catch-up for that test's anchor, so the assertion is live). |
| QH4-03 | HS5 (Boss DTO amendment; `src/shared/types.rs` is HS3's) | O | `src/client/components/homeschool/today.rs:480`, `:709` (`days: None`); `src/shared/types.rs` `LessonOccurrence` | Low | Already recorded as `docs/RESIDUAL.md` R-11 with the fix attached; restated here so the table is complete. After QH3-04's amendment `upsert_assignment` replaces the whole row, and Today's two inline text-edit handlers send `days: None` because `LessonOccurrence` carries no `days` — so a per-week override pinned from the Year sheet is silently reset to the subject's days by an unrelated text edit from Today. Recoverable (retype it in the Year sheet); not a regression against any Accept clause. | Concur with R-11 verbatim: add `#[serde(default)] pub days: Option<Vec<Weekday>>` to `LessonOccurrence` (schema-additive), set it in `occurrence()` from `row.and_then(\|row\| row.days.clone())`, pass `days: occurrence.days.as_deref().map(days_to_string)` at both Today call sites, drop the `days: None` allowance from the `hs5_qa3` guard, and add one `hs4`-style case proving a pinned row's inline text edit leaves `assignments.days` untouched. |

## Notes for the fixing wave

- QH4-01 is one pure helper in HS3's file plus a one-line call-site change in HS4's, with one `--lib` case
  and one `hs4` case; no migration, no DTO, no normative signature changes (`can_finish_week` is not in the
  §3 HS3 signature list). Land it on one branch and re-run the full baseline; nothing on the phone or TV
  needs to change — both already render from `can_finish_week`.
- QH4-02 is a one-line filter and one assertion; it can ride the same branch.
- QH4-03 is R-11 and waits for the Boss contract amendment R-11 already names; no action beyond concurrence.
- Observed, not filed: `School()`'s dispatcher returns early for every action, `OpenSettings` included,
  until the server's `today` has resolved, so the header chip is inert for the first round trip after a
  cold load (cosmetic, self-healing); `EnrollmentCard` labels `week_started_on` as "started" (cosmetic);
  the phone's `text-4xl` on two decorative emoji, noted by round 3, stands.
- Residuals R-2, R-3, R-4 and R-12 stand as recorded; no flaky test failed in either full run here, with the
  scratch directories wiped first and nothing else building on the box.
