VERDICT: FAIL

# Fable QA — Round 3

**Auditor:** Fable 5, fresh context (no prior conversation), 2026-08-30.
**Scope:** `docs/PLAN.md` §0/§3/§5, `docs/reviews/PURPLE_TEAM.md` §P3/§P5.4/§P5.5, `docs/VERIFICATION.md`, `docs/BLOCKED.md`, `docs/NON_RUST.md`, `docs/HANDOFF.md`, `docs/RESIDUAL.md`, `docs/qa/QA_ROUND_2.md`, and the full diff `5769946..bed42f9` (129 files). Every Rust/SQL/config file under `src/`, `migrations/`, `.github/`, `Cargo.toml`, `xtask/` was read in full; the QA-round-2 fix commits `a8d5704..bed42f9` were diffed hunk by hunk against the round-2 solutions; the blocked branch `phase-qa2/T3.1` (`efbc749`) was diffed against its base.
**Tiers** are the *originally assigned* tier from PLAN §3 (H/S/O) of the task that owns the file being fixed.

## What was run

| Gate | Result |
| --- | --- |
| `cargo fmt --check` | exit 0 |
| `cargo clippy --features server --all-targets -- -D warnings` | exit 0 |
| `cargo clippy --features web --target wasm32-unknown-unknown -- -D warnings` | exit 0 |
| `cargo test --features server` | exit 0 — **401 passed, 0 failed, 1 ignored** (198 lib unit tests + 203 across 27 integration binaries; `realtime_tests` 17/17 in 132 s, `whiteboard_tests` 3/3 in 17 s, `profiles_tests` 6/6 in 12.5 s, `loop_tests` 1/1 in 6 s) |
| `cargo test --features server --test loop_tests` ×5, one process per run | **5 / 5 green** (6.1 s each) |
| `cargo tree -d --features server` | no duplicate `axum`/`tower-http`/`hyper`/`resvg`/`usvg`/`tiny-skia` |
| `cargo tree -e normal --features server` | 0 matches for `openssl-sys`/`native-tls` |
| Version pins | every §P5.4 crate `=`-pinned in the root `Cargo.toml`; `xtask/Cargo.toml` now `resvg =0.45.1`, `usvg =0.45.1`, `tiny-skia =0.11.4`, `image =0.25.10`; `Cargo.lock` carries exactly one `resvg`/`usvg`/`tiny-skia` tree (Q2-07 closed) |

Per-binary counts from this run, for cross-checking T3.3's transcript (Q3-03): `backup_tests` 11, `calendar_tests` 15, `ci_tests` 6, `config_tests` 4, `db_tests` 9, `docs_tests` 17, `health_pool_closed_tests` 1, `health_tests` 2, `http_tests` 14, `loop_tests` 1, `palette_tests` 6, `photo_tests` 7, `profiles_tests` 6, `pwa_tests` 16, `realtime_tests` 17, `router_tests` 12, `routine_tests` 10, `screensaver_tests` 5, `service_tests` 3, `storage_tests` 9 (+1 ignored), `tls_tests` 7, `tv_tests` 22, `whiteboard_tests` 3, lib unit tests 198.

### Acceptance spot checks executed by hand (beyond the suite)

All against `target/debug/family-hub.exe run` with `FAMILY_HUB_DATA_DIR` in a fresh scratch directory, `FAMILY_HUB_ADDR=127.0.0.1:18080`, `FAMILY_HUB_TLS_ADDR=127.0.0.1:18443`, `DIOXUS_PUBLIC_PATH` pointed at `target/dx/family-calendar/release/web/public`. Observed, not inferred.

