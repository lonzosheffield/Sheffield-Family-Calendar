# BLOCKED

Per PLAN §5 / PURPLE §P5.1: a task lands here when its branch is halted. Boss
reviews this file between waves and either re-scopes (a new task in the next
wave) or accepts the gap into `docs/RESIDUAL.md`.

---

## T0.7 — QA round 1 fix Q1-17 (exact pins in the xtask)

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
