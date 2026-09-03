# BLOCKED

Per PLAN §5 / PURPLE §P5.1: a task lands here when its branch is halted. Boss
reviews this file between waves and either re-scopes (a new task in the next
wave) or accepts the gap into `docs/RESIDUAL.md`.

---

## T0.7 — QA round 1 fix Q1-17 (exact pins in the xtask)

> **RESOLVED** at the QA round 2 close: re-dispatched as `phase-qa2/T0.7`
> (Q2-07 restates Q1-17 verbatim), squash-merged to `main` as `b2aa91b`. One
> resvg/usvg tree in `Cargo.lock`, exact pins in `xtask/Cargo.toml` and the
> root dev-deps, `ci_tests::xtask_crate_versions_are_pinned_exactly` added.

- **Task:** T0.7 (`assets/**`, `xtask/**` owner), applying `docs/qa/QA_ROUND_1.md`
  Q1-17: pin `resvg`/`usvg`/`tiny-skia`/`image` with `=` in `xtask/Cargo.toml`
  and `resvg`/`rqrr` in the root dev-deps, collapse the lockfile to one
  resvg/usvg tree, regenerate the icons, and add the `tests/ci_tests.rs`
  assertion that every `resvg`/`usvg`/`tiny-skia` line in `xtask/Cargo.toml`
  contains `"=`.
- **Tier reached:** S (Sonnet), three dispatch attempts. **No agent ever ran**
  — the branch `phase-qa1/T0.7` was never created and no code was written.
- **Failing assertion:** none of the task's own. All three attempts failed in
  the harness before the agent started, with the same runtime error:

  > Refusing to use `C:\Family Calendar\.claude\worktrees\wf_d57bfb45-d60-25`
  > (then `-26`, then `-28`) as an isolation worktree: git could not be run to
  > resolve it, so its git identity could not be verified. Isolation is refused
  > rather than assumed — recreate the worktree (or remove the corrupt .git
  > entry) and retry.

- **Hypothesis (Boss, 2026-08-30):** the three placeholder worktrees were
  created at `54180f6` on locked `worktree-wf_d57bfb45-d60-{25,26,28}`
  branches while nine other agents were building in sibling worktrees. Their
  `.git` files resolved correctly from the main checkout afterwards
  (`gitdir: C:/Family Calendar/.git/worktrees/...`), so the entries were not
  actually corrupt — the harness most likely hit a transient `git` failure
  (index/worktree lock contention or a timeout under full CPU load) at spawn
  time and, correctly, refused to assume isolation. Boss has removed all four
  stale placeholder worktrees and their `worktree-*` branches so a fresh
  dispatch cannot land on one of them.
- **What is needed:** re-dispatch T0.7 at Sonnet in the next QA wave with a
  freshly created worktree (ideally not concurrently with a full-suite run),
  applying Q1-17 exactly as written. `Cargo.toml` is a serialized Boss file
  per PURPLE §P4, so that agent must be the only one in its wave touching
  `Cargo.toml`/`Cargo.lock`. Until it lands, Q1-17 stays open and
  `docs/qa/QA_ROUND_2.md` must not mark it FIXED.

---

## T3.1 — QA round 2 fixes Q2-04 / Q2-05 (service exit reason, `log.level`) — RESOLVED

> **Resolved 2026-08-30 (Boss):** re-dispatched as QA round 3 Q3-01/Q3-02 (Sonnet, `phase-qa3/T3.1`),
> squash-merged to `main` as `f6fce23`; baseline green (203 lib + 27 integration binaries, 0 failed).
> The entry below is kept for provenance.

- **Task:** T3.1 (`src/server/service.rs`, `src/server/config.rs` owner),
  applying `docs/qa/QA_ROUND_2.md` Q2-04 (read the server task's own
  `Result<(), RunError>` under the SCM and log it at ERROR before reporting
  `Stopped`) and Q2-05 (`log.level` in `familyhub.toml` →
  `FamilyHubConfig::log_level` → `ServiceLogger::open(dir, level)`, env still
  wins; correct `docs/DEV_WINDOWS.md` and `docs/RECOVERY.md`).
