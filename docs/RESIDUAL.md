# RESIDUAL

Per PLAN §5 / PURPLE §P5.1 wave gate 6: gaps the Boss has accepted rather than
re-scoped into a task, each with the solution that would close it. Nothing here
is a defect the QA loop (T3.5) is still owed; a residual is a deliberate
"not in this run".

---

## R-1. The join-QR overlay does not carry the first-run setup code

- **Origin:** `docs/HANDOFF.md` T2.1 H-24 (Boss decision, wave 2-a) and
  `docs/qa/QA_ROUND_2.md` Q2-01 solution step 5.
- **What ships:** the first-run parent-PIN setup code is generated at boot by
  `router::run` and written to the hub's log and to `<data>\setup-code.txt`
  only. It is never served over the network and never rendered on the
  television (PLAN §3 T1.4 and PURPLE §P5.5 default 9, as amended at the QA
  round 2 close). Redeeming it is `POST /api/setup` from the phone's Settings
  tab, so setting the family's first PIN needs someone at the hub PC.
- **Why it is residual:** D1/D8 scope calendar editing and administration off
  the TV to phone-only, and the kiosk hydrates from the plain-HTTP listener;
  exposing the code there would gate the very first PIN on nothing more than
  LAN access.
- **Solution, if wanted later:** show the code on T2.1's join-QR overlay
  **only** when `parent_setup_code()` is gated to the kiosk's own HTTP
  listener (loopback / the reserved TV IP), never over `/api/*` or the
  hydration payload. Wave-3 hardening item; not scheduled.

## R-2. School's Today inline edit has no affordance for a row not yet on screen

- **Origin:** `docs/HANDOFF.md` H-HS5-6 (HS5 → HS7/HS8).
- **What ships:** `LessonOccurrence` carries the assignment row's **id**, not
  its `ordinal`, and `upsert_assignment` is keyed on `(subject, week,
  ordinal)`. The **Year** view derives the ordinal exactly (first appearance
  in date order, across the whole week), so its inline edit always works.
  **Today** only has the part of the week already dealt out on screen, so
  `homeschool::today::assignment_ordinals` recovers the ordinal for every row
  a parent can see there and simply offers no edit affordance for the rest
  (`edit_ordinal_for` returns `None`) — a parent who wants to pre-write
  Thursday's "Math: lesson 14" on Monday has to do it from **Year**, not
  **Today**.
- **Why it is residual:** H8 (owner, 2026-09-02) asks for Year/Month/extras,
  which is what shipped; this is a gap in Today's own inline edit, not in the
  feature H8 requested, and every occurrence the fixture and every acceptance
  test render is edit-affordanced correctly (§3 HS5 (b), (h)).
- **Solution, if wanted later:** one field — `ordinal: i64` on
  `LessonOccurrence`, which `occurrences()` already computes and has in hand
  — rather than more client-side inference. A schema-additive, backward
  compatible change to `src/shared/types.rs`.

## R-3. HS7's `/health` curricula check depends on the AO transcription being present on the box

- **Origin:** `docs/homeschool/PLAN_HOMESCHOOL.md` §3 HS7 Accept (e); N1.
- **What ships:** `/health`'s `curricula` count is exactly the number of
  curricula rows in the database, populated at boot by the directory loader
  (`FAMILY_HUB_CURRICULA_DIR`, default `<data>\curricula`). The transcript in
  `docs/VERIFICATION.md`'s HS7 row demonstrates `curricula: 0` for an empty
  directory and `curricula: 1` for a directory holding the real, gitignored
  `ao-year-1.toml` — but that file is **never** committed (N1), so a
  from-scratch checkout has no curricula directory contents at all until
  either HS2's transcription is regenerated from
  `docs/homeschool/curriculum/ao-year-1.notes.md` or the owner's own
  `import-curriculum` run (`docs/OWNER_CHECKLIST.md` step 14) places a real
  file there.
- **Why it is residual:** this is exactly what N1 requires — the licensed
  content cannot live in the repo — not a defect; recorded here so a future
  agent re-running HS7's Accept (e) on a fresh checkout knows to expect a
  "skip: file absent" result rather than treat it as a regression.
- **Solution:** none needed; this is the intended shape. If a future task
  wants the check to be exercisable from a clean checkout, it would need its
  own committed synthetic curriculum (mirroring `tests/fixtures/curricula/sample-year.toml`)
  loaded into a scratch directory rather than the real AO file.

## R-4. The full suite is flaky under parallel load (pre-existing; six distinct failures catalogued)

- **Origin:** HANDOFF H-HS3-5 (wave A), the HS5-qa1 / HS7-qa1 notes at the QA round 1 close,
  and HS1-qa2's five-run study at the QA round 2 close (reproduced on `main` @ `0889f16`, so
  not introduced by any HS branch). Each binary passes 4–5/5 in isolation; failures appear
  only when `cargo test --features server` runs its ~33 binaries concurrently, worse while
  another cargo build or a second full suite is running on the box.
- **What ships:** the eight Boss baselines of the two QA closes (2026-09-03) were all green
  after wiping `%TEMP%\familyhub-*` first — the round 2 close found 41 leftover scratch dirs
  (HS1-qa2 found 3478 the day before), each a pid-keyed directory some earlier run never
  removed (`838e62f` wipes them on reuse, not on exit). The round-4 fix close (2026-09-03) found
  48 before its first run and ~23 left behind by *each* full run after that; item 2 below fired
  once in the first gate run of that close and was green on the immediate re-run.