1. **Q2-01 / T1.4** — with no client ever connecting, `<data>\setup-code.txt` existed (six digits) and `familyhub.log` held `generated the first-run parent PIN setup code` once `/health` answered 200. `GET /api/session` → **404** before any PIN. `POST /api/setup` with a wrong code → **401**, no `Set-Cookie`; with `Origin: http://evil.example` → **403**; with the real code → **200** `Set-Cookie: fh_session=…; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age=2592000` and `setup-code.txt` deleted; a second call → **409**. `GET /api/session` → **204** with the cookie, **401** without.
2. **Q2-02 / T2.2 / T2.5 / T2.4** — `POST /api/upload_photo` with the 12 MP fixture and **no** credential → **401**, `uploads/` empty; with the cookie **only** (no `auth` field) → **201** in 866 ms, stored `task-1-….jpg` 63 643 bytes; `x.svg` with the cookie → **415**, nothing written; `GET /uploads/<file>` → `content-type: image/jpeg`, `x-content-type-options: nosniff`, `content-disposition: attachment`. `POST /api/create_local_event` with `"auth": null` → **500** without the cookie, **200** (row id 1) with it. `POST /api/delete_custom_task` with `"auth": ""` → 500 `a valid parent session is required` without the cookie, **200** with it, and the photo file was removed from `uploads/`.
3. **Q1-11 / T1.2 / T1.3** — `GET /ws` with `Origin: http://evil.example` → **403**; same-origin upgrade → **101**. `GET /m` → **308** `https://127.0.0.1:18443/m`; `/manifest.webmanifest` and `/sw.js` → 308; `/tv` → **200**; `https://…:18443/m` → 200; `https://…:18443/manifest.webmanifest` → 200 `application/manifest+json`; `/ca.crt` → 200 `application/x-x509-ca-cert`.
4. **T1.7** — `/health` → 200 with all 8 keys: `{"db":true,"last_google_poll":null,"cert_not_after":"2027-10-01T14:47:07+00:00","days_to_expiry":396,"disk_free_bytes":370922283008,"ws_clients":0,"uptime_seconds":6,"migration_version":3}`.
5. **T1.4 backoff** — three wrong PINs after one wrong setup code were each answered 401 (≈512 ms each in the debug build: argon2id dominates the 4/8/16 ms schedule); the correct PIN then returned 200 with a fresh cookie — no lockout.
6. **T2.7** — `list_screensaver_images` → the three seeded placeholders; the first URL → 200 `image/jpeg` with `nosniff` + `attachment`.
7. **Q1-05 / T3.1** — the whole session's `familyhub.log` held 64 lines: 0 TRACE, 0 DEBUG, 64 INFO, 0 WARN, 0 ERROR.
8. **Round-2 Low** — `POST /api/logout` with a cross-site `Origin` and the cookie still answered 200 and revoked the session (unchanged, still Low).

## Round-2 findings — status