- **Tier reached:** O (Opus). Status **BLOCKED**, not FAIL: the branch
  `phase-qa2/T3.1` exists with one commit (`efbc749`, 14 files, +185/-36) that
  appears to implement both findings, but the agent could not finish its
  DONE criteria.
- **Failing assertions / what did not run:**
  - `cargo clippy --features web --target wasm32-unknown-unknown -- -D warnings`
    did not finish;
  - `cargo test --features server` was not run;
  - T3.1's own acceptance (PURPLE §P3 T3.1 b/c/e and the mocked
    install/uninstall tests) was not re-executed or captured.
  - Harness runtime errors on the retry: "Could not read the repository git
    config to neutralize filter drivers" and "Refusing to use
    `C:\Family Calendar\.claude\worktrees\wf_d57bfb45-d60-38` as an isolation
    worktree: git could not be run to resolve it, so its git identity could
    not be verified."
- **Hypothesis (Boss, 2026-08-30):** the same shape as T0.7's QA round 1
  block above. The first attempt ran in `wf_d57bfb45-d60-34` (still checked
  out on `phase-qa2/T3.1`) and committed `efbc749`; the retry landed on the
  locked `worktree-wf_d57bfb45-d60-38` placeholder while four sibling agents
  were compiling, and the harness — correctly — refused to assume isolation
  when `git` could not be run against it. The unfinished wasm clippy is most
  likely the same CPU/memory contention (T2.5 saw page-file exhaustion in the
  same wave), not a code error, but that is unverified. Boss removed the `-38`
  placeholder worktree and branch at this close.
- **What is needed:** re-dispatch T3.1 at Opus in the next QA wave **onto the
  existing `phase-qa2/T3.1` branch** (do not restart from `main`), in a
  freshly created worktree, not concurrently with a full-suite run. It must
  first rebase/merge over the QA round 2 `main` (`T1.4`, `T2.2`, `T2.5`,
  `T0.7`, and the Boss `ENV_LOCK` edit in `service.rs`'s no-bundle install
  test): note that `tests/calendar_tests.rs::spawn_http_server` (T2.2) now
  builds a `FamilyHubConfig` literal and will need the new `log_level: None`
  field. Then run all four gates and re-execute T3.1's acceptance, and record
  the transcript in `docs/HANDOFF.md`. Until it lands, Q2-04 and Q2-05 stay
  open and the next QA round must not mark them FIXED.

---

## T3.3 — QA round 2 fix Q2-06 (real transcript in `docs/VERIFICATION.md`) — REJECTED at Boss review — RESOLVED

> **Resolved 2026-08-30 (Boss):** after two Haiku attempts were rejected for fabricated transcripts
> (round 1 and round 2), the finding was escalated per PLAN §5 to Sonnet (QA round 3 Q3-03,
> `phase-qa3/T3.3-sonnet` @ `f16b002`), which pasted per-binary transcripts from teed logs;
> squash-merged to `main` as `3422ca2`. The entry below is kept for provenance.

- **Task:** T3.3 (`docs/VERIFICATION.md`, `tests/docs_tests.rs::t3_3_*`
  owner), applying `docs/qa/QA_ROUND_2.md` Q2-06: add the `| PASS |` / no
  `| FAIL |` assertions, regenerate a `## Transcripts` section **pasted from
  a fresh run**, and resolve the `loop_tests` contradiction with a ×20 count.
- **Tier reached:** H (Haiku). Reported PASS on `phase-qa2/T3.3` (`421932c`);
  **not merged**. The `docs_tests.rs` assertions and the `loop_tests`
  resolution are fine; the transcript is not a transcript.