- **The failures seen, with the mechanism and the fix each wants:**
  1. `realtime_tests::t1_2_7_a_client_reconnects_and_resnapshots_within_thirty_seconds`
     ("the snapshot replays the stroke drawn before the restart, left: 149, right: 1") and
     its sibling `loop_tests::t2_6_phone_drives_the_tv_across_a_server_restart`: every test
     in the binary shares one sqlite file and `DEFAULT_BOARD_ID`, so a concurrent sibling's
     strokes land in the snapshot. Fix: a per-test board id (or a per-test data dir) in those
     two binaries; owners HS3/T1.2.
  2. `backup_tests::restore_drill_recreates_the_live_database_from_a_backup`:
     `remove_with_sidecars` swallows `std::fs::remove_file`'s error, so a Windows delete
     refused by a still-open handle leaves `assert!(!db_path.exists())` to fail. Fix:
     retry-with-backoff on the removal, or assert on the removal result; owner T1.5.
  3. `homeschool_db_tests::a_bad_file_beside_a_good_one_loads_exactly_one_curriculum_and_logs_the_path`:
     the WARN line goes missing (1 of 4 loader lines captured). `tracing::subscriber::set_default`
     is thread-local while callsite `Interest` is cached process-wide, so a sibling test that
     hits `loader.rs`'s WARN callsite with no subscriber installed caches `Interest::never`
     and the recorder never sees it. Fix: make the recorder the binary's global default, or
     serialise that test; owner HS1.
  4. Housekeeping: the leftover `%TEMP%\familyhub-*` directories above. Fix: remove the
     scratch dir in a `Drop` guard (or a documented pre-run wipe in `docs/DEV_WINDOWS.md`).
  5. `realtime_tests::t1_2_3_eight_clients_at_thirty_messages_per_second_for_thirty_seconds`
     (`tests/realtime_tests.rs:572`, the `p99 < 250 ms` broadcast-latency budget): failed once on
     HS3-qa4's box in a run started straight after a 14-minute cold clippy build, green on the next
     run (round-4 fix wave, 2026-09-03). A wall-clock budget on a loaded box; same housekeeping
     note as item 4 — wipe the scratch dirs, never run two builds at once; owner T1.2.
  6. `service_tests::a_startup_bind_failure_is_logged_within_five_seconds`: a 5 s wall-clock
     budget with no headroom — failed once at 5.103 s during HS5-qa4's full run, passed alone
     (`5 passed`) and on the re-run (round-4 fix wave, 2026-09-03). Fix: widen the budget or
     measure the log line rather than the process exit; owner T1.6 (service host).
- **Why it is residual:** none of the four is a product defect — each is test isolation on a
  shared box — and none failed in any Boss baseline once the scratch dirs were wiped. Fixing
  them touches four binaries owned by four different tasks; the QA loop's DONE gate (two
  consecutive green full runs) is met as is.
- **Solution:** the four one-liners above, as one Haiku/Sonnet housekeeping task in the next
  wave; until then, wipe `%TEMP%\familyhub-*` (keeping `familyhub-test`) before a full run and
  never run two full suites at once.

---

## R-5 … R-10. QA round 3 of the Homeschool wave (`docs/qa/QA_HS_ROUND_3.md`, HS8, 2026-09-03)

Every finding of round 3 stands open on `main` @ `141ee33` (none carries a FIXED
status in the report); each is recorded here with the auditor's (Fable 5) solution
attached verbatim so the fixing wave applies it without re-deriving it. Nothing
below touches `migrations/`, the DTOs in `src/shared/types.rs`, or a normative
signature.

**Status at the round 4 merge (Boss, 2026-09-03):** R-5 and R-10 applied by the Boss in
`342c0a6`; R-8's server half (the `upsert_assignment` `days` amendment) landed as HS4-qa3
`69e6166`; R-6, R-7, R-9 and R-8's client half landed as HS5-qa3b `dacb1af`. All six are
closed on `main` pending HS8's round 4 verdict. One new Low fell out of R-8's amendment and is
recorded below as R-11.

## R-5. QH3-01 (High, HS5/O) — `assets/tailwind.css` was never rebuilt after the HS wave

- **Origin:** `docs/qa/QA_HS_ROUND_3.md` QH3-01; HANDOFF H-21/H-30 (Boss rebuilds
  `assets/tailwind.css` once at the wave close).
- **What ships:** `assets/tailwind.css` (last rebuilt at `41600f7`, D4.3) is
  committed and nothing in `cargo build`/`dx build` regenerates it
  (`docs/DEV_WINDOWS.md` step 5); CI's "Tailwind rebuild (fail on diff)" step
  (`.github/workflows/ci.yml:67-74`) goes red on `main`. None of the utilities the
  HS wave introduced has a rule in the served stylesheet: `grid-cols-6`,
  `min-h-[44px]`, `min-h-[20px]`, `overflow-x-auto`, `min-w-[38rem]`,
  `min-w-[20rem]`, `max-h-[85vh]`, `rounded-t-3xl`, `rounded-md`, `w-28`, `w-10`,
  `h-6`, `p-6`, `mt-4`, `mt-6`, `mt-0.5`, `py-0.5`, `mr-1`, `mr-2`, `ml-1`,
  `items-stretch`, `self-center`, `list-none` (rebuild with the pinned 3.4.17
  binary: 20,991 B vs the committed 20,051 B; the running hub's `GET /tailwind.css`
  carries zero `grid-cols-6` rules). In the live app the phone's bottom `nav` is
  `grid` with no column template, so the six tab buttons stack in one column inside
  the fixed bottom bar on every tab of the PWA; HS5 (h)'s `min-h-[44px]` rows and
  `overflow-x-auto` container hold in the markup but not the render; the Year
  grid's `min-w-[38rem]` has no rule (W-9 not met); the settings, cell and day
  sheets have no `max-h-[85vh]` cap; the Year checkbox has no size. No Rust gate
  reads the file, so `dx build` exit 0 (HS7 (d)) could not see it.
- **Why it is residual:** a Boss-file rebuild step that was skipped at the wave
  close, not a defect in any Rust file the wave wrote; recorded pending the Boss
  micro-commit.