| Round 2 | Status | Evidence |
| --- | --- | --- |
| Q2-01 | **FIXED** | spot check 1; `router::run` calls `auth::ensure_setup_code` right after the pool opens (`src/server/router.rs:664`); `POST /api/setup` (`router.rs:263-296`) mirrors `/api/login`; `/api/session` 404 before a PIN (`router.rs:365-368`); `tests/service_tests.rs::run_generates_the_first_run_setup_code_and_logs_it_once_health_answers`; `tests/router_tests.rs::login_sets_a_well_formed_session_cookie` drives `/api/setup` (401/200/409) and the 404 probe; OWNER_CHECKLIST step 4 and RECOVERY mode 7 no longer promise a TV display; Boss amended PLAN T1.4 / PURPLE default 9 and created `docs/RESIDUAL.md` R-1 |
| Q2-02 | **FIXED** | spot check 2; `mobile/session.rs` is now `SessionState { FirstRun, SignedOut, Parent }` + `probe/login/setup/logout` over a `credentials: 'same-origin'` `inline_js` fetch, declared on the existing `docs/NON_RUST.md` row; `localStorage` token, `token()/store()/clear()` gone; `MobileShell` provides the context signal and probes once on mount; `settings.rs` renders `FirstRunForm` / `SignInForm` / Sign out with three SSR unit tests; `remote.rs`/`calendar.rs`/`routine.rs` send no bearer; `api::calendar::require_parent`, `api::photos::{require_parent_session, delete_custom_task}`, `api::screensaver::upload_screensaver_image_handler` accept the cookie; `photo_tests::t2_5_a` cookie-only, `t2_5_g` still the no-credential 401; `calendar_tests::q2_02_a_*`; PWA.md updated; HANDOFF H-19/H-25 closed |
| Q2-03 | **FIXED** | `src/client/components/routine.rs:138` reads `bus.tasks_version` |
| Q2-04 | **OPEN** — never merged (`docs/BLOCKED.md` T3.1) | `src/server/service.rs:1159-1163` still only calls `handle.is_finished()` and logs `the server task ended unexpectedly`; the fix exists on the unmerged branch `phase-qa2/T3.1` (`efbc749`) — Q3-01 |
| Q2-05 | **OPEN** — never merged (`docs/BLOCKED.md` T3.1) | `src/server/service.rs:651` is still `level_from_env()`; `FamilyHubConfig` has no `log_level`; `docs/DEV_WINDOWS.md:168-177` and `docs/RECOVERY.md:120-123` still tell the owner to set `$env:FAMILY_HUB_LOG` before `install`/`start` — Q3-02 |
| Q2-06 | **OPEN** — rejected at Boss review, not re-dispatched (`docs/BLOCKED.md` T3.3) | `tests/docs_tests.rs:793-813` asserts only the count; `docs/VERIFICATION.md` unchanged since `54180f6`: no transcript, "Total tasks verified: 28" for 27 rows, and the `loop_tests` "20 / 20" table still contradicted by the "residual failure rate … runs 8-11 and 9-12" paragraph — Q3-03 |
| Q2-07 | **FIXED** | pins table above; `tests/ci_tests.rs::xtask_crate_versions_are_pinned_exactly`; no PNG changed in the diff (icons byte-identical, as HANDOFF records) |

## Findings

PASS requires zero Critical/High/Med. There are **0 Critical, 0 High, 3 Med** — all three are round-2 findings that are still not on `main`. No new Med-or-above defect was found in the round-2 fix commits or in the rest of the tree; the new observations are all Low (end of this file).

