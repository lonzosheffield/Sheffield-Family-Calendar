VERDICT: FAIL

# QA — Homeschool ("School") wave, round 2

**Auditor:** Fable 5 (HS8, fresh context, no prior knowledge of the run) · **Date:** 2026-09-03 ·
**Tree:** `main` @ `0889f16`, audited as the full diff `abb202b..HEAD` (73 files, +18,763 / −90) ·
**Contract:** `docs/homeschool/PLAN_HOMESCHOOL.md` v3.1 §0 (N1), §2 H1–H8 (normative SQL, occurrence
rule, signatures, DTOs), §3 HS1–HS7 Owns/Do/Accept, §4 defaults; reviews `RED_HS`, `PURPLE_HS`,
`WHITE_HS`, `DELTA_V3`; `docs/PLAN.md` §5; `docs/design/DESIGN_DIRECTION.md` §2–§3; `docs/PROTOCOL.md`;
`docs/qa/QA_HS_ROUND_1.md` (every QH1 item re-verified on `main`, not taken on trust).

Verdict is **FAIL** on three Med findings (QH2-01…QH2-03), all in HS5's phone tab and all in the
gap between what H6 says a parent can do on a row and what the row's handlers actually send.
Every committed acceptance suite is green on two consecutive full runs, both clippy gates and
`cargo fmt --check` exit 0, N1 holds on the tracked tree, and every round-1 fix that was merged is
genuinely fixed (details below). Nothing here touches a migration, a DTO or a normative signature.

## What was run on this machine (all exit 0 unless stated)