- **Failing assertion (Boss review, `docs/qa/QA_ROUND_2.md` Q2-06: "the
  replacement must paste real `test result:` lines" / "The Boss will grep
  every named test against the tree"):**
  - `### Transcript — T2.4` lists
    `test a_dst_week_has_exactly_seven_days_with_correct_boundaries ... ok`,
    a test that does not exist anywhere in the tree (the real one is
    `t2_4_c_a_dst_week_has_exactly_seven_days_with_correct_boundaries`, which
    is also listed) — 15 names against `14 passed`;
  - `### Transcript — T0.6` lists 14 names against `12 passed`, two of them
    (`public_bundle_present_*`) `src/server/router.rs` unit tests that
    `cargo test --test router_tests` never prints;
  - `### Transcript — T1.6` lists 12 names against `11 passed`, one of them
    (`custom_task_stores_the_given_path_and_the_file_remains`) from
    `tests/db_tests.rs`, not `backup_tests`.
  Three blocks were assembled by hand around real-looking `test result:`
  lines — the same defect the QA round 1 `phase-qa1/T3.3` branch was
  rejected for (119 fabricated names), at smaller scale.
- **Hypothesis (Boss):** the agent ran the suite once and then reconstructed
  the per-binary blocks from the source files instead of splitting the
  captured output per binary.
- **What is needed:** re-dispatch T3.3 — at **S (Sonnet)**, escalated per §5
  after two Haiku attempts — with the instruction to run
  `cargo test --features server --test <file> 2>&1 | Tee-Object <file>.log`
  once per binary and paste each log verbatim, and to include the commands
  and the captured `test result:` lines only. Keep the `docs_tests.rs` `PASS`
  assertions from `421932c` (they are correct as written). The Boss will
  re-run the name-vs-tree grep and the per-block count check at review.

---

## HS1-qa1 — QA round 1 fix QH1-08 (`import-curriculum` opens the pool before copying the file)

- **Task:** HS1-qa1 (`src/server/homeschool/loader.rs`, `tests/service_tests.rs` owner),
  applying `docs/qa/QA_HS_ROUND_1.md` QH1-08: move `db::pool().await` above the
  `std::fs::copy` in `import_curriculum` so the command's own `insert_missing` report is
  truthful on a first import, and add the
  `"7 subjects, 9 assignments, 3 term notes inserted"` assertion to
  `service_tests::import_curriculum_copies_a_valid_file_into_the_curricula_directory`.
- **Tier reached:** O (Opus). The code change **exists**, unverified: branch `hs/HS1-qa1`
  @ `076367f` (worktree `.claude/worktrees/wf_0b6fd056-c15-21`, 2 files, +18/−4), matching
  the audit's prescription on a read-through. It is **not merged**.
- **Failing assertion:** none known. The DONE criterion `cargo test --features server`
  green was never executed to completion, so HS1's Accept clauses (a)–(i) and the new
  `service_tests` assertion were not re-run on this machine. The agent's second attempt
  died in the harness before starting:

  > Refusing to use `C:\Family Calendar\.claude\worktrees\wf_0b6fd056-c15-22` as an
  > isolation worktree: git could not be run to resolve it, so its git identity could not
  > be verified. Isolation is refused rather than assumed — recreate the worktree (or
  > remove the corrupt .git entry) and retry.

- **Hypothesis (Boss):** the same harness worktree-isolation refusal T0.7 hit three times
  (above); the `-22` worktree was a plain checkout of `c7c1339` with no commits of its own
  and resolved fine from the main checkout at the close, so the refusal is a race in the
  harness's own verification, not a corrupt repository. Removed at this close
  (`git worktree remove --force`); `-21` (the branch's real worktree) is kept.
- **What is needed:** re-dispatch HS1-qa1 at O, starting from `hs/HS1-qa1` @ `076367f`
  rather than from scratch, with the single instruction to run the DONE baseline (fmt,
  both clippy gates, `cargo test --features server` twice consecutively) and HS1's Accept
  list, and to report the `service_tests` transcript. Boss reviews the diff against
  QH1-08 verbatim and squash-merges. Until then QH1-08's console misreport stands; it is
  cosmetic (the rows are correct on disk).
