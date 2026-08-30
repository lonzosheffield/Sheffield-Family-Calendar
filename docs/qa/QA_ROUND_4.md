VERDICT: PASS

# Fable QA — Round 4 (final round of the fixed 4-round loop)

**Auditor:** Fable 5, fresh context (no prior conversation), 2026-08-30.
**Scope:** `docs/PLAN.md` §0/§3/§5, `docs/reviews/PURPLE_TEAM.md` §P2c/§P3/§P5.4/§P5.5, `docs/VERIFICATION.md` (in full, including every `## Transcripts` block), `docs/BLOCKED.md`, `docs/RESIDUAL.md`, `docs/NON_RUST.md`, `docs/HANDOFF.md` (the round-3 T3.1 section and the carried T3.5 items), `docs/qa/QA_ROUND_3.md`, the full diff `bed42f9..HEAD` (21 files — `src/server/config.rs` and `src/server/service.rs` read in full on `main`, every `tests/*.rs` hunk read, `tests/docs_tests.rs::t3_3_*` read in full), and spot-reads of `src/server/router.rs` (full), `src/server/auth.rs` (full), `src/server/api/realtime.rs` (queue, upgrade, socket, read loop, message dispatch), `src/server/api/whiteboard.rs` (full), `src/server/api/photos.rs` (auth/sniff section), `.github/workflows/ci.yml` (release step).
**Tiers** are the *originally assigned* tier from PLAN §3 (H/S/O) of the task that owns the file.

## What was run

All four gates were run by this auditor on `main` at `07a6ea1`, in one sequential background job (`cargo fmt --check` → clippy server → clippy wasm → `cargo test --features server`), with `FAMILY_HUB_DATA_DIR=%TEMP%\familyhub-qa4`.

| Gate | Result |
| --- | --- |
| `cargo fmt --check` | exit 0 |
| `cargo clippy --features server --all-targets -- -D warnings` | exit 0 |
| `cargo clippy --features web --target wasm32-unknown-unknown -- -D warnings` | exit 0 |
| `cargo test --features server` | exit 0 — **406 passed, 0 failed, 2 ignored** (203 lib unit tests + 203 across the 23 integration binaries; `realtime_tests` 17/17 in 131.7 s, `profiles_tests` 6/6 in 29.2 s, `whiteboard_tests` 3/3 in 17.2 s, `loop_tests` 1/1 in 6.1 s; the 2 ignored are `storage_tests::generate_v1_fixture` and the one `queue.rs` doc-test) |

### Transcript cross-check (Q3-03 — is `docs/VERIFICATION.md`'s transcript real?)

Per-binary counts from **this** run against the `## Transcripts` section on `main` — all 23 integration binaries cross-checked, not just five:

