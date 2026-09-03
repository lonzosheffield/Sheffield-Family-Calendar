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

## R-4. The full suite is flaky under parallel load (pre-existing; four distinct failures catalogued)

- **Origin:** HANDOFF H-HS3-5 (wave A), the HS5-qa1 / HS7-qa1 notes at the QA round 1 close,
  and HS1-qa2's five-run study at the QA round 2 close (reproduced on `main` @ `0889f16`, so
  not introduced by any HS branch). Each binary passes 4–5/5 in isolation; failures appear
  only when `cargo test --features server` runs its ~33 binaries concurrently, worse while
  another cargo build or a second full suite is running on the box.
- **What ships:** the eight Boss baselines of the two QA closes (2026-09-03) were all green
  after wiping `%TEMP%\familyhub-*` first — the round 2 close found 41 leftover scratch dirs
  (HS1-qa2 found 3478 the day before), each a pid-keyed directory some earlier run never
  removed (`838e62f` wipes them on reuse, not on exit).
- **The four failures seen, with the mechanism and the fix each wants:**
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