| Gate | Result |
| --- | --- |
| `cargo test --features server` (run 1) | 33 `test result:` lines, **613 passed, 0 failed**, 2 ignored (pre-existing) |
| `cargo test --features server` (run 2, consecutive) | 33 `test result:` lines, **613 passed, 0 failed**, 2 ignored — identical to run 1 (HS7 (d)) |
| `cargo clippy --features server --all-targets -- -D warnings` | exit 0 |
| `cargo clippy --features web --target wasm32-unknown-unknown -- -D warnings` | exit 0 (HS3 (e) as amended by QH1-09) |
| `cargo fmt --check` | exit 0 |
| `git ls-files docs/homeschool/curriculum/` | empty |
| `git grep -il ambleside` | `.gitignore`, the plan, the four reviews, `docs/HANDOFF.md` (the line naming the guard), `docs/VERIFICATION.md:50` (the HS2 row naming the guard), `docs/qa/QA_HS_ROUND_1.md` — **nothing under `src/`, `tests/`, `assets/`** |
| Every `name` / `source` / `text` / `detail` / `source_note` value ≥ 6 chars from the gitignored `ao-year-1.toml` (414 needles) grepped over every tracked text file | Hits only on generic plan vocabulary — `Poetry`, `Geography`, `Recitation`, `Composer`, `Free reads`, `chapter N` / `chapter I` (the parser's own doc example in `tests/curriculum_tests.rs`) — and two book names quoted in the white team's review as the reason for W-2. **No passage, book title or week→reading string is tracked** anywhere under `src/`, `tests/`, `assets/` or `docs/verification/`. QH1-06's transcript scrub is confirmed. |
| `Cargo.toml` / `Cargo.lock` since `abb202b` | untouched; `toml = "=1.1.5"` server-only (PLAN §5.8) |

### Accept clauses executed by hand (beyond the two full runs)

1. **HS7 (a)(b)** — `cargo test --features server --test homeschool_loop_tests`: TV-side `toggle_lesson` → phone < 1 s, phone-authed `set_school_week` → TV < 1 s, Together tick → both, kill/restart → both reconnect inside the backoff budget and every pre-restart write survives. `1 passed`.
2. **HS5 (b)** SSR — `cargo test --features server --test glyph_tests hs5_b`: Today for the fixture under `SignedOut` (one `Together`, one `Old Tales` with `part 1 of 2`, `(then tell it back)`, `Week 2 of 3`, three boy blocks, no `Finish week`/`Mark all done`/`Edit text`) and under `Parent` (exactly one `Finish week`, three `Mark all done`). `3 passed`. All 22 `hs5_*` tests: `22 passed`.
3. **HS6 (a)–(i)** — `cargo test --features server --test tv_tests hs6_`: golden section placement, BFS ≤ 12 (worst case 8), tick-every-row, `SetView(Homeschool)` + four state cards, type-scale/overscan, 4-panel wrap, no shared row, 📌 extra activatable, QH1-04's celebrate-over-rows. `10 passed`.
4. **HS3 (a)(b)(c)(f)(g)(h)(i)** — `cargo test --features server --lib hs3_`: `34 passed`; **HS3 (d)** `--test realtime_tests hs3_`: `1 passed` (16 = 14 + 2).
5. **HS4 (a)–(n)** — `cargo test --features server --test homeschool_tests hs4_`: `17 passed` (QH1-01's `hs4_n_*` and QH1-10's 501-char note included).
6. **HS1 (a)–(f)(h)(i)** — `cargo test --features server --test homeschool_db_tests`: `26 passed`, N1 guard included.
7. **HS1 (g)** by hand against `target\debug\family-hub.exe import-curriculum` with `FAMILY_HUB_DATA_DIR` at a scratch directory: a missing path → exit 1, no `curricula\`, no `family.db`; a `days = "Th"` file → exit 1, `bad.toml: line 6: subject "Stories" days "Th": 'h' is not one of the day letters MTWRFSU`, nothing copied, no `family.db`; the committed fixture → exit 0, copied, `family.db` created; a second import → exit 0 and `0 … inserted`. (The first import also printed `0 subjects, 0 assignments, 0 term notes inserted` — QH1-08, still open, see QH2-05.)
8. **HS2 (a)–(h)** — `cargo test --features server --test curriculum_tests` with the gitignored files present: `9 passed`.
9. **HS5 (a)** — `hs5_a_the_widest_tab_label_fits_a_column_of_the_narrowest_phone` + the six-button SSR: `mobile_tab_budget_px()` ≤ 60. Green.

## Round-1 findings re-verified on `main`

| Item | Status | How verified |
| --- | --- | --- |
| QH1-01 | **FIXED** | `api/homeschool.rs:793-804` clears before every set; `hs4_n_*` writes Done → Skipped → Done+note on one key and reads back `skipped` / `p.40` with row count 1. |
| QH1-02 | **FIXED** | `homeschool/mod.rs:300-370`: `focus`, `grid_week`, `cursor` are `use_memo`s read inside `grid_res` / `month_res`; `hs5_qa1_*` source-shape guard pins it. |
| QH1-03 | **FIXED** | `shared/homeschool.rs:837-849` orders `done` / `due_today` / `catch_up` per rule 10; `hs3_i_*` asserts a ticked extra is in both `due_today` and `done`, counted once; `day_items()` no longer pulls from `done`. |
| QH1-04 | **FIXED** | `tv/model.rs:611-629` (`NoSchoolToday` when `total_count == 0`, `celebrate` flag on `Day`); `surface.rs` draws the chip above the rows; `hs6_i_*` covers both sentences. Phone side `today.rs:489-495` says "School's out for {name} ⚽". |
| QH1-05 | **FIXED** | `BoyChips` in `today.rs:59-93`; Year and Month render it with `allow_everyone: false`; `hs5_h_two_enrolled_boys_*`, `hs5_i_the_month_chip_*`. |
| QH1-06 | **FIXED** | `docs/VERIFICATION.md` Chrome pass reads `curriculum_name: "<the AO file's name>"` and "the six *unshared* daily rows (Math first)"; the 414-needle grep above confirms no copied string remains. |
| QH1-07 | **FIXED** | `DaySheet.before_span` + `BEFORE_SPAN_LINE`; `hs5_j_a_past_day_*`. |
| QH1-08 | **OPEN** (BLOCKED, `hs/HS1-qa1` @ `076367f` unmerged) | Reproduced by hand (item 7 above). Restated as QH2-05 with the merge instruction. |
| QH1-09 | **FIXED** | Contract amended at `PLAN_HOMESCHOOL.md:445-447`, logged in `docs/HANDOFF.md`; the amended gate passes here. |
| QH1-10 | **FIXED** | `MAX_NOTE_CHARS = 500` checked first in `toggle_lesson` and `toggle_extra`; `hs4_d_*` rejects 501 chars with no write. |

## Contract items checked by reading, all satisfied

`migrations/0005_homeschool.sql` is H1 verbatim. H3 rules 1–10 in `src/shared/homeschool.rs`
match the normative text: the rule-5 chunking (1×2 → parts 1/2 Mon, 2/2 Wed; 2×2 → one each;
2×1 → both Tuesday; 2×3 → row 0 Mon+Wed, row 1 Fri; 0 rows or 0 days → none, checked before any
division), `weekly` on the first day, `free_read` never, `date_for` on the 7-day span from
`week_started_on`, `last_school_day`, `parse_days` rejecting `Th`/`MM`/`X`, Sakamoto `weekday`
with the four pinned answers, `today_view` keeping ticked rows in `due_today` and daily work in
`catch_up`, `merge_extras` with the 14-day floor and the span-scoped counts. No date crate in
`src/shared/`. H4 `together_view` renders a shared occurrence once, names covered and done boys,
and `toggle_lesson_together` revalidates per member and writes every matched boy in one
transaction. H5 loader: ordered all-or-nothing validation with file name + `line N` from
`toml::Spanned`, insert-missing-only keyed `(slug, subject, week, ordinal)`, `--replace` updating in
place and counting orphaned log rows, `curricula_dir()` a method, absolute, created at boot, logged
at `info`; the Isaiah seed is `ON CONFLICT (profile_id) DO NOTHING` by name with warn/skip. H7:
every `auth` fn calls `require_session_or_cookie` before touching the pool; `toggle_lesson`
rejects `subject_id ≤ 0` first, then the ±1 day window, then `week ≠ current_week`, then any
triple not in the boy's recomputed occurrences; `add_extra` bounds `scheduled_date` to ±365 days;
both LAN-open fns cap `note` at 500 chars; `enroll` and `set_subject_schedule` pass `days` through
`parse_days`. Idempotency: every dated mutation claims its key inside the same transaction as its
write. Realtime: `homeschool_version` bumped by both variants and by `bump_all` (Resync /
reconnect); the TV resource is keyed on `homeschool_version` **and** the date, so midnight and
reconnect both refetch; the phone's `view_res` likewise; `FocusId::Lesson(String)` / `Extra(i64)`
keyed on identity; `use_callback` in `School()` is replaced every render (dioxus-hooks 0.7.10), so
the mutation date it captures is never stale. Design direction: no external font or URL in any new
file; every ground/ink pair on both surfaces is in `PALETTE_PAIRS` (the palette suite walks all
four TV panels and `components/**`: `28 passed`); TV type-scale golden unchanged; overscan walker,
hover grep and rail budget green; glyphs are emoji constants in `glyphs.rs`; TV section labels are
the §2.6 tracked-caps device on `TV_BODY_TEXT` as a `<p>`; phone headings are the existing
`text-lg font-bold text-sheffield-dark`. No test weakened: the migration/key-count bumps, the
five→six tab rename (HS5's own Do clause), the `nav.rs`/`tv_tests.rs` panel-count bumps and the one
`hs3_i` assertion that encoded QH1-03 are all recorded in `docs/HANDOFF.md` per PLAN §5.2. No
non-Rust added since round 1. `docs_tests` id list carries HS1–HS7; link checker green.

## Findings

Tier = the tier of the agent originally assigned (§3 roster: HS1 HS3 HS5 HS6 = O; HS2a HS2b HS4 HS7 = S).
Severity: Critical / High / Med / Low. PASS requires zero Critical/High/Med.

| id | task | tier | file:line | severity | description | solution (apply verbatim) |
| --- | --- | --- | --- | --- | --- | --- |
| QH2-01 | HS5 | O | `src/client/components/homeschool/today.rs:429`, `:441` | **Med** | `TogetherRow` passes `on_skip: move \|()\| {}` and `on_note: move \|_: String\| {}` to `LessonRow`, while `LessonRow` (`row.rs:208-217`) renders the **Skip** and **Note** buttons whenever `can_edit && tickable` — which `TogetherRow` sets to `parent`. So a signed-in parent sees Skip and Note on every Together row and both do nothing, silently. Shared readings are exactly the rows H6 item 6 ("long-press → Skip / Note") and W-6 ("stopped at p.40") exist for; with the handlers empty a family cannot skip a read-aloud they are not doing (W-13) or note where they stopped, and the row sits in Together catch-up until Finish week. The normative `toggle_lesson_together` carries no status or note, but each covered boy's shared occurrence is a valid target of the per-boy `toggle_lesson` (it is in his `occurrences()`), so the fan-out belongs on the client. No SSR test clicks a button, so the suite cannot see it. | In `today.rs` add, above `TogetherRow`: `/// H6 item 6 on a Together row: Skip and Note are per-boy log-row writes, so they fan out to `toggle_lesson` for every boy the row covers — the normative Together signature carries neither. pub fn together_row_actions(slot: &TogetherOccurrence, week: i64, status: LogStatus, note: Option<String>) -> Vec<SchoolAction> { slot.user_ids.iter().map(\|&user_id\| SchoolAction::ToggleLesson { user_id, week, subject_id: slot.occurrence.subject_id, assignment_id: slot.occurrence.assignment_id, scheduled_date: slot.occurrence.scheduled_date.clone(), completed: true, status, note: note.clone() }).collect() }`. Replace line 429 with `on_skip: { let slot = slot.clone(); move \|()\| { for action in together_row_actions(&slot, week, LogStatus::Skipped, None) { on_action.call(action); } } },` and line 441 with `on_note: { let slot = slot.clone(); move \|note: String\| { let status = slot.occurrence.status.unwrap_or(LogStatus::Done); let note = (!note.trim().is_empty()).then(\|\| note.trim().to_string()); for action in together_row_actions(&slot, week, status, note) { on_action.call(action); } } },`. Add to `today.rs`'s `mod tests`: `together_skip_and_note_fan_out_to_every_covered_boy` — a `TogetherOccurrence` with `user_ids: vec![1, 2]` on subject 3 / assignment `Some(32)` / `2026-09-07`; `together_row_actions(&slot, 2, LogStatus::Skipped, None)` yields exactly two `SchoolAction::ToggleLesson` with `user_id` 1 and 2, `status: Skipped`, `completed: true`, the slot's triple; with `LogStatus::Done, Some("p.40".into())` both carry the note. |
| QH2-02 | HS5 | O | `src/client/components/homeschool/day_sheet.rs:139-159`, `row.rs:250-296`, `mod.rs:47-52` | **Med** | H6 Month view: "Extras can be **edited**, deleted, ticked or **skipped** from the same sheet." Neither is possible from the phone: `update_extra` is never imported or called anywhere under `src/client/` (`grep -rn update_extra src/client` is empty), `SchoolAction` has no variant for it, and every `SchoolAction::ToggleExtra` the surface emits (`day_sheet.rs:147-152`, `today.rs:650-655`) passes `status: LogStatus::Done` — `ExtraRow` offers a checkbox and Delete only. A parent who mistyped a title, or whose boy did not get to a task, can only delete it and add it again. The server side (`update_extra`, `toggle_extra(status)`) is complete and tested (HS4 (k), HS1 (i)); only the client is missing. | (1) `mod.rs`: add `update_extra` to the `crate::server::api` import; add the variant `UpdateExtra { extra_id: i64, title: String, category: Category, text: Option<String>, scheduled_date: String }` to `SchoolAction`; add the dispatch arm `SchoolAction::UpdateExtra { extra_id, title, category, text, scheduled_date } => { let _ = update_extra(extra_id, title, category, text, scheduled_date, String::new()).await; }`. (2) `row.rs` `ExtraRow`: add props `on_skip: EventHandler<()>` and `on_edit: EventHandler<String>`, a `let mut editing = use_signal(\|\| false); let mut draft = use_signal(\|\| extra.title.clone());`, and inside the `if can_edit` block replace the lone Delete button with `div { class: "mt-2 flex gap-3 text-xs font-semibold text-sheffield-dark", button { onclick: move \|_\| { let next = !editing(); editing.set(next); }, "Edit title" } button { onclick: move \|_\| on_skip.call(()), "Skip" } button { class: "text-red-700", aria_label: "Delete {extra.title}", onclick: move \|_\| on_delete.call(()), "Delete" } } if editing() { div { class: "mt-2 flex items-center gap-2", input { class: "w-full rounded-xl border border-slate-200 bg-white p-2 text-sm text-slate-800", r#type: "text", maxlength: "80", aria_label: "Title for {extra.title}", value: "{draft}", oninput: move \|event\| draft.set(event.value()) } button { class: "shrink-0 rounded-xl bg-sheffield-dark px-3 py-2 text-sm font-bold text-white", onclick: move \|_\| { editing.set(false); on_edit.call(draft()); }, "Save" } } }`. (3) `day_sheet.rs:139-159` and `today.rs:644-659`: pass `on_skip: { let extra_id = extra.id; move \|()\| on_action.call(SchoolAction::ToggleExtra { user_id, extra_id, completed: true, status: LogStatus::Skipped }) }` and `on_edit: { let extra = extra.clone(); move \|title: String\| { let title: String = title.trim().chars().take(EXTRA_TITLE_MAX).collect(); if !title.is_empty() { on_action.call(SchoolAction::UpdateExtra { extra_id: extra.id, title, category: extra.category, text: extra.text.clone(), scheduled_date: extra.scheduled_date.clone() }) } } }` (import `EXTRA_TITLE_MAX` from `day_sheet` in `today.rs`). (4) Tests: extend `hs5_j_only_a_parent_is_offered_the_add_task_form` — the parent render's `slice_at(&html, "data-extra-row", "91")` contains `>Skip<` and `>Edit title<`; the signed-out render's slice contains neither; add `the_day_sheet_can_skip_and_retitle_an_extra` in `mod.rs`'s `mod tests` asserting `SchoolAction::UpdateExtra { .. }` is a variant `dispatch` matches (a `match` arm exists — compile-time) and that `hs5_g`'s file count stays 8. |
| QH2-03 | HS5 | O | `src/client/components/homeschool/mod.rs:457-468` | **Med** | H6 Realtime: Together ticks "are **not** queued offline … they show a toast on failure"; `docs/PWA.md` (HS5's own paragraph) promises the same ("retried in the foreground with a message instead"). The dispatch arm is `let _ = toggle_lesson_together(…).await;` — a failure (expired parent cookie, hub unreachable, a boy moved to another week) is discarded and the parent sees the box stay empty with no word why. The lesson and extra arms at least queue; the Together arm neither queues nor tells. | In `School()`, before `let dispatch = use_callback(`, add `let mut notice = use_signal(\|\| Option::<String>::None);`. Replace the `ToggleTogether` arm's `let _ = toggle_lesson_together(` … `.await;` with `if let Err(err) = toggle_lesson_together(curriculum_id, week, subject_id, assignment_id, scheduled_date, completed, date, new_idempotency_key(), String::new()).await { notice.set(Some(format!("Couldn't tick that for everyone — {err}. Sign in as a parent and try again."))); }`. In the `rsx!`, directly after the header `div { class: "flex items-center justify-between gap-2", … }`, add `if let Some(message) = notice() { div { class: "flex items-center justify-between gap-3 rounded-2xl bg-sheffield-sun px-4 py-3 text-sm font-semibold text-slate-800", role: "alert", "data-school-notice": "true", span { "{message}" } button { class: "rounded-xl bg-white px-3 py-1 text-sm font-bold text-sheffield-dark", onclick: move \|_\| notice.set(None), "OK" } } }` (both pairs are already in `PALETTE_PAIRS`). Extend `hs5_qa1_the_year_and_month_resources_read_the_signals_they_are_keyed_on` (same source-shape style): the text of `mod.rs` between `SchoolAction::ToggleTogether {` and `SchoolAction::ToggleExtra {` contains `notice.set(` and does **not** contain `let _ = toggle_lesson_together`. |
| QH2-04 | HS4 | S | `src/server/api/homeschool.rs:1085-1088` | Low | `toggle_lesson_together` with `completed = true` calls `set_occurrence` alone (`INSERT … ON CONFLICT DO NOTHING`), so a boy whose row for that occurrence is `skipped` keeps it `skipped` while his brothers go `done`; `together_view` then reports every boy done but `status = None` (the statuses disagree), and the row renders unticked with no "n of m" chip. Unreachable from the phone until QH2-01 lands, reachable the moment it does. Same shape as QH1-01. | Replace lines 1085-1088 with `if completed { hs::clear_occurrence(&mut *tx, &key).await.map_err(super::to_server_error)?; hs::set_occurrence(&mut *tx, &key, LogStatus::Done.as_str(), None, &date).await.map_err(super::to_server_error)?; }` (the `else` branch is unchanged). Extend `hs4_f_*`: before the Together tick, `toggle_lesson` `TOGETHER_B`'s same triple with `status: LogStatus::Skipped`; after the Together tick `SELECT status FROM lesson_log WHERE profile_id = TOGETHER_B` is `done` and his row count is still 1. |
| QH2-05 | HS1 | O | `src/server/homeschool/loader.rs:1079-1099` | Low | QH1-08, still open: `import_curriculum` copies the file into `curricula_dir()` before `db::pool()` opens, and opening the pool runs `load_and_seed` over that directory, so a first import always prints `0 subjects, 0 assignments, 0 term notes inserted` (reproduced by hand, item 7 above). The fix exists on `hs/HS1-qa1` @ `076367f` (BLOCKED entry) but that branch is based on `c7c1339`, **before** the five round-1 squash commits — merging the branch whole would revert QH1-01…QH1-07. | Boss, on a fresh branch off current `main`: `git checkout 076367f -- src/server/homeschool/loader.rs tests/service_tests.rs` (the branch's only two changed files; `git diff main 076367f -- src/server/homeschool/loader.rs tests/service_tests.rs` is exactly the pool-before-copy move plus the `"7 subjects, 9 assignments, 3 term notes inserted"` assertion), run fmt, both clippy gates and `cargo test --features server --test service_tests --test homeschool_db_tests`, squash-merge, and close the BLOCKED entry. Do **not** merge `hs/HS1-qa1` itself. |
| QH2-06 | HS4 | S | `src/server/api/homeschool.rs:681-689` | Low | `get_month(user_id, …)` returns `MonthView.user_id` from `sched::month_view`, which falls back to the first extra's owner or `0` when the boy is unenrolled — so for an unenrolled boy with no extras the DTO names boy `0`, not the boy asked for. `MonthPanel` uses `month.user_id` as the pressed chip. Unreachable today (`focused_boy` only picks enrolled boys) but wrong on the wire. | Replace `Ok(sched::month_view(…))` with `let mut view = sched::month_view(enrollment.as_ref(), plan.as_ref(), &logs, &extras, year, month, &today); view.user_id = user_id; Ok(view)`. Extend `hs4_l_get_month_*`: `unenrolled_month.user_id == UNENROLLED`, and add a fourth boy (`4`, no extras) whose `get_month(4, 2026, 9).user_id == 4`. HS3's `month_view` signature and its tests are untouched. |
| QH2-07 | HS5 | O | `src/client/components/homeschool/today.rs:489-495` | Low | QH1-04's phone half ("School's out for {name} ⚽" for a boy with no rows and `total_count == 0` inside a group that is not paused) shipped without an SSR case; the Boss close asked round 2 to name one. | Add `hs5_c_a_boy_paused_inside_a_live_group_gets_schools_out_not_nothing_left` to `tests/glyph_tests.rs`: take `fixture_today_view()`, on `groups[0].boys[1]` clear `due_today`/`catch_up`/`done` and set every count to 0, render under `SignedOut`; assert `html.contains("School's out for Nathaniel")`, `!html.contains("Nothing left for Nathaniel")`, and the other boy's block still renders its rows. |

## Notes for the fixing wave

- QH2-01, QH2-02 and QH2-03 are client-only and all inside HS5's Owns; land them on one `hs/HS5-qa2` branch. QH2-04 pairs with QH2-01 (it is the server side of the same path) and belongs to HS4. QH2-06 is HS4 too. QH2-05 is a Boss cherry-pick, not an agent task.
- Nothing here touches `migrations/`, the DTOs in `types.rs`, or a normative signature.
- `dx build --platform web --release` was not re-run in this round (HS7's transcript stands; the wasm clippy gate, which is the compile half, passed here).
- Residuals recorded by the wave (`docs/RESIDUAL.md` R-2/R-3, HANDOFF H-HS3-5 load-flaky tests) stand; neither flaky test failed in either full run here.