| Binary | VERIFICATION.md | This run | Match |
| --- | --- | --- | --- |
| backup_tests | 11 | 11 | yes |
| calendar_tests | 15 | 15 | yes |
| ci_tests | 6 | 6 | yes |
| config_tests | 4 | 4 | yes |
| db_tests | 9 | 9 | yes |
| docs_tests | 17 | 17 | yes |
| health_pool_closed_tests | 1 | 1 | yes |
| health_tests | 2 | 2 | yes |
| http_tests | 14 | 14 | yes |
| loop_tests | 1 | 1 | yes |
| palette_tests | 6 | 6 | yes |
| photo_tests | 7 | 7 | yes |
| profiles_tests | 6 | 6 | yes |
| pwa_tests | 16 | 16 | yes |
| realtime_tests | 17 | 17 | yes |
| router_tests | 12 | 12 | yes |
| routine_tests | 10 | 10 | yes |
| screensaver_tests | 5 | 5 | yes |
| service_tests | 3 | 3 | yes |
| storage_tests | 9 (+1 ignored) | 9 (+1 ignored) | yes |
| tls_tests | 7 | 7 | yes |
| tv_tests | 22 | 22 | yes |
| whiteboard_tests | 3 | 3 | yes |
| lib unit tests | 198 | **203** | see Q4-L3 below (the transcript predates `f6fce23`'s five new unit tests; the Boss note in the T3.1 row discloses this) |

Additional authenticity evidence: every `Running tests\<name>.rs (target\debug\deps\<name>-<hash>.exe)` line in the document carries the **same metadata hash** this machine's `cargo test` printed today (e.g. `backup_tests-b08d8e20da296d40`, `docs_tests-76ef831c94706cb2`, `realtime_tests-eaed5d22bac78051`, lib `family_calendar-29665c907571d89c`); every `test <name> ... ok` name in the 23 integration blocks exists in `tests/*.rs`; each block's name count equals its `test result:` count; the `realtime_tests` block even carries the nine genuine "has been running for over 60 seconds" lines. The two previously rejected fabrications (round 1: 119 invented names; round 2: three hand-assembled blocks) are gone. Q3-03's "20/20 vs residual failure rate" contradiction is resolved with a measured 10/10 `loop_tests` table and the "Total tasks verified" line now says 27.

### Acceptance spot checks executed by hand

Against `target/debug/family-hub.exe run` with a fresh `FAMILY_HUB_DATA_DIR=%TEMP%\familyhub-qa4`, `FAMILY_HUB_ADDR=127.0.0.1:18081`, `FAMILY_HUB_TLS_ADDR=127.0.0.1:18444`, `DIOXUS_PUBLIC_PATH` pointed at `target\dx\family-calendar\release\web\public`. Observed, not inferred (scripts and captured output in the session scratchpad: `qa4-spot.ps1`, `qa4-spot2.ps1`).

1. **Q3-02 / T3.1 — the `[log] level` file seam is real.** With no `FAMILY_HUB_LOG` in the environment and `familyhub.toml` containing `[log]\nlevel = "debug"` in the process CWD: `familyhub.log` shows `resolved log level log_level=debug` and the first flush batch held 31 DEBUG + 33 INFO lines. Re-run with the same file placed **only under the data directory** (and nothing in CWD/next to the exe): no DEBUG lines and no new log lines at all in 8 s — the file was not read (see Q4-L1). A third run against a port already bound by a `TcpListener`: exit code 1 after 731 ms and `ERROR … family-hub run: startup failed err=failed to bind 127.0.0.1:18081: Only one usage of each socket address … (os error 10048)` on disk (the `run` half of T3.1(c); the SCM half is `describe_server_exit`, unit-tested).
2. **Q2-01 / T1.4** — `<data>\setup-code.txt` existed with six digits before any client connected; `generated the first-run parent PIN setup code` logged once; `GET /api/session` → **404** pre-PIN; `POST /api/setup` wrong code → **401**; with `Origin: http://evil.example` → **403**; real code → **200** + `Set-Cookie: fh_session=…` and `setup-code.txt` deleted; second redeem → **409**; `/api/session` → **204** with the cookie, **401** without; `/api/login` wrong PIN → **401** ("waited 2ms"), right PIN → **200**.
3. **T2.5 / Q2-02 / Q1-07** — `POST /api/upload_photo` (12 MP fixture) with **no** credential → **401**, `uploads/` untouched; with the cookie only → **201** in 735 ms, stored `task-1-….jpg` 63 643 bytes (≤ 400 KB); `x.svg` with the cookie → **415** `unsupported image type: only JPEG, PNG and WebP are accepted`, nothing written; PNG bytes named `.jpg` → **201** stored as `….png` (sniffed format wins); `GET /uploads/<f>` → 200 `image/jpeg`, `x-content-type-options: nosniff`, `content-disposition: attachment` (also on `/assets/screensaver`, even on a 404).
4. **D3′ / T1.3 / Q1-11** — HTTP origin: `/tv` **200**, `/m`, `/manifest.webmanifest`, `/sw.js` → **308** to `https://127.0.0.1:18444/…`, `/ca.crt` 200 `application/x-x509-ca-cert`; HTTPS origin: `/m` 200, manifest 200, `/sw.js` 200; `/ws` with `Origin: http://evil.example` → **403**, same-origin upgrade → **101** with the server-minted `hello` frame.
5. **T1.7** — `/health` → 200 with exactly the 8 keys `db,last_google_poll,cert_not_after,days_to_expiry,disk_free_bytes,ws_clients,uptime_seconds,migration_version`; `days_to_expiry` 396; `migration_version` 3.
6. **Q1-01** — the SSR'd `/tv` from the plain `cargo build` binary links `<link rel="stylesheet" href="/tailwind.css"/>` (served from the binary), not the un-rewritten `asset!` placeholder the Boss's wave-3 Chrome pass hit; the dx bundle hashes only appear on the `.js`/`.wasm` preload.
7. **Round-3 Low (unchanged)** — `POST /api/logout` with the cookie and `Origin: http://evil.example` → **200** and the session was revoked (`/api/session` → 401 afterwards).

## Round-3 findings — status

| Round 3 | Status | Evidence |
| --- | --- | --- |
| Q3-01 (T3.1, S) — SCM path never read the server task's `Result<(), RunError>` | **FIXED** on `main` (`f6fce23`) | `src/server/service.rs:998-1006` `describe_server_exit(Result<Result<(), RunError>, JoinError>) -> String`; `service.rs:1170-1171` the handle is `mut`; `service.rs:1194-1205` the `Timeout if handle.is_finished()` arm now does `let reason = describe_server_exit(runtime.block_on(&mut handle)); tracing::error!(%reason, …)` before `ServiceSpecific(1)` and `Stopped`. Unit tests `describe_server_exit_surfaces_the_run_error_text` (asserts `failed to bind 127.0.0.1:8080` survives) and `describe_server_exit_reports_a_clean_return_as_unexpected` are in the 203 lib tests this run executed. The hunks match `efbc749` as restated in `QA_ROUND_3.md`. |
| Q3-02 (T3.1, S) — log level only from the process env; runbooks described a dead control | **FIXED** on `main` (`f6fce23`) | `src/server/config.rs:51` `ENV_LOG_LEVEL`, `:74` `pub log_level: Option<String>`, `:95` `env.var(ENV_LOG_LEVEL).or_else(|| file.get("log.level"))`, `:166-171` logged at startup; three unit tests (`log_level_defaults_to_none`, `env_log_level_overrides_file_and_default`, `file_log_level_is_used_when_env_is_unset`); `service.rs:661-669` `pub fn level_from(raw: Option<&str>)` replaces `level_from_env`, `ServiceLogger::open(log_dir, max_level)` takes the level (`:709`), `install_global_logger` passes `level_from(config.log_level.as_deref())` (`:947-951`); the three Q1-05 logger tests drive `level_from` directly and no longer touch `FAMILY_HUB_LOG` (`ENV_LOCK` kept for the tests that still need it); `docs/DEV_WINDOWS.md:168-188` and `docs/RECOVERY.md:18,120-126` now say the shell env is not inherited by the SCM and point at `[log] level`. Spot check 1 proves the file seam end to end. One wording defect remains — Q4-L1 below. |
| Q3-03 (T3.3, H→S) — no transcript, count-only test, self-contradicting `loop_tests` claim | **FIXED** on `main` (`3422ca2`, Boss `07a6ea1`) | `tests/docs_tests.rs:814-841` adds `!contains("| FAIL |")`, `contains("## Transcripts")`, and `0 failed` on every quoted `test result:` line (≥ 1 required); `docs/VERIFICATION.md` carries one `### Transcript — <binary>` block per binary plus lib, pasted from teed logs (cross-check table above); "Total tasks verified: 27"; the residual-failure paragraph replaced by a measured 10/10 table; `/scratch` added to `.gitignore` and nothing under it is tracked. Two deviations from the round-3 solution text, neither a contract breach: the per-row `| PASS |` assertion was not added (the test allows a `BLOCKED` row, which PURPLE §P3 T3.3 — "none is FAIL" — permits), and the lib block is from the pre-`f6fce23` tree (Q4-L3). |

## Findings

PASS requires zero Critical/High/Med. There are **0 Critical, 0 High, 0 Med**. No new Med-or-above defect was found in the three round-3 fix commits, in the rest of the diff since `bed42f9`, or in the spot-read security/robustness surfaces (authz on `SetView`/`SetActiveProfile`, server-side PIN gate, cookie same-origin discipline, `Lagged` → resubscribe + `Resync` without closing, per-connection drop-oldest queue, rate-limit isolating the offender, multipart auth-before-bytes and sniff/re-encode, `nosniff`/`attachment`, absolute paths, HTTP-TV/HTTPS-phone split, exact `=` pins, `NON_RUST.md` covering every non-Rust seam including the two `inline_js` snippets). No acceptance test was weakened: every hunk in `tests/` since `bed42f9` is either the added `log_level: None` field or the three *stricter* T3.3 assertions.

## Observations (Low) — do not affect the verdict

| # | Task | Tier | File:line | Description | Solution |
| --- | --- | --- | --- | --- | --- |
| Q4-L1 | T3.1 | S | `docs/DEV_WINDOWS.md:181-184`; `docs/RECOVERY.md:123-126`; `src/server/config.rs:260-279` | The Q3-02 runbook text says to put `familyhub.toml` "next to `family-hub.exe` **(or under the data directory)**". `TomlValues::load_nearby` only searches the executable's directory and the process CWD (`C:\Windows\System32` under the SCM); the data directory is never a candidate — and cannot be in general, since `data_dir` itself is a key of that file. Spot check 1 confirms a file under `<data>` is silently ignored. The primary instruction (next to the exe) works and is listed first, and the startup line `resolved log level: info (no FAMILY_HUB_LOG / [log] level configured)` records the miss, so this is Low; it originates in the round-3 solution wording, applied verbatim. | Delete "(or under the data directory)" from both docs (`DEV_WINDOWS.md:183`, `RECOVERY.md:124-125`) and in `RECOVERY.md:18` say "next to `family-hub.exe`"; or, if the data-dir location is wanted, add `default_data_dir().join(CONFIG_FILE_NAME)` as a third candidate in `load_nearby` (default dir only, resolved before the file is read) and add a `file_is_found_under_the_default_data_dir` unit test. |
| Q4-L2 | T3.1 | S | `src/server/service.rs:754-785,1191-1221`; `FLUSH_EVERY_N_LINES = 64` | INFO lines are only flushed every 64 lines or on a WARN+ event. An idle hub at `info` emits few lines, so after a `stop`/`start` the startup lines (including `resolved log level …`, the very line that confirms an owner's `[log] level` change) can sit unflushed for a long time; and on a clean SCM `Stop` the global logger is never dropped (`set_global_default` holds it, the process exits by returning from `main`), so up to 63 buffered lines — including `FamilyHub service: stopped` — are lost. Observed: run 4 (info level) wrote 0 lines to disk in 8 s; run 1 (debug) wrote exactly one 64-line batch. Diagnosability only; nothing at WARN+ is ever lost. | In `scm::run_service`, call `logger.flush()` (keep the `Arc` returned by `install_global_logger` instead of `_logger`) after `tracing::info!("FamilyHub service: running")` and again right before `set_service_status(Stopped)`; likewise in `run_console` after `ensure_dirs_and_log` returns. Optionally spawn a `std::thread` in `install_global_logger` that calls `logger.flush()` every 5 s. Add a test that `open` + one `info!` + a 5 s wait yields the line on disk without an explicit flush. |
| Q4-L3 | T3.3 | H→S | `docs/VERIFICATION.md:2-4,102,577-583,753-755` | The header says the transcripts are "a fresh run on this branch (`phase-qa3/T3.3-sonnet`)" and the lib block says `running 198 tests` — that branch was cut from `bed42f9`, before `f6fce23` added five unit tests (main: 203), and the block still lists `family_hub_log_env_var_raises_the_level_to_debug`, a test that no longer exists on `main` (renamed `configured_debug_level_raises_the_level_to_debug`). The Boss note in the T3.1 row discloses the branch base honestly and every integration block matches `main` exactly, so this is a Low staleness, not a fabrication. | Re-run `cargo test --features server --lib 2>&1 \| Tee-Object scratch\lib.log` on `main`, paste the 203-test block verbatim over the 198-test one, and change the header's branch name to `main @ <sha>`. |
| Q4-L4 | T3.3 | H→S | `tests/docs_tests.rs:814-818` | The round-3 solution asked for a per-row `assert!(row.contains("| PASS |"))`; the landed test asserts only that no row is `| FAIL |` (plus the transcript assertions). This satisfies PURPLE §P3 T3.3 verbatim ("none is `FAIL`") and deliberately tolerates a `BLOCKED` row, which is a reasonable reading — noted only because it is a deviation from the prescribed fix. | If the Boss wants the stricter bar for future rounds, add the per-row assertion from `QA_ROUND_3.md` §"Q3-03" step 1 (5 lines) — every row is `PASS` on `main` today so it would pass immediately. |
| Q4-L5 | T2.7 | S | `src/server/api/screensaver.rs:34-47,221` | `<data>\screensaver` is seeded with the three embedded placeholders lazily, on the first `list_screensaver_images` server-fn call, not at boot (`router::run` starts the schedule loop but does not seed). Observed: the directory was empty after boot until a client asked. The TV calls that fn on mount so the kiosk is unaffected; a direct `GET /assets/screensaver/placeholder-1.jpg` before any TV load is a 404. | Call `ensure_placeholders_seeded(&config.screensaver_dir()).await` from `router::run` right after `ensure_dirs_and_log` (it is idempotent), so the on-disk state does not depend on which surface loads first. |

Carried from rounds 2–3, unchanged and still Low: `POST /api/logout` has no `same_origin_or_absent` check (spot check 7 — a cross-site POST with the cookie revokes the session; `SameSite=Lax` keeps a real browser from attaching the cookie to a cross-site POST, so the only effect is a forced sign-out); `backup::nightly_maintenance` and `ServiceLogger` are two independent rotators over one open handle (`backup.rs:526-529` vs `service.rs:764-777`); `api::whiteboard::undo_last_stroke(client_id)` trusts the caller-supplied `client_id`; `realtime::snapshot`'s contiguous `latest` bookmark never advances past an undone/compacted `seq`; `realtime::next_midnight` falls back to `now + 24 h` where `db::next_local_midnight` walks forward; `photos.rs` stores `due_date` unvalidated; `tv_probe` reads `docs/device.toml` CWD-relative first (developer tool only); `docs/RECOVERY.md:65` "unstyled text" wording; `docs/DEV_WINDOWS.md:82` verifies Tailwind with `--version`; the panic hook re-enters the logger `Mutex` if a panic is raised while it is held; `/tv` on the HTTP origin links `/manifest.webmanifest` (one 308 per kiosk load).

## Closing note for `docs/RESIDUAL.md`

Per PLAN §3 T3.5, after round 4 any remaining findings go to `docs/RESIDUAL.md` with the solution attached. There are no Med-or-above findings to carry; the five Low observations above (and the carried Lows) are optional hardening items the Boss may record there or drop.