- **Solution (Fable, apply verbatim):** Boss micro-commit on `main`: run CI's exact
  command from the repo root with the pinned binary,
  `& "$env:USERPROFILE\.cargo\bin\tailwindcss.exe" -i input.css -o assets/tailwind.css --minify`,
  verify `Select-String -Path assets/tailwind.css -Pattern '\.grid-cols-6\{' -Quiet`
  is `True`, and commit `assets/tailwind.css` alone ("chore(boss): rebuild
  tailwind.css after the HS wave"). Then, so this cannot recur silently, add to
  `tests/ci_tests.rs`:

  ```rust
  #[test]
  fn every_tailwind_utility_named_under_components_has_a_rule_in_the_committed_css() {
      let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
      let css = std::fs::read_to_string(root.join("assets/tailwind.css")).expect("assets/tailwind.css");
      const PREFIXES: [&str; 22] = ["grid-cols-", "min-h-", "min-w-", "max-h-", "max-w-", "rounded", "overflow-", "w-", "h-", "p-", "px-", "py-", "m-", "mt-", "mb-", "ml-", "mr-", "gap-", "tracking-", "items-", "self-", "list-"];
      let mut missing = Vec::new();
      for path in walk_rs(&root.join("src/client/components")) {
          let source = std::fs::read_to_string(&path).expect("component source");
          for literal in source.split('"').skip(1).step_by(2) {
              for token in literal.split_whitespace() {
                  if !PREFIXES.iter().any(|p| token.starts_with(p)) || token.contains('{') { continue; }
                  let escaped: String = token.chars().flat_map(|c| if "[]./:%()".contains(c) { vec!['\\', c] } else { vec![c] }).collect();
                  if !css.contains(&format!(".{escaped}")) { missing.push(format!("{} in {}", token, path.display())); }
              }
          }
      }
      missing.sort();
      missing.dedup();
      assert!(missing.is_empty(), "assets/tailwind.css is stale — rebuild it with `tailwindcss -i input.css -o assets/tailwind.css --minify` and commit the diff. Missing rules: {missing:#?}");
  }
  ```

  with `fn walk_rs(dir: &std::path::Path) -> Vec<std::path::PathBuf>` collecting
  every `*.rs` under `dir` recursively (`std::fs::read_dir`, recurse into
  subdirectories). Run it before and after the rebuild: it must fail before (naming
  `grid-cols-6`) and pass after. Re-run `dx build --platform web --release` once and
  re-take the Chrome pass for `/m` if the CA can be installed; otherwise record the
  served `/tailwind.css` byte count and a `grid-cols-6` hit in `docs/VERIFICATION.md`.
  Land it first and re-run the full baseline (the palette suite scans classes in
  source, not the CSS, so it stays green either way).

## R-6. QH3-02 (Med, HS5/O) — inline text edit erases the row's `detail`

- **Origin:** `docs/qa/QA_HS_ROUND_3.md` QH3-02; `src/client/components/homeschool/mod.rs:127-132`,
  `:531-539`; `today.rs:438-448`, `:662-671`; `year.rs:361-373`;
  `src/server/homeschool/db.rs:661-663`.
- **What ships:** `SchoolAction::EditAssignment { subject_id, week, ordinal, text }`
  carries no `detail`, `dispatch` calls `upsert_assignment(subject_id, week,
  ordinal, text, None, …)`, and `hs::upsert_assignment` is `ON CONFLICT (subject_id,
  week, ordinal) DO UPDATE SET text = excluded.text, detail = excluded.detail` — so
  every `Edit text → Save` on a row whose file carries a `detail` (the fixture's
  `Old Tales` week 2 `stop at the bridge`) writes `NULL` over it and the row's
  second line disappears from Today, Year and the TV. The loader is
  insert-missing-only, so a reboot does not bring it back — only
  `import-curriculum --replace` does. `LessonOccurrence` already carries `detail`
  at all three call sites. No test edits a row that has a `detail`.
- **Why it is residual:** client-only, inside HS5's Owns, plus one storage
  assertion in HS1's `tests/homeschool_db_tests.rs`; queued for one `hs/HS5-qa3`
  branch with R-7.
- **Solution (Fable, apply verbatim):** (1) `mod.rs`: add `detail: Option<String>`
  to `SchoolAction::EditAssignment` and change the dispatch arm to
  `SchoolAction::EditAssignment { subject_id, week, ordinal, text, detail } => { let _ = upsert_assignment(subject_id, week, ordinal, text, detail, String::new()).await; }`.
  (2) `today.rs`: in `TogetherRow`'s `on_edit` and `DayItemRow`'s `on_edit`, capture
  `let detail = occurrence.detail.clone();` above the closure and pass
  `detail: detail.clone()` in the `EditAssignment` call. (3) `year.rs` `CellEntry`:
  same — `let detail = occurrence.detail.clone();` and `detail: detail.clone()` in
  the Save handler. (4) Guard, in `tests/glyph_tests.rs` beside `hs5_qa1_*` (same
  source-shape style): read `mod.rs`, slice from `SchoolAction::EditAssignment {`
  (the dispatch arm) to `SchoolAction::AddExtra {`, assert it contains `detail` and
  does **not** contain `text, None`; read `today.rs` and `year.rs` and assert each
  contains `detail: detail.clone()` at least once. (5)
  `tests/homeschool_db_tests.rs::a_parent_edit_survives_a_reload_and_replace_restores_the_file_text`:
  after the edit, additionally
  `assert_eq!(assignment_detail(&pool, old_tales, 2, 1).await, Some("stop at the bridge".into()))`
  when the edit passes `Some("stop at the bridge")` — pinning that the storage fn
  keeps a detail it is handed.

## R-7. QH3-03 (Med, HS5/O) — no `Back a week` from the `Year complete` state

- **Origin:** `docs/qa/QA_HS_ROUND_3.md` QH3-03; `src/client/components/homeschool/today.rs:280-285`,
  `:155-173`; H2 ("`current_week > weeks` is the terminal Year complete 🎉 state …
  Back returns to `weeks`"); HS4 (g).
- **What ships:** `GroupBlock` renders `year_complete` as a bare `StateCard` and
  returns before the nudge/Back branch, `nudge_line` returns `None` for
  `year_complete`, and the standalone `Back a week` button sits in the `else` arm
  that is unreachable once the year is complete. A parent who taps **Finish week**
  on the last week is left with a card and no way back; the only recovery is
  re-enrolling from School settings, which resets `week_started_on` and defaults the
  form to week 1. The TV is read-only here, so the phone is the only surface that
  can do it. The server already accepts `set_school_week(user_id, weeks, …)` from
  that state (HS4 (g)).
- **Why it is residual:** client-only, inside HS5's Owns; queued for the same
  `hs/HS5-qa3` branch as R-6.
- **Solution (Fable, apply verbatim):** In `today.rs` `GroupBlock`, replace the
  `else if group.year_complete { StateCard { … } }` arm with:

  ```rust
  else if group.year_complete {
      StateCard { glyph: glyphs::YEAR_COMPLETE_GLYPH, title: "Year complete".to_string(), body: "Every week of the plan is finished.".to_string() }
      if parent {
          button {
              class: "self-start rounded-xl bg-white px-3 py-2 text-sm font-bold text-sheffield-dark ring-1 ring-slate-200",
              onclick: {
                  let user_ids = user_ids.clone();
                  let weeks = group.weeks;
                  move |_| on_action.call(SchoolAction::SetWeek { user_ids: user_ids.clone(), week: weeks })
              },
              "Back a week"
          }
      }
  }
  ```

  Extend `hs5_c_a_paused_group_says_school_is_out_and_a_finished_year_celebrates`
  in `tests/glyph_tests.rs`: after the `Year complete` assertion on the parent
  render, `assert_eq!(count(slice_at(&html, "data-school-group", "1-2"), ">Back a week<"), 1)`;
  then `let signed_out = render_today(complete_view_clone, false); assert!(!signed_out.contains("Back a week"))`.

## R-8. QH3-04 (Low, HS5/O) — the Year cell sheet's `Save days` reschedules every week

- **Origin:** `docs/qa/QA_HS_ROUND_3.md` QH3-04; `src/client/components/homeschool/year.rs:296-320`;
  `docs/homeschool/PLAN_HOMESCHOOL.md:266-268`, `:475`.
- **What ships:** the Year cell sheet's **Save days** sends
  `SchoolAction::SetSubjectSchedule { subject_id, days, shared }`, i.e.
  `subjects.days` for **every** week, while H6 and D-5 describe the sheet as
  "inline edit of `assignment.text` / `days` **for that week**". The normative HS4
  signatures carry no per-week `days` setter (`upsert_assignment` has no `days`
  parameter, though `assignments.days` exists and rule 1 honours it), so HS5 used
  the only fn available; the surface silently reschedules all 36 weeks when the
  parent thinks they moved one, and the input starts empty rather than showing the
  current days.
- **Why it is residual:** a contract gap rather than an agent error; harmless while
  the family keeps the file's days. Needs a Boss contract amendment before code.
- **Solution (Fable, apply verbatim):** Boss amendment (PLAN §5.2, logged in
  HANDOFF): `upsert_assignment(subject_id, week, ordinal, text, detail: Option<String>, days: Option<String>, auth)`
  writing `assignments.days` after `parse_days` (HS4: `days = excluded.days` in the
  `ON CONFLICT` clause; reject a bad string before any write, one `hs4_i`-style
  case). HS5: `YearCellSheet`'s days control becomes per-entry, dispatching
  `EditAssignment { …, days: Some(days()) }`, prefilled with `occurrence`'s `days`
  or the subject's. Until the amendment lands, relabel the control
  `Days for {row.title} (every week)` and prefill it from `days_to_string` of the
  grid's `days` for that subject so the parent sees what they are overwriting.

## R-9. QH3-05 (Low, HS5/O) — Year/Month show an endless `Loading…` when nobody is enrolled

- **Origin:** `docs/qa/QA_HS_ROUND_3.md` QH3-05; `src/client/components/homeschool/mod.rs:687-724`.
- **What ships:** with nobody enrolled, Today shows the `No school plan yet → Enroll
  a boy` card, but toggling to **Year** or **Month** shows `Loading today's school
  work…` for ever (`focus()` is `None`, so `grid_res`/`month_res` resolve to
  `Ok(None)` and the panes fall through to `LoadingCard`). A loading message that
  never ends reads as a hung app.
- **Why it is residual:** Low; small enough to ride the `hs/HS5-qa3` branch.
- **Solution (Fable, apply verbatim):** In `School()`, compute
  `let nobody = use_memo(move || enrollments_memo().iter().all(|row| !row.enrolled));`
  and in both the `SchoolPane::Year` and `SchoolPane::Month` arms render
  `NoSchoolPlan { on_enroll: move |()| settings_open.set(true) }` when `nobody()`
  before falling back to `LoadingCard`. One SSR-free unit test:
  `focused_boy(None, &[enrollment(1,false)])` is already `None`; add
  `the_year_and_month_panes_offer_enrollment_when_nobody_is_enrolled` as a
  source-shape guard in `glyph_tests` asserting both `rsx!` arms name `NoSchoolPlan`.

## R-10. QH3-06 (Low, HS1/O) — `import-curriculum --replace` cannot tell open clients to refetch

- **Origin:** `docs/qa/QA_HS_ROUND_3.md` QH3-06; `src/server/homeschool/loader.rs:1108-1116`;
  `docs/homeschool/README.md`, `CURRICULUM_FORMAT.md` "Loading"; H6 Realtime
  (`CurriculumUpdated { curriculum_id }` "after edits/replace").
- **What ships:** `import-curriculum --replace` runs in its own process, so it
  cannot publish anything to the running hub's bus; a kiosk or phone already open
  keeps its fetched grid until the next `homeschool_version` bump or reconnect, and
  neither README nor CURRICULUM_FORMAT tells the owner to restart the service (or
  reopen the tab) after a replace.
- **Why it is residual:** inherent to the CLI design, not a defect in the code
  path; docs only.
- **Solution (Fable, apply verbatim):** add to `docs/homeschool/CURRICULUM_FORMAT.md`
  "Loading" and the README's parent section one sentence — "After
  `import-curriculum --replace`, `family-hub.exe stop` / `start` the service (or
  wait for the next School change) so open phones and the TV refetch the rewritten
  plan; the rows are already on disk."

## R-11. Today's inline text edit un-pins a per-week `days` override (Low, HS5 → Boss DTO amendment)

**Status (Boss, round-4 fix merge, 2026-09-03): CLOSED.** QH4-03 concurred with the solution below verbatim and HS5-qa4 (`0dd31b7`) landed it: `LessonOccurrence.days: Option<Vec<Weekday>>` (`#[serde(default)]`, appended last), set in `sched::occurrence()`, carried by both of `today.rs`'s inline edit handlers; the `hs5_qa3` guard lost its Today-side allowance and `homeschool_tests::hs4_i_a_pinned_rows_inline_text_edit_from_today_leaves_its_days_untouched` is the storage proof. `PLAN_HOMESCHOOL.md` §2's DTO listing carries the amendment.

- **Origin:** `docs/HANDOFF.md` H-HS5-qa3b-1 (HS5-qa3b, `dacb1af`); `src/client/components/homeschool/today.rs`
  (`TogetherRow` and `DayItemRow` `EditAssignment` calls); `src/shared/types.rs` `LessonOccurrence`.
- **What ships:** `SchoolAction::EditAssignment` carries `detail` **and** `days` because
  `upsert_assignment` replaces the whole row (R-8's amendment). The Year cell sheet sends both from
  what it has on screen, but Today's two inline-edit handlers pass `days: None`: `LessonOccurrence`
  carries no `days`, and Today holds only the part of the week already dealt out
  (`due_today` + `catch_up` + `done`), so `today.rs` cannot recover `assignments.days` the way
  `assignment_ordinals` recovers the ordinal (R-2's limit). A parent who pins "Days in week 12 only"
  from the Year sheet and later edits that row's *text* from Today drops the row back to the
  subject's days.
- **Why it is residual:** Low and recoverable (retype the days in the Year sheet). Closing it
  changes a normative DTO in HS3's Owns, which needs a Boss amendment, not an HS5 branch; not
  worth reopening the contract while round 4 is in flight.
- **Solution (Fable):** amend PLAN §5.2 / `PLAN_HOMESCHOOL.md` HS3: `LessonOccurrence` gains
  `days: Option<Vec<Weekday>>` (schema-additive, `#[serde(default)]`), which `occurrences()` has
  in hand where it partitions pinned/floating rows; `today.rs` then passes
  `days: occurrence.days.as_deref().map(days_to_string)` at both call sites, and the
  `hs5_qa3_an_inline_text_edit_carries_the_rows_detail_and_days_through` guard drops its
  Today-side `days: None` allowance. One `hs4`-style storage case: a pinned row's inline text edit
  leaves `assignments.days` untouched.

## R-12. `a_bad_file_beside_a_good_one_loads_exactly_one_curriculum_and_logs_the_path` drops its WARN line about once in fifteen runs (Low, HS1 test harness, pre-dates HS9)

- **Origin:** Boss, HS9 close, 2026-09-03. `tests/homeschool_db_tests.rs` (the
  `RecordingSubscriber` test near line 1065); `src/server/homeschool/loader.rs::load_directory`.
- **What happens:** the `LoadReport { loaded: 1, skipped: 1 }` assertion passes — the bad file
  *was* skipped and `tracing::warn!` *was* reached — but the recorder's captured lines are
  missing the `WARN … bad.toml` event (once, on 2026-09-03 06:26 in a pre-HS9 baseline, the
  `scanning the curricula directory` INFO line was missing as well). Seen twice in 31 full-suite
  runs kept in the Boss scratchpad (`full2.log`, `baseline-hs9.log`); every other run, including
  the three HS9 gate runs, passed. Not an HS9 effect: HS9 only added `init_test_env()` to this
  file's `scratch_dir`, and the first failure is from before the branch existed.
- **Confounder:** both failing runs happened while a *second* full `cargo test --features server`
  was running on the same machine (`full1.log`/`full2.log` are a minute apart; on 2026-09-03 a
  stalled harness job resumed and ran 14:48-14:58 under the Boss's 14:49-14:55 baseline), which
  the workflow forbids for exactly this reason. Every single-suite run on record is green. Treat
  the flake as unconfirmed until it is seen with one suite running.
- **Seen again (round-4 fix wave, 2026-09-03):** HS3-qa4's second full run failed at
  `tests/homeschool_db_tests.rs:1123` and passed alone (`26 passed`) and on the next full run — but
  two sibling worktrees (HS4-qa4, HS5-qa4) were building and testing on the same box at the time,
  so the confounder stands. The Boss's four single-suite gate runs at the fix merge were all green.
- **Why it is residual:** the assertion it guards (H5: "a bad file is logged at WARN with its
  path") holds every time it is inspected by hand and the loader code has one unconditional
  `warn!` on that path. The loss is in the test's capture, not the product; nobody ships on it.
- **Suspected mechanism (unverified):** the recorder is installed with the thread-scoped
  `tracing::subscriber::set_default` while the other tests in the same binary run in parallel
  with no subscriber at all, so `tracing`'s process-wide callsite-interest / max-level cache is
  rebuilt by this test while other threads are registering the same `loader.rs` callsites for
  the first time. The runtime is `current_thread`, so it is not a future hopping threads.
- **Solution (Fable):** make the capture independent of the cache: install the recorder once per
  binary with `set_global_default` (a `OnceLock<RecordingSubscriber>` whose lines are filtered by
  the scratch path each test already has in `needle`), or gate the WARN assertion behind a
  `tracing::callsite::rebuild_interest_cache()` call *before* `load_directory`. Either is a
  test-only change; rerun the suite ten times and keep the one that never drops a line.

---

## R-13 … R-16. QA round 5 of the Homeschool wave (`docs/qa/QA_HS_ROUND_5.md`, HS8, 2026-09-03)

Every finding of round 5 stands open on `main` @ `ccf4e96` (none carries a FIXED status
in the report); each is recorded here with the auditor's (Fable 5) solution attached
verbatim so the fixing wave applies it without re-deriving it. None is a regression of
a round-1…4 item: R-13 and R-14 are consequences of the per-week `days` override that
the QH3-04 amendment (R-8) introduced, R-15 of the H2 nudge being driven by
`can_finish_week` alone, and R-16 is a validation inconsistency in HS4's file. R-13
needs a Boss DTO amendment first (same shape and provenance as R-11 / QH4-03's
`days`); applying it also closes R-2.

## R-13. QH5-01 (High, HS5/O + one DTO field in HS3's `src/shared/types.rs`) — an edit on a pinned later-ordinal row overwrites the *other* row

- **Origin:** `docs/qa/QA_HS_ROUND_5.md` QH5-01;
  `src/client/components/homeschool/year.rs:61-77` (`row_ordinals`), `:327-330`;
  `src/client/components/homeschool/today.rs:124-152` (`assignment_ordinals`), `:443`,
  `:533-544`, `:669`; `src/shared/homeschool.rs:646-686`.
- **What ships:** both surfaces recover the `ordinal` that `upsert_assignment` is keyed on
  from the row's **rank in date order** — `row_ordinals` takes first appearance across the
  grid's day cells, `assignment_ordinals` sorts the on-screen lessons by `scheduled_date`.
  That was exact while every row floated in rule 5's spread, and R-2 records it as exact.
  It stopped being exact the moment QH3-04 landed: a row pinned by `assignments.days` is
  dealt to its own days and takes no part in the spread (`shared/homeschool.rs:646-686`),
  so a later-ordinal row pinned to an earlier day now ranks first. Concrete, on the
  committed fixture: from the Year cell sheet a parent moves `Fables` week 1's second
  reading (`ordinal 2`, "The Patient Heron", Friday) to Monday — `Save days` correctly
  sends `ordinal: 2, days: Some("M")`. After the refetch the grid is Mon `Heron` (pinned),
  Tue `Kite` part 1 of 2, Fri `Kite` part 2 of 2, and `row_ordinals` /
  `assignment_ordinals` now say `Heron → 1, Kite → 2`. The parent's **next** edit on that
  subject — retyping Heron's text from the Year sheet or from Today, or touching its days
  again — is sent as `EditAssignment { ordinal: 1, text: <Heron's draft>, days: Some("M") }`
  and `upsert_assignment` (`ON CONFLICT (subject_id, week, ordinal) DO UPDATE`) overwrites
  **Kite's** row: its text is replaced by Heron's and it is pinned to Monday too; the real
  Heron row is untouched, so the week now shows Heron twice and "The Kite and the Kettle"
  is gone from the database. The loader is insert-missing-only, so a reboot does not bring
  it back; the only recovery is `import-curriculum --replace` on the hub PC or retyping the
  lost text by hand. Silent data loss from a supported parent action, on the one subject of
  the family's real file that carries two ordinals every week; reachable from both the Year
  sheet and Today. No test pins an ordinal-2 row ahead of an ordinal-1 row
  (`hs4_i_a_pinned_rows_inline_text_edit_from_today_leaves_its_days_untouched` pins a
  subject with one row).
- **Why it is residual:** the one that must not wait — silent data loss, and the family's
  real file has a two-ordinal subject every week. It needs the Boss DTO amendment first,
  then one `hs/HS5-qa5` branch touching `types.rs` (the field), `shared/homeschool.rs` (one
  line + one `--lib` case), `today.rs`, `year.rs`, `tv/fixture.rs`, the two component test
  fixtures, `tests/homeschool_tests.rs` and `tests/glyph_tests.rs`. Nothing on the TV
  changes behaviour (the kiosk never edits). Closes R-2 for free.
- **Solution (Fable, apply verbatim):** The row already knows its own ordinal — carry it,
  as `days` is carried (QH4-03), and delete the inference.
  1. Boss amendment to `docs/homeschool/PLAN_HOMESCHOOL.md` §3 HS3's `LessonOccurrence`
     line: append `ordinal: i64` after `days` — "the row's `assignments.ordinal` (`1` for
     the untitled daily occurrence, the row H6 item 6 creates),
     `#[serde(default = "first_ordinal")]`, appended last (QA round 5 QH5-01)".
  2. `src/shared/types.rs`, in `LessonOccurrence` directly after
     `pub days: Option<Vec<Weekday>>,`:

     ```rust
     /// The row's own `assignments.ordinal` — the key `upsert_assignment` writes to (QA round 5 QH5-01).
     /// `1` for the untitled daily occurrence, which is the row H6 item 6 creates. Appended last and
     /// defaulted, like `days`, so the DTO stays schema-additive.
     #[serde(default = "first_ordinal")]
     pub ordinal: i64,
     ```

     and, beside the struct, `fn first_ordinal() -> i64 { 1 }`.
  3. `src/shared/homeschool.rs::occurrence()`: after the `days:` field add
     `ordinal: row.map_or(1, |row| row.ordinal),`.
  4. `today.rs`: replace both
     `let edit_ordinal = edit_ordinal_for(&ordinals, subject_id, assignment_id);` (`:443`,
     `:669`) with `let edit_ordinal = Some(occurrence.ordinal);`; delete `group_items`,
     `item_date`, `assignment_ordinals`, `edit_ordinal_for`, the `ordinals` prop and
     argument on `GroupBlock`, `TogetherRow`, `BoyBlock` and `DayItemRow`, and the
     now-unused `BTreeMap` import.
  5. `year.rs`: delete `row_ordinals` and its two unit tests
     (`ordinals_come_from_first_appearance_across_the_week`,
     `a_daily_row_with_no_assignment_rows_has_no_ordinals_to_recover` — they test the
     deleted helper, not an Accept clause); in `YearCellSheet` replace the
     `ordinal: occurrence.assignment_id.and_then(|id| ordinals.get(&id).copied()).unwrap_or(1),`
     prop with `ordinal: occurrence.ordinal,` and drop `let ordinals = row_ordinals(&row);`.
  6. Give every `LessonOccurrence` literal the field: `tv/fixture.rs::occurrence()` and the
     `today.rs` / `year.rs` test fixtures `ordinal: 1` (the `year.rs`
     `occurrence(Some(52), …)` fixture `ordinal: 2`); `tests/glyph_tests.rs` builds its rows
     through `sched::today_view` / `sched::week_grid`, so nothing there changes.
  7. `--lib` case beside `hs3_b_an_assignment_row_with_its_own_days_overrides_the_subjects`:

     ```rust
     #[test]
     fn hs3_b_a_pinned_later_ordinal_keeps_its_own_ordinal_ahead_of_an_earlier_row() {
         let mut plan = reading_plan("TF", 2);
         plan.subjects[0].rows[1].days = Some(days("M"));
         let enrollment = sample_enrollment(1, "2026-09-07");
         let dealt = occurrences(&plan, &enrollment);
         assert_eq!(dealt[0].scheduled_date, "2026-09-07");
         assert_eq!(dealt[0].assignment_id, Some(101));
         assert_eq!(dealt[0].ordinal, 2, "QH5-01: the ordinal is the row's own, never its rank in date order");
         assert_eq!(dealt[1].assignment_id, Some(100));
         assert_eq!(dealt[1].ordinal, 1);
     }
     ```
  8. `tests/homeschool_tests.rs`, beside
     `hs4_i_a_pinned_rows_inline_text_edit_from_today_leaves_its_days_untouched`:

     ```rust
     #[tokio::test]
     async fn hs4_i_editing_a_pinned_second_reading_never_overwrites_the_first() {
         let _guard = hs4_lock().await;
         let pool = db::pool().await.expect("pool");
         reset_homeschool_state(pool).await;
         let curriculum_id = load_fixture(pool).await;
         let fables = subject_id(pool, curriculum_id, "Fables").await;
         let token = parent_session().await;
         const BOY: i64 = 1;
         const ANCHOR: &str = "2026-09-07";
         api::upsert_assignment(fables, 1, 2, "The Patient Heron".to_string(), None, Some("M".to_string()), token.clone())
             .await
             .expect("pin week 1's second fable to Monday from the Year sheet");
         enroll_direct(pool, BOY, curriculum_id, 1, "MTWRF", ANCHOR).await;
         let grid = api::get_week_grid(BOY, 1).await.expect("grid");
         let row = grid.rows.iter().find(|row| row.title == "Fables").expect("the Fables row");
         let monday = row.cells[0].first().cloned().expect("the pinned reading is dealt to Monday");
         assert_eq!(monday.text.as_deref(), Some("The Patient Heron"));
         assert_eq!(monday.ordinal, 2, "QH5-01: the Monday entry is ordinal 2 however early it falls");
         api::upsert_assignment(fables, 1, monday.ordinal, "The Patient Heron, retold".to_string(), monday.detail.clone(), monday.days.as_deref().map(days_to_string), token.clone())
             .await
             .expect("the Year sheet's edit, sent with the occurrence's own ordinal");
         let rows: Vec<(i64, String, Option<String>)> =
             sqlx::query_as("SELECT ordinal, text, days FROM assignments WHERE subject_id = ?1 AND week = 1 ORDER BY ordinal")
                 .bind(fables)
                 .fetch_all(pool)
                 .await
                 .expect("week 1 rows");
         api::upsert_assignment(fables, 1, 2, "The Patient Heron".to_string(), None, None, token).await.expect("restore the fixture row");
         assert_eq!(
             rows,
             vec![(1, "The Kite and the Kettle".to_string(), None), (2, "The Patient Heron, retold".to_string(), Some("M".to_string()))],
             "the first fable is untouched and the second carries the edit"
         );
     }
     ```
  9. Source-shape guard in `tests/glyph_tests.rs` beside `hs5_qa3_*`:

     ```rust
     #[test]
     fn hs5_qa5_the_ordinal_an_edit_writes_to_is_the_rows_own() {
         for file in ["today.rs", "year.rs"] {
             let source = school_source(file);
             for banned in ["assignment_ordinals(", "row_ordinals(", "edit_ordinal_for("] {
                 assert!(!source.contains(banned), "{file} must not infer an ordinal from date order (QH5-01): {banned}");
             }
         }
         assert!(one_line(&school_source("today.rs")).contains("let edit_ordinal = Some(occurrence.ordinal);"));
         assert!(one_line(&school_source("year.rs")).contains("ordinal: occurrence.ordinal,"));
     }
     ```
  10. `docs/RESIDUAL.md` R-2 is closed by the same field (Today's edit no longer needs a
      row on screen to know its ordinal); record it at the merge, and record the two
      `year.rs` unit-test deletions in `docs/HANDOFF.md`.

## R-14. QH5-02 (Med, HS5/O) — the Year cell sheet's *text* Save pins a floating row to its resolved days

- **Origin:** `docs/qa/QA_HS_ROUND_5.md` QH5-02;
  `src/client/components/homeschool/year.rs:389-406` (`CellEntry`'s text **Save**,
  `days: pinned_days(&days())` at `:401`); `:447-450`; `src/shared/homeschool.rs:665-672`.
- **What ships:** H6/D-5 make the cell sheet's text control an "inline edit of
  `assignment.text`". Its Save handler sends `days: pinned_days(&days())`, i.e. the days
  control's value — which is prefilled with the row's **resolved** days (`entry_days`, "MW"
  for a floating split row) — so a text-only edit writes `assignments.days = 'MW'` over the
  row's `NULL`. The row is now pinned: rule 1 deals it once per pinned day with
  `part: None` and it takes no part in rule 5's spread, so after retyping `Old Tales` week
  2's text from the Year sheet the phone and the TV show the full reading on Monday **and**
  Wednesday with no `part 1 of 2` / `continue · 2 of 2` (rule 5's labels, HS5 (b)'s and
  HS6's row anatomy), and that week is detached from the subject's days for good (a later
  `School settings` change to `Old Tales`' days leaves week 2 on MW; a school-days change
  intersects it away). Today's two handlers (`days: occurrence.days`) and the Year sheet's
  own `Save days` (`days: Some(days())`) are right; only the text Save conflates "what the
  control shows" with "what the row stores". Invisible to the existing guards:
  `hs5_qa3_the_year_cell_sheet_edits_the_days_of_one_week_not_of_every_week` asserts the
  prefill and `days: Some(days())`, both of which stay true.
- **Why it is residual:** a one-line change in `CellEntry` plus a deleted helper; rides the
  same `hs/HS5-qa5` branch as R-13. Its `homeschool_tests` case reads `ordinal` off the
  occurrence, so land R-13 first or pass `1` meanwhile.
- **Solution (Fable, apply verbatim):** `year.rs` `CellEntry`: above the `rsx!`, beside
  `let detail = occurrence.detail.clone();`, add

  ```rust
  // QH5-02: the text control writes back the days the row *stores* (`None` = inherit the subject's),
  // never the resolved days the control displays — otherwise a text edit pins a floating row and
  // drops its part labels.
  let stored_days = occurrence.days.as_deref().map(days_to_string);
  ```

  in the text **Save** button's `onclick` capture it
  (`let detail = detail.clone(); let stored_days = stored_days.clone();`) and replace
  `days: pinned_days(&days()),` with `days: stored_days.clone(),`; leave `Save days` at
  `days: Some(days()),`; delete `pinned_days` and its unit test
  `an_empty_days_control_inherits_the_subject_rather_than_writing_nonsense` (a helper test,
  not an Accept clause). Extend
  `hs5_qa3_the_year_cell_sheet_edits_the_days_of_one_week_not_of_every_week` in
  `tests/glyph_tests.rs`:

  ```rust
  let year = school_source("year.rs");
  assert!(one_line(&year).contains("let stored_days = occurrence.days.as_deref().map(days_to_string);"), "QH5-02: the text Save must write back the stored override");
  assert!(one_line(&year).contains("days: stored_days.clone(),"));
  assert!(!year.contains("pinned_days("), "a text edit must never pin a floating row (QH5-02)");
  ```

  Storage proof in `tests/homeschool_tests.rs` beside `hs4_i_*` (uses `ordinal` from
  R-13 / QH5-01; until that lands, pass `1`):

  ```rust
  #[tokio::test]
  async fn hs4_i_a_text_edit_from_the_year_sheet_leaves_a_floating_row_floating() {
      let _guard = hs4_lock().await;
      let pool = db::pool().await.expect("pool");
      reset_homeschool_state(pool).await;
      let curriculum_id = load_fixture(pool).await;
      let old_tales = subject_id(pool, curriculum_id, "Old Tales").await;
      let token = parent_session().await;
      const BOY: i64 = 1;
      enroll_direct(pool, BOY, curriculum_id, 2, "MTWRF", "2026-09-07").await;
      let grid = api::get_week_grid(BOY, 2).await.expect("grid");
      let row = grid.rows.iter().find(|row| row.title == "Old Tales").expect("Old Tales");
      let monday = row.cells[0].first().cloned().expect("part 1 of 2 on Monday");
      assert_eq!(monday.part, Some((1, 2)));
      assert_eq!(monday.days, None, "the fixture row floats");
      api::upsert_assignment(old_tales, 2, monday.ordinal, "ch. 2 'The Long Road', retold".to_string(), monday.detail.clone(), monday.days.as_deref().map(days_to_string), token.clone())
          .await
          .expect("the Year sheet's text edit");
      let after = api::get_week_grid(BOY, 2).await.expect("grid");
      let row = after.rows.iter().find(|row| row.title == "Old Tales").expect("Old Tales");
      api::upsert_assignment(old_tales, 2, 1, "ch. 2 'The Long Road'".to_string(), Some("stop at the bridge".to_string()), None, token).await.expect("restore");
      assert_eq!(row.cells[0].first().and_then(|o| o.part), Some((1, 2)), "QH5-02: a text edit must not pin the row and lose its split");
      assert_eq!(row.cells[2].first().and_then(|o| o.part), Some((2, 2)));
  }
  ```

## R-15. QH5-03 (Med, HS5/O) — the nudge calls the week "done" on the last school day with work outstanding; H2's fortnight nudge is unreachable

- **Origin:** `docs/qa/QA_HS_ROUND_5.md` QH5-03;
  `src/client/components/homeschool/today.rs:155-173` (`nudge_line`), `:311-331`;
  `src/shared/homeschool.rs:757-774`.
- **What ships:** H2 (normative): the footer nudges "**Week 3 done — start week 4?** when
  complete; **You've been on week 3 for 15 days** once `today − week_started_on ≥ 14`", and
  Finish week is *offered* when complete **or** today ≥ the last school day. `nudge_line`
  prints the "done" sentence whenever `group.can_finish_week` is true — and
  `can_finish_week_with_extras` is true on the last school day whatever is logged — so on
  Friday with work outstanding the banner reads `Week 2 done — start week 3?` directly
  under a chip reading `9 done · 0 skipped / 22` (the same surface-contradicts-itself shape
  round 4 filed as QH4-01). Worse, the second nudge H2 specifies is unreachable in
  production: `days_on_week ≥ 14` implies `today > week_started_on + 6 ≥ last_school_day`,
  so `can_finish_week` is already true and the first branch always wins; the only thing
  that ever exercises "You've been on week N for D days" is a unit fixture with
  `can_finish_week: false`. The DTO has no `week_complete` flag, but it does not need one:
  `done_count + skipped_count == total_count` over the group's boys is exactly "every
  occurrence and every in-span extra logged" (H3 rule 8 + rule 10 — the very rows
  `header_chip_text` sums), so the chip and the nudge can be made to agree from what the
  client already holds.
- **Why it is residual:** client-only in `today.rs`, no server change; the HS5 (b) fixture
  keeps rendering exactly one `Finish week` per finishable group. Rides the `hs/HS5-qa5`
  branch.
- **Solution (Fable, apply verbatim):** `today.rs`: replace `nudge_line` with

  ```rust
  /// H2: "complete" is every occurrence and every in-span extra logged — the rows
  /// `header_chip_text` sums (H3 rule 8 + rule 10), so the chip and the nudge cannot disagree.
  pub fn week_is_complete(group: &TogetherGroup) -> bool {
      let logged: u32 = group.boys.iter().map(|boy| boy.done_count + boy.skipped_count).sum();
      let total: u32 = group.boys.iter().map(|boy| boy.total_count).sum();
      total > 0 && logged >= total
  }

  /// H2's nudge line, or `None` when the week needs no nudging. Three sentences, in H2's order: complete;
  /// a fortnight on one week; the last school day reached with work still open (Finish week is offered
  /// then too, but the week is not "done").
  pub fn nudge_line(group: &TogetherGroup) -> Option<String> {
      if group.paused || group.year_complete {
          return None;
      }
      if week_is_complete(group) {
          return Some(format!("Week {} done — start week {}?", group.week, group.week + 1));
      }
      if group.days_on_week >= 14 {
          return Some(format!("You've been on week {} for {} days", group.week, group.days_on_week));
      }
      if group.can_finish_week {
          return Some(format!("Last school day of week {} — finish it now, or carry the rest into next week", group.week));
      }
      None
  }
  ```

  (the Finish week button stays inside the banner behind `if group.can_finish_week`, so it
  is still offered on the last school day). Unit tests in `today.rs`: change
  `a_complete_week_nudges_towards_the_next_one` to build a complete group —
  `let mut done = group(2, true, 3); done.boys[0].done_count = 10; done.boys[0].skipped_count = 1; assert_eq!(nudge_line(&done).as_deref(), Some("Week 2 done — start week 3?"));`
  — and add

  ```rust
  #[test]
  fn the_last_school_day_offers_finish_week_without_calling_the_week_done() {
      assert_eq!(nudge_line(&group(2, true, 4)).as_deref(), Some("Last school day of week 2 — finish it now, or carry the rest into next week"));
  }
  ```

  and, in `a_fortnight_on_one_week_nudges_by_elapsed_days_instead`,
  `assert_eq!(nudge_line(&group(3, true, 15)).as_deref(), Some("You've been on week 3 for 15 days"), "QH5-03: the fortnight nudge outranks the last-school-day one");`.
  `tests/glyph_tests.rs::hs5_b_the_identical_render_as_a_parent_gains_exactly_the_parent_affordances`
  keeps its `count(&html, "Finish week") == complete_groups` (the fixture's finishable
  group renders the third sentence and one button); add
  `assert!(!html.contains("done — start week"), "nothing in the fixture is complete, so nothing may be called done (QH5-03)");`
  to `hs5_b_today_renders_the_fixture_the_way_h6_lays_it_out`.

## R-16. QH5-04 (Low, HS4/S) — `update_extra` re-files an extra to any date, escaping `add_extra`'s ±365-day window

- **Origin:** `docs/qa/QA_HS_ROUND_5.md` QH5-04; `src/server/api/homeschool.rs:1510-1558`
  (`update_extra`) vs `:1416-1431` (`add_extra`).
- **What ships:** HS4 (k) bounds `add_extra`'s `scheduled_date` to
  `[today − 365, today + 365]` after D-8's "unbounded write primitive" finding;
  `update_extra` re-files an existing extra to any date that parses (`2099-01-01`,
  `0001-01-01`), so the bound is one edit away from meaningless. Parent-gated, so not a
  LAN-security hole; an inconsistency in the same validation, and `get_month`'s per-month
  window means such a task is simply never seen again.
- **Why it is residual:** Low; a helper and one assertion in HS4's file.
- **Solution (Fable, apply verbatim):** Factor the window into one helper directly above
  `add_extra`:

  ```rust
  /// HS4 (k): a parent-added task lives within a year of today, on the way in and on every re-filing.
  fn check_extra_date(scheduled_date: &str) -> Result<(), ServerFnError> {
      if sched::weekday(scheduled_date).is_none() {
          return Err(validation_error("scheduled_date must be a valid YYYY-MM-DD date"));
      }
      let today = today_string();
      let earliest = sched::add_days(&today, -365).ok_or_else(|| validation_error("could not compute the allowed date range"))?;
      let latest = sched::add_days(&today, 365).ok_or_else(|| validation_error("could not compute the allowed date range"))?;
      if scheduled_date < earliest.as_str() || scheduled_date > latest.as_str() {
          return Err(validation_error("scheduled_date must be within a year of today"));
      }
      Ok(())
  }
  ```

  in `add_extra` replace the `weekday` check through the `within a year of today` return
  with `check_extra_date(&scheduled_date)?;`, and in `update_extra` replace its `weekday`
  check with the same call. Extend
  `hs4_k_add_extra_requires_a_session_and_bounds_scheduled_date`, before the
  `toggle_extra` step:
  `assert!(api::update_extra(extra.id, "Copywork".to_string(), Category::Daily, None, "2099-01-01".to_string(), token.clone()).await.is_err(), "update_extra honours the same ±365 day window as add_extra");`.