| # | Task | Tier | File:line | Sev | Description | Solution (verbatim detail below) |
| --- | --- | --- | --- | --- | --- | --- |
| Q3-01 | T3.1 | S | `src/server/service.rs:1136,1155-1167` | Med | **Q2-04, unchanged.** Under the SCM the `JoinHandle` from `runtime.spawn(router::run(..))` is only ever polled with `is_finished()`; its `Result<(), RunError>` is never read, so the service log says `the server task ended unexpectedly` and the bind/DB/PKI reason (`RunError`'s `Display`) is lost on the one path that matters. PURPLE §P3 T3.1(c) is met by the `run` console path (`run_console` logs `%err`), not by the installed service. | Merge the already-written fix on `phase-qa2/T3.1` (`efbc749`), rebased onto `main`; the exact hunks are restated below in case the branch is lost. |
| Q3-02 | T3.1 | S | `src/server/service.rs:647-664,700-721,938-940`; `src/server/config.rs:50-87`; `docs/DEV_WINDOWS.md:168-177`; `docs/RECOVERY.md:16,120-123` | Med | **Q2-05, unchanged.** §P5.5 default 33 (`debug` behind `FAMILY_HUB_LOG`) is read only from the process environment (`level_from_env`). A service started by the SCM inherits the machine environment, not the owner's PowerShell, so on the deployed path the level cannot be changed at all; both runbooks still describe `$env:FAMILY_HUB_LOG = "debug"` "before `install`/`start`", which does nothing. | Same branch as Q3-01 (`efbc749` adds `FamilyHubConfig::log_level`, `service::level_from`, `ServiceLogger::open(dir, level)` and corrects both runbooks); restated below. |
| Q3-03 | T3.3 | H | `tests/docs_tests.rs:793-813`; `docs/VERIFICATION.md:47,105-165` | Med | **Q2-06 / Q1-16, unchanged.** The T3.3 contract is "one row per task ID with a `PASS`/`FAIL` **and a command transcript**; a `#[test]` asserts every task ID appears exactly once **and none is `FAIL`**". The test still asserts only the count; the document has no per-task transcript or timings, says "Total tasks verified: 28" for a 27-row table, and still contradicts itself on `loop_tests` (a 20/20 table followed by a "residual failure rate … runs 8-11 and 9-12" paragraph). Two earlier branches were rejected for fabricated transcripts, so the acceptance bar is a *pasted* transcript. | Add the two assertions, regenerate `## Transcripts` from real per-binary runs with the exact procedure below, fix the total, and replace the `loop_tests` paragraph with the one true count. |

## Solutions

### Q3-01 and Q3-02 — land `phase-qa2/T3.1` (`efbc749`) on `main`

The blocked branch already implements both findings correctly (reviewed hunk by hunk this round). The fixing agent (Sonnet, T3.1's tier; per `docs/BLOCKED.md` the re-dispatch may be at Opus) must **not** restart from `main`:

1. `git switch phase-qa2/T3.1` (worktree `wf_d57bfb45-d60-34`, or a fresh one) and `git rebase main` (`a8d5704..bed42f9`). Expected conflicts and how to take them:
   - `src/server/service.rs`, test `install_refuses_when_no_wasm_bundle_is_present_beside_the_executable`: **keep `main`'s** `let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());` line (Boss `1f8559d`). `efbc749` drops `ENV_LOCK` only from the three Q1-05 logger tests; `ENV_LOCK` itself stays (the `tv_probe_*` tests and the no-bundle test still hold it).
   - `tests/router_tests.rs` and `tests/photo_tests.rs`: take **both** sides — `efbc749` adds one `log_level: None,` line to each `test_config()`/`spawn_test_server()` literal; `main`'s round-2 hunks (`/api/setup` assertions, `post_multipart_with_cookie`) are elsewhere in the files.
   - `tests/calendar_tests.rs::spawn_http_server` (new on `main`, `tests/calendar_tests.rs:1578-1583`): add `log_level: None,` after `screensaver_schedule_hour: None,` or the crate does not compile.
2. Re-run all four gates (`cargo fmt --check`; both clippies with `-D warnings`; `cargo test --features server`) and additionally `cargo test --features server --test service_tests` and `cargo test --features server service::tests::describe_server_exit_surfaces_the_run_error_text`, and paste the `test result:` lines into `docs/HANDOFF.md` under a "T3.1 — QA round 3" heading. Boss squash-merges and updates `docs/BLOCKED.md` (T3.1 → RESOLVED).

If the branch is unavailable, the hunks to apply are exactly these (from `efbc749`):

`src/server/config.rs` — a new key, resolved like every other one (env wins over file), logged at startup:
```rust
const ENV_LOG_LEVEL: &str = "FAMILY_HUB_LOG";
// in FamilyHubConfig:
    pub log_level: Option<String>,
// in from_sources, after screensaver_schedule_hour:
        let log_level = env.var(ENV_LOG_LEVEL).or_else(|| file.get("log.level"));
// in ensure_dirs_and_log, after the screensaver match:
        match &self.log_level {
            Some(level) => tracing::info!(log_level = %level, "resolved log level"),
            None => tracing::info!("resolved log level: info (no FAMILY_HUB_LOG / [log] level configured)"),
        }
```
plus `log_level: None,` in every `FamilyHubConfig { .. }` literal (`src/server/config.rs` tests ×2, `tests/{calendar,health,health_pool_closed,http,photo,profiles,pwa,router,routine,screensaver,tls}_tests.rs`), and three unit tests (`log_level_defaults_to_none`, `env_log_level_overrides_file_and_default` with `[log]\nlevel = "warn"` + env `debug` → `Some("debug")`, `file_log_level_is_used_when_env_is_unset`).

`src/server/service.rs`:
```rust
pub fn level_from(raw: Option<&str>) -> tracing::Level {
    match raw.map(str::to_ascii_lowercase).as_deref() {
        Some("trace") => tracing::Level::TRACE,
        Some("debug") => tracing::Level::DEBUG,
        Some("warn") => tracing::Level::WARN,
        Some("error") => tracing::Level::ERROR,
        _ => tracing::Level::INFO,
    }
}
// ServiceLogger::open takes the level instead of reading the environment:
    pub fn open(log_dir: &Path, max_level: tracing::Level) -> io::Result<Self> { /* ... max_level, not level_from_env() */ }
// install_global_logger:
    let logger = std::sync::Arc::new(ServiceLogger::open(
        &config.log_dir(),
        level_from(config.log_level.as_deref()),
    )?);

/// Q2-04: the text logged before the service reports `Stopped`.
fn describe_server_exit(
    result: Result<Result<(), crate::server::router::RunError>, tokio::task::JoinError>,
) -> String {
    match result {
        Ok(Ok(())) => "the server task returned without an error".to_string(),
        Ok(Err(err)) => format!("startup failed: {err}"),
        Err(join) => format!("the server task panicked: {join}"),
    }
}
// scm::run_service:
        let mut handle =
            runtime.spawn(async move { crate::server::router::run(config_for_run).await });
        // ...
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) if handle.is_finished() => {
                    let reason = describe_server_exit(runtime.block_on(&mut handle));
                    tracing::error!(%reason, "FamilyHub service: the server task ended unexpectedly");
                    exit_code = ServiceExitCode::ServiceSpecific(1);
                    break;
                }
```
The three Q1-05 logger tests call `ServiceLogger::open(&dir, level_from(None))` / `level_from(Some("debug"))` instead of setting `FAMILY_HUB_LOG`; add `describe_server_exit_surfaces_the_run_error_text` (a `RunError::Bind { addr: "127.0.0.1:8080", err: AddrInUse }` must render `failed to bind 127.0.0.1:8080`) and `describe_server_exit_reports_a_clean_return_as_unexpected`.

Docs (`efbc749` wording): `docs/DEV_WINDOWS.md` "Log level" → "For `family-hub.exe run` set `$env:FAMILY_HUB_LOG = "debug"` in that shell. For the **installed service** the shell's environment is not inherited — put `[log]\nlevel = "debug"` in `familyhub.toml` next to `family-hub.exe` (or under the data directory), then `family-hub.exe stop` and `start`." `docs/RECOVERY.md` mode 2 gets the same two sentences and the "four things worth knowing" table gains a `Log level` row.

### Q3-03 — T3.3's contract, with a real transcript (Q2-06 restated; Sonnet per `docs/BLOCKED.md`'s escalation)

1. `tests/docs_tests.rs::t3_3_every_task_id_appears_exactly_once_in_verification` — inside the loop, after the count assertion:
   ```rust
        let row = verification_content
            .lines()
            .find(|line| line.starts_with(&pattern))
            .unwrap_or_else(|| panic!("no table row starts with {pattern:?}"));
        assert!(
            row.contains("| PASS |"),
            "task {task_id} is not PASS in docs/VERIFICATION.md: {row}"
        );
   ```
   and after the loop:
   ```rust
    assert!(
        !verification_content.contains("| FAIL |"),
        "docs/VERIFICATION.md must not contain a FAIL row"
    );
    assert!(
        verification_content.contains("## Transcripts"),
        "docs/VERIFICATION.md must carry the per-task command transcript T3.3's contract names"
    );
   ```
2. `docs/VERIFICATION.md` — fix line 47 to **27** tasks; add a `## Transcripts` section generated **only** by this PowerShell loop (paste the captured files verbatim, never reconstruct from source):
   ```powershell
   $bins = 'backup_tests','calendar_tests','ci_tests','config_tests','db_tests','docs_tests','health_pool_closed_tests','health_tests','http_tests','loop_tests','palette_tests','photo_tests','profiles_tests','pwa_tests','realtime_tests','router_tests','routine_tests','screensaver_tests','service_tests','storage_tests','tls_tests','tv_tests','whiteboard_tests'
   foreach ($b in $bins) { cargo test --features server --test $b 2>&1 | Tee-Object "$env:TEMP\qa3-$b.log" }
   ```
   One `### Transcript — <binary>` heading per binary (never a `| T1.2 |` table-row form, which the exactly-once test would count), each containing the command and the pasted `Running …` / `test …` / `test result:` lines. Map binaries to task IDs in prose above the blocks using the table already in "Test Execution Details". The Boss will (a) grep every `test <name> ... ok` line against `tests/*.rs`/`src/**` and (b) check each block's name count equals its `test result:` count — this round's counts are in "What was run" above.
3. Replace both the "20 / 20" table and the "still shows a residual failure rate … runs 8-11 and 9-12" paragraph with one sentence stating the count from a fresh `cargo test --features server --test loop_tests` ×20 (one process per run; this audit's ×5 and round 2's ×10 were all green — if ×20 is also 20/20, say so and delete the "residual" paragraph outright).
4. Re-run `cargo test --features server --test docs_tests` and paste its `test result:` line into `docs/HANDOFF.md`.

## Low observations (not blocking, no action required this round)

- **New — `src/server/backup.rs:526-529` (T1.6) vs `src/server/service.rs:745-776` (T3.1):** two independent rotators over one open handle. `nightly_maintenance` renames `familyhub.log` → `.1` while `ServiceLogger` holds it open (Rust opens with `FILE_SHARE_DELETE`, so the rename succeeds — the logger's own rotation relies on that); if the nightly check ever wins (the on-disk size crosses 10 MB on a flush from another task between the logger's own pre-write check and the nightly call), every later line lands in `familyhub.log.1`, which then grows unbounded, and `familyhub.log` stays empty until restart. The window is a few milliseconds once per ~10 MB of logs, so Low. Fix: delete the `rotate_log_if_needed` step from `backup::nightly_maintenance` (the logger already rotates at open and on every append) and drop item 5 from `backup.rs`'s module doc; or, in `ServiceLogger::append_line`, after `rotate_log_if_needed` returns `Ok(false)`, reopen when `file.writer.get_ref().metadata().map(|m| m.len()).unwrap_or(0) > std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0)`.
- **New — `src/server/api/realtime.rs:685-693`:** `next_midnight` falls back to `now + 24 h` when local midnight does not exist (`.earliest()` → `None`); `db::next_local_midnight` already walks forward minute by minute for exactly that case. No US/UK zone skips midnight, so no impact for this household; swapping the fallback for `db::next_local_midnight` would make the two helpers agree.
- **New — `src/server/api/photos.rs:179-186`:** `due_date` is stored unvalidated; `db::custom_tasks` compares it as a string against `YYYY-MM-DD`, so a malformed value from a non-browser caller never auto-hides. The phone's `<input type="date">` always sends the right shape.
- **New — `src/server/service.rs:1021-1034`:** `tv_probe` reads `docs/device.toml` relative to the CWD first (a relative path literal in `src/`); the `tv-probe` subcommand is a developer tool and falls back to the exe directory, so this does not affect the service path.
- Carried from round 2, unchanged: `POST /api/logout` has no `same_origin_or_absent` check (spot check 8; `SameSite=Lax` keeps a real browser from attaching the cookie); `api::whiteboard::undo_last_stroke(client_id)` trusts the caller-supplied `client_id`; `realtime::snapshot`'s contiguous `latest` bookmark never advances past an undone/compacted `seq`; `docs/RECOVERY.md:65` still says a missing `public\` gives "unstyled text" (it gives a styled but inert kiosk since Q1-01); `docs/DEV_WINDOWS.md:82` verifies Tailwind with `--version` (the standalone binary exits 9; `ci.yml` uses `--help`); `db::hard_reset_board`'s doc comment is stale after Q1-09; the panic hook in `install_global_logger` re-enters `ServiceLogger`'s `Mutex` if a panic is raised while it is held; `/tv` on the HTTP origin still links `/manifest.webmanifest` (one 308 per kiosk load).
