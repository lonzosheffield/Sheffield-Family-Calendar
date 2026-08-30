VERDICT: FAIL

# Fable QA — Round 2

**Auditor:** Fable 5, fresh context (no prior conversation), 2026-08-30.
**Scope:** `docs/PLAN.md` §0/§3/§5, `docs/reviews/PURPLE_TEAM.md` §P3/§P5.4/§P5.5, `docs/VERIFICATION.md`, `docs/BLOCKED.md`, `docs/NON_RUST.md`, `docs/HANDOFF.md`, `docs/qa/QA_ROUND_1.md`, and the full diff `5769946..043bbe8` (127 files; every Rust/SQL/config/doc file read in full, the QA-round-1 fix commits `4a89075..043bbe8` diffed hunk by hunk).
**Tiers** are the *originally assigned* tier from PLAN §3 (H/S/O) of the task that owns the file being fixed.

## What was run

| Gate | Result |
| --- | --- |
| `cargo fmt --check` | exit 0 |
| `cargo clippy --features server --all-targets -- -D warnings` | exit 0 |
| `cargo clippy --features web --target wasm32-unknown-unknown -- -D warnings` | exit 0 |
| `cargo test --features server` | exit 0 — **391 passed, 0 failed** across the lib (191 unit tests) and 27 integration binaries; wall clock 3 min 19 s (`realtime_tests` 17/17 in 132 s, `whiteboard_tests` 3/3 in 17 s, `loop_tests` 1/1 in 6 s, `profiles_tests` 6/6 in 28 s) |
| `cargo test --features server --test loop_tests` ×10, one process per run | **10 / 10 green** (6 s each) — the Q1-09 flake did not reproduce; see Q2-06 for the doc that still says otherwise |
| Version pins | every §P5.4 crate in the root `Cargo.toml` is `=`-pinned as in round 1; `xtask/Cargo.toml` still floats (`resvg = "0.41"`, `usvg = "0.41"`, `tiny-skia = "0.11"`) and `Cargo.lock` still carries two resvg/usvg trees (0.41.0 + 0.45.1) — Q1-17, never dispatched (`docs/BLOCKED.md`) |

### Acceptance spot checks executed by hand (beyond the suite)

All against `target/debug/family-hub.exe run` with `FAMILY_HUB_DATA_DIR` in a scratch directory, `FAMILY_HUB_ADDR=127.0.0.1:18080`, `FAMILY_HUB_TLS_ADDR=127.0.0.1:18443`, `DIOXUS_PUBLIC_PATH` pointed at `target/dx/family-calendar/release/web/public`:

1. **Q1-01 / T3.1 / T0.6** — `family-hub.exe install` with an empty `target/debug/public` **refused** (`I/O error: … has no wasm client bundle; copy target/dx/family-calendar/release/web/public beside family-hub.exe first`, exit 1) before touching the SCM. `GET /tailwind.css` → 200 `text/css` 18 269 bytes. `GET /tv` → `<link rel="stylesheet" href="/tailwind.css"/>`, no manganis placeholder, the hashed `family-calendar-dxh….js` module script present. `GET /health` → 200 with all 8 keys (`migration_version: 3`, `days_to_expiry: 396`). `GET /m` → 308 `https://127.0.0.1:18443/m`; `/manifest.webmanifest` → 308; `/ca.crt` → 200 `application/x-x509-ca-cert`; `https://…:18443/m` → 200.
2. **Q1-07 / T2.5 / T2.7** — `POST /api/upload_photo` with no `auth` → **401**; with `auth=not-a-token` → **401**; `POST /api/upload_screensaver_image` with no `auth` → **401**; `uploads/` and `screensaver/` both still empty afterwards.
3. **Q1-11 / T1.4** — `GET /ws` upgrade with `Origin: http://evil.example` → **403**; with `Sec-Fetch-Site: cross-site` → **403**; same-origin → **101**. `POST /api/login` wrong PIN → 401 (after the backoff); correct PIN → 200 `Set-Cookie: fh_session=…; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age=2592000`; `GET /api/session` → 401 without / **204** with the cookie; cross-origin `/api/login` → 403.
4. **Q1-02 / Q1-03 / T1.4** — three wrong setup codes answered in 59/76/78 ms (≥ 2ⁿ ms each); the real code from `<data>\setup-code.txt` set the PIN and the file was deleted. **Eight parallel wrong `verify_parent_pin` calls** took 4 120 ms of wall clock with monotonically later answers (475 → 2 345 ms): serialised, not merely delayed. The correct PIN verified immediately afterwards (no lockout).
5. **Q1-13 / T1.2** — a `.NET ClientWebSocket` on `/ws` received `{"type":"hello",…}` then `{"type":"health","stale":false,"last_update":"2026-08-30T08:56:12…"}` 14 s later; `/health` showed `ws_clients: 1`.
6. **Q1-04 / T3.1** — a second `family-hub.exe run` on the occupied port exited **1 after 409 ms**; its `familyhub.log` holds `ERROR … family-hub run: startup failed err=failed to bind 127.0.0.1:18080: Only one usage of each socket address …`.
7. **Q1-05 / T3.1** — after the whole session above the hub's log held 64 lines: **0 TRACE, 0 DEBUG**, 64 INFO, 0 WARN.

## Round-1 findings — status

| Round 1 | Status | Evidence |
| --- | --- | --- |
| Q1-01 | **FIXED** | spot check 1; `tests/router_tests.rs::tailwind_css_is_served_from_the_binary_at_a_stable_url`, `service.rs::install_refuses_when_no_wasm_bundle_is_present_beside_the_executable`, CI assembles `dist/FamilyHub` and uploads it, three runbooks updated |
| Q1-02 | **FIXED** (code) — the "+ TV" clause is still undecided in PLAN, folded into Q2-01 | spot check 4; `set_initial_pin` shares `PIN_GATE` and the counter |
| Q1-03 | **FIXED** | spot check 4; `profiles_tests::eight_parallel_wrong_pins_are_serialised_not_just_individually_delayed` |
| Q1-04 | **FIXED** on the `run` path; the service path still drops the reason — Q2-04 | spot check 6; `RunError`, panic hook, SCM failure actions, `service_tests::a_startup_bind_failure_is_logged_within_five_seconds` |
| Q1-05 | **FIXED** in code; unreachable from an installed service — Q2-05 | spot check 7; `max_level_hint`, `BufWriter`, three unit tests |
| Q1-06 | **FIXED** | `create_photo_task`/`write_photo` gone; `insert_custom_task` stores a path |
| Q1-07 | **FIXED** | spot check 2; `t2_5_g_*`, screensaver twin, phone dialog gated on `is_parent()` |
| Q1-08 | **FIXED** | claim + write in one transaction; `client_nonce`; `routine_tests::t1_5_q1_08_*`; scratch dir wiped, keys salted with the PID |
| Q1-09 | **FIXED** | single ordered writer on its own thread; contiguous `latest`; suite green; see the `loop_tests` row above |
| Q1-10 | **FIXED** | `MAX_WS_MESSAGE_BYTES`, `valid_stroke`, six unit tests, two socket tests |
| Q1-11 | **HALF** — server side landed; the phone still holds a bearer token in `localStorage` and every call site still threads it (HANDOFF H-25) — Q2-02 | `mobile/session.rs` unchanged by design; `docs/PLAN.md` not amended |
| Q1-12 | **FIXED** | `TvModel.events: CalendarState<_>`, four arms rendered, `tv_tests::t2_4_e_*` |
| Q1-13 | **FIXED** | spot check 5; `qa1_13_*`; PROTOCOL §4.1 |
| Q1-14 | **FIXED** | `FAMILY_HUB_SCREENSAVER_HOUR` / `[screensaver] schedule_hour`, `view_after_activity`, pointer handlers gone, `screensaver.rs` inside the pointer-free scan |
| Q1-15 | **FIXED** | five substitutions, scan widened to `components/**`, red ramp in the table, `tailwind.css` rebuilt |
| Q1-16 | **OPEN** — the branch was rejected (fabricated transcript) and never re-dispatched — Q2-06 | `tests/docs_tests.rs::t3_3_*` still asserts only the count; `docs/VERIFICATION.md` has no transcript |
| Q1-17 | **OPEN** — never dispatched (`docs/BLOCKED.md`) — Q2-07 | `xtask/Cargo.toml` unchanged |

## Findings

PASS requires zero Critical/High/Med. There are **2 High and 5 Med**. Low observations are listed at the end and are not blocking.

| # | Task | Tier | File:line | Sev | Description | Solution (verbatim detail below) |
| --- | --- | --- | --- | --- | --- | --- |
| Q2-01 | T1.4 | S | `src/server/auth.rs:205-227`, `src/server/router.rs:575`, `src/server/api/profiles.rs:124-144`, `docs/OWNER_CHECKLIST.md:106-108`, `docs/RECOVERY.md:543-544`, `docs/PLAN.md:118` | **High** | **In production the first-run setup code is never generated and no parent session can ever be obtained.** `auth::ensure_setup_code` runs only from the `parent_setup_status` server fn, and *nothing in `src/client/` calls it* (`grep -rn "parent_setup_status\|set_initial_parent_pin" src/client/` is empty) — so `<data>\setup-code.txt` and the log line the checklist tells the owner to read never appear. Verified live: a fresh hub had no `setup-code.txt` until I `curl`ed `/api/parent_setup_status` by hand. `docs/OWNER_CHECKLIST.md` step 4 and `docs/RECOVERY.md` mode 7 also claim the code is "shown on the television", which the Boss decided against (HANDOFF "T2.1 H-24: decided — do not expose it to the kiosk") without the §5.2 PLAN commit Q1-02's solution required. Every parent-only function (TV remote, photo tasks, task delete, calendar CRUD) is therefore dead on a real install. | Generate the code at boot in `router::run`; add `POST /api/setup` (setup code + PIN → cookie) beside `/api/login`; fix the two doc sentences; Boss commit amending PLAN T1.4 / §5.9 default 9. |
| Q2-02 | T2.2 | O | `src/client/components/mobile/settings.rs:22-100`, `src/client/components/mobile/session.rs`, `src/client/components/mobile/remote.rs:53,73`, `src/client/components/calendar.rs:326,366`, `src/client/components/routine.rs:280,503`, `src/server/api/calendar.rs:90-93`, `src/server/api/photos.rs:98-102,396`, `src/server/api/screensaver.rs:132` | **High** | The phone has **no first-run form**: Settings offers only "Enter the six-digit parent PIN" → `verify_parent_pin`, which returns `PinNotSet` on a fresh hub, so the owner cannot set the initial PIN from any UI (OWNER_CHECKLIST step 4 cannot pass). Separately, the Q1-11 client half (HANDOFF H-25) is still undone: the session is a bearer UUID in `localStorage`, contrary to PLAN §3 T1.4 / §5.9 default 31, and the server's cookie is unused by the only client that exists. | Add the first-run form and migrate the phone to `/api/setup` + `/api/login` + `/api/session`; teach the four remaining server seams to accept the cookie. |
| Q2-03 | T2.5 | S | `src/client/components/routine.rs:137-140` | Med | The phone's custom-task list re-fetches on `bus.routine_version`, **not** `bus.tasks_version` — the only consumer of `ServerMessage::TasksUpdated` is the TV (`tv/shell.rs:187`). A task ticked/created/deleted from the television or the other phone never refreshes this phone's "Extra tasks" until something unrelated bumps the routine. G22/W1 ("phone ticks never reach the TV") is closed only in one direction. | One line: read `tasks_version` in that resource. |
| Q2-04 | T3.1 | S | `src/server/service.rs:1136-1167` | Med | Under the SCM the `JoinHandle` returned by `runtime.spawn(router::run(..))` is polled only with `is_finished()`; its `Result<(), RunError>` is never read, so the service log says only `the server task ended unexpectedly` — the bind/DB/PKI reason Q1-04 was about is lost exactly where it matters (the `run` console path logs it, the service path does not). PURPLE §P3 T3.1(c) "a deliberate startup failure appears in the log file" is met by the CLI, not by the service. | `block_on(handle)` after the loop and log the `RunError`/panic at ERROR before reporting `Stopped`. |
| Q2-05 | T3.1 | S | `src/server/service.rs:651-664`, `docs/DEV_WINDOWS.md:168-177`, `docs/RECOVERY.md:381-384` | Med | §P5.5 default 33 ("`debug` behind `FAMILY_HUB_LOG`") is implemented as a process env var only, and both runbooks tell the owner to `$env:FAMILY_HUB_LOG = "debug"` "before `install`/`start`". A service started by the SCM inherits **the system environment, not the owner's shell**, so on the deployed path the level cannot be changed at all without a machine-wide `setx /M` and a restart — the docs describe something that does not work. | Add `log.level` to `familyhub.toml` (`FamilyHubConfig::log_level`, env still wins) and pass it into `ServiceLogger::open`; correct the two runbooks. |
| Q2-06 | T3.3 | H | `tests/docs_tests.rs:793-813`, `docs/VERIFICATION.md:138-165` | Med | Q1-16 is still open: the T3.3 test asserts each task ID appears once but not that none is `FAIL`; `docs/VERIFICATION.md` has no per-task command transcript or timings (contract: "command transcript", "per-task pass/fail and timings"). The doc also **contradicts itself** on `loop_tests`: it reports 20/20 green in the Q1-09 table and, two paragraphs later, "still shows a residual failure rate … runs 8-11 and 9-12". The rejected `phase-qa1/T3.3` branch fabricated 119 test names; the replacement must paste real `test result:` lines. | Add the PASS assertions; regenerate a `## Transcripts` section from a real run (the binaries and counts from this audit's run are listed below for cross-checking); resolve the `loop_tests` paragraph with a fresh ×20 count. |
| Q2-07 | T0.7 | H | `xtask/Cargo.toml:7-9`, `Cargo.toml` dev-deps `resvg`/`rqrr`, `Cargo.lock:4113/4130, 5562/5589` | Med | Q1-17 is still open (never dispatched — `docs/BLOCKED.md`): §P5.4 says `resvg`/`tiny-skia` are "pinned exactly in `Cargo.toml` by T0.7"; they are caret ranges and the lockfile carries two resvg/usvg trees. | Exact pins, one lock tree, regenerated icons, a `ci_tests` assertion — exactly Q1-17's text, restated below. |

## Solutions

### Q2-01 — the setup code exists from the first boot, and a phone can turn it into a cookie session

1. **`src/server/router.rs::run`**, immediately after `crate::server::db::pool().await.map_err(RunError::Db)?;`:
   ```rust
   // First-run parent-PIN setup code (T1.4, PURPLE §P5.5 default 9): generated
   // here, at boot, so the log line and `<data>\setup-code.txt` the owner
   // checklist tells the parent to read exist before any client ever asks.
   // Idempotent: a no-op once a PIN is set, and re-uses the code otherwise.
   {
       let pool = crate::server::db::pool().await.map_err(RunError::Db)?;
       if let Err(err) = crate::server::auth::ensure_setup_code(pool, &config.data_dir).await {
           tracing::error!(%err, "could not generate the first-run parent PIN setup code");
       }
   }
   ```
2. **`src/server/router.rs`** — add the setup route next to `/api/login`:
   ```rust
   .route("/api/setup", post(setup_handler))
   ```
   ```rust
   #[derive(serde::Deserialize)]
   struct SetupRequest { setup_code: String, pin: String }

   /// `POST /api/setup` — first-run only: the setup code from the log /
   /// `<data>\setup-code.txt` plus the new six-digit PIN, answered with the
   /// same `fh_session` cookie `/api/login` mints. Same-origin rule as login.
   async fn setup_handler(headers: HeaderMap, Json(body): Json<SetupRequest>) -> Response {
       if !crate::server::auth::same_origin_or_absent(&headers) {
           return (StatusCode::FORBIDDEN, "cross-origin setup requests are not allowed").into_response();
       }
       let pool = match crate::server::db::pool().await {
           Ok(pool) => pool,
           Err(err) => {
               tracing::error!(%err, "POST /api/setup: database unavailable");
               return (StatusCode::INTERNAL_SERVER_ERROR, "the hub's database is unavailable").into_response();
           }
       };
       let data_dir = FamilyHubConfig::load().data_dir;
       match crate::server::auth::set_initial_pin(pool, &data_dir, &body.setup_code, &body.pin).await {
           Ok(token) => (StatusCode::OK, [(header::SET_COOKIE, session_cookie(&token))]).into_response(),
           Err(crate::server::auth::AuthError::PinAlreadySet) => {
               (StatusCode::CONFLICT, "a parent PIN is already set — sign in instead").into_response()
           }
           Err(err) => (StatusCode::UNAUTHORIZED, err.to_string()).into_response(),
       }
   }
   ```
   Also make `GET /api/session` answer **`404` when no PIN is set yet** (so the phone can tell "first run" from "signed out" with one probe): in `session_probe_handler`, before the cookie check, `if let Ok(pool) = crate::server::db::pool().await { if matches!(crate::server::auth::pin_is_set(pool).await, Ok(false)) { return StatusCode::NOT_FOUND; } }`.
3. **Tests.** `tests/service_tests.rs::run_with_cwd_forced_to_system32_never_creates_a_db_there`: after `/health` answers, `assert!(data_dir.join("setup-code.txt").is_file(), "the first-run setup code must exist before any client asks")` and assert the log contains `generated the first-run parent PIN setup code`. `tests/router_tests.rs::login_sets_a_well_formed_session_cookie`: replace the direct `auth::set_initial_pin` call with `POST /api/setup {"setup_code": code, "pin": "482913"}` → 200 + the same five cookie assertions, a wrong code → 401, a second call → 409, and `GET /api/session` → 404 before the PIN exists.
4. **Docs.** `docs/OWNER_CHECKLIST.md` step 4: replace "to the log, to `%ProgramData%\FamilyHub\setup-code.txt`, and onto the television" with "to the log and to `%ProgramData%\FamilyHub\setup-code.txt` (the television never shows it)", and "go to **Settings**, enter the setup code and choose a **six-digit** PIN" stays — it becomes true with Q2-02. `docs/RECOVERY.md` mode 7 step 4: delete "and shows it on the television". `src/server/auth.rs:12-13, 201-204` doc comments: "the TV" → "the log and the file", and "every client (TV included) asks at least once" → "`router::run` calls it at boot".
5. **Boss commit (§5.2).** `docs/PLAN.md` T1.4 row: "first-run setup code to log + `<data>\setup-code.txt` + TV" → "first-run setup code to log + `<data>\setup-code.txt` (not shown on the TV — HANDOFF T2.1 H-24 decision)"; §5.9's default-9 reference likewise; record in HANDOFF. `docs/RESIDUAL.md` (which HANDOFF says T3.2 would create — it does not exist) gets the "join-QR overlay may gain the code if gated to the HTTP listener" item.

### Q2-02 — a first-run form, and the phone on the cookie it was promised

1. **`src/client/components/mobile/session.rs`** — replace the `localStorage` token with a probe:
   ```rust
   /// What `GET /api/session` said: `None` until it answers.
   #[derive(Clone, Copy, PartialEq, Eq, Debug)]
   pub enum SessionState { FirstRun, SignedOut, Parent }

   pub async fn probe() -> Option<SessionState> {
       match http::status("GET", "/api/session", None).await? {
           204 => Some(SessionState::Parent),
           404 => Some(SessionState::FirstRun),
           _ => Some(SessionState::SignedOut),
       }
   }
   pub async fn login(pin: &str) -> bool { http::status("POST", "/api/login", Some(&format!(r#"{{"pin":"{pin}"}}"#))).await == Some(200) }
   pub async fn setup(setup_code: &str, pin: &str) -> bool { http::status("POST", "/api/setup", Some(&format!(r#"{{"setup_code":"{setup_code}","pin":"{pin}"}}"#))).await == Some(200) }
   pub async fn logout() { let _ = http::status("POST", "/api/logout", None).await; }
   ```
   with `mod http` a `#[wasm_bindgen(inline_js = …)]` `fetch(url, {method, credentials: 'same-origin', headers: {'content-type':'application/json'}, body})` returning `response.status` (same shape as `routine.rs::upload`; the non-wasm stub returns `None`), declared in `docs/NON_RUST.md` on the existing `inline_js` row. Delete `token()`, `store()`, `clear()`, `SESSION_STORAGE_KEY`; keep `is_parent()` only as `matches!(state, Some(SessionState::Parent))` over a `Signal<Option<SessionState>>` provided by `MobileShell` (`use_context_provider`, refreshed by a `use_resource` on mount and after every login/setup/logout).
2. **`src/client/components/mobile/settings.rs`** — three states from that signal: `FirstRun` renders the **setup form** (setup code `input` `inputmode="numeric" maxlength=6`, new PIN, confirm PIN; submit → `session::setup`; on success `status = "PIN set — this phone is signed in."`), `SignedOut` renders today's PIN form but calling `session::login`, `Parent` renders "Sign out" → `session::logout`. The offline-queue and install sections are unchanged. Copy for the first-run form: "First-time setup. Type the setup code from `%ProgramData%\FamilyHub\setup-code.txt` on the hub PC (or its log), then choose a six-digit PIN."
3. **Call sites** — `mobile/remote.rs:53,73`: `auth: None` (the upgrade cookie authorises the connection, `realtime.rs:1050`); `calendar.rs:326,366`: `None`; `routine.rs:280`: `String::new()`; `routine.rs:503` / `upload::submit`: drop the `auth` parameter and the `form.append('auth', …)` line (the cookie rides on the same-origin `fetch`).
4. **Server seams that still only take the bearer** (edit, then the Boss ratifies as in round 1): `src/server/api/calendar.rs::require_parent` → `async`, `if token.is_empty() { auth::require_parent().await } else { auth::require_session(&token) }` (the `profiles.rs::require_session_or_cookie` shape); `src/server/api/photos.rs::delete_custom_task` → the same; `photos::upload_photo_handler` and `screensaver::upload_screensaver_image_handler` gain a `headers: HeaderMap` extractor and `require_parent_session(auth.as_deref())` becomes `require_parent_session(auth.as_deref(), &headers)` which accepts either a valid bearer field **or** `auth::session_from_headers(&headers)` being a valid session (the field, if present, still comes first).
5. **Tests.** `tests/photo_tests.rs`: `t2_5_a` posts with a `Cookie: fh_session=<token>` header and **no** `auth` field (keep `t2_5_g` as the no-credential 401). `tests/calendar_tests.rs`: one create with `auth: None` through the HTTP endpoint carrying the cookie → 200. `mobile/settings.rs` unit test: a `VirtualDom` render with the context signal set to `FirstRun` contains "First-time setup"; with `Parent` contains "Sign out". Keep `tests/realtime_tests.rs::t1_4_q1_11_*` as they are.
6. **Docs.** `docs/PWA.md` "The five tabs": "sign in with the six-digit PIN under Settings first (the very first time, enter the setup code and choose the PIN there)". `docs/HANDOFF.md`: close H-19 and H-25.

### Q2-03 — the phone's task list listens to the right signal

`src/client/components/routine.rs:137-140`:
```rust
let mut tasks = use_resource(move || async move {
    // `TasksUpdated` bumps `tasks_version` (T1.5 / G22); `routine_version`
    // is the routine's own signal and never moves for a task change.
    let _version = (bus.tasks_version)();
    get_custom_tasks((state.active_user_id)()).await
});
```
(Reading both `tasks_version` and `routine_version` is acceptable; reading only `routine_version` is the bug.)

### Q2-04 — the service logs why it failed to start

`src/server/service.rs::scm::run_service`, replace the `loop` and the `Stopped` report with:
```rust
let mut exit_code = ServiceExitCode::Win32(0);
loop {
    match stop_rx.recv_timeout(Duration::from_secs(1)) {
        Ok(()) => break,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) if handle.is_finished() => {
            // Q1-04 / Q2-04: read the task's own result so the *reason* (a
            // bind, database or PKI failure, or a panic) reaches the log,
            // not just the fact that the task ended.
            match runtime.block_on(&mut handle) {
                Ok(Ok(())) => tracing::error!("FamilyHub service: the server task returned without an error"),
                Ok(Err(err)) => tracing::error!(%err, "FamilyHub service: startup failed"),
                Err(join) => tracing::error!(%join, "FamilyHub service: the server task panicked"),
            }
            exit_code = ServiceExitCode::ServiceSpecific(1);
            break;
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
    }
}
```
(`let mut handle = runtime.spawn(...)`; `&mut JoinHandle` is `Future`.) Because the SCM path cannot be unit-tested here, add the same shape to `run_console` for symmetry only if it is not already logging (it is — leave it). Add a unit test on a helper `fn describe_server_exit(result: Result<Result<(), RunError>, tokio::task::JoinError>) -> String` extracted from the match above, asserting the `RunError::Bind` text survives.

### Q2-05 — a log level the installed service can actually be given

1. `src/server/config.rs`: `pub log_level: Option<String>` resolved like the other keys (`FAMILY_HUB_LOG` env, else `log.level` in `familyhub.toml`, else `None`); log it in `ensure_dirs_and_log`. Unit tests for both sources (copy the `screensaver_schedule_hour` pair).
2. `src/server/service.rs`: `fn level_from_env()` → `pub fn level_from(raw: Option<&str>) -> tracing::Level` (same match, `None` → INFO); `ServiceLogger::open(log_dir: &Path)` → `ServiceLogger::open(log_dir: &Path, max_level: tracing::Level)`; `install_global_logger` passes `level_from(config.log_level.as_deref())`; the three Q1-05 unit tests call `open(&dir, level_from(Some("debug")))` etc. instead of setting the env var (and can drop `ENV_LOCK`).
3. Docs. `docs/DEV_WINDOWS.md` "Log level" and `docs/RECOVERY.md` mode 2: "For `family-hub.exe run` set `$env:FAMILY_HUB_LOG = "debug"` in that shell. For the **installed service** the shell's environment is not inherited — put `[log]\nlevel = "debug"` in `familyhub.toml` next to `family-hub.exe` (or under the data directory) and `family-hub.exe stop` / `start`." Add the `[log]` section to `docs/RECOVERY.md`'s "the four things worth knowing" table.

### Q2-06 — T3.3's contract, with a real transcript

1. `tests/docs_tests.rs::t3_3_every_task_id_appears_exactly_once_in_verification`: after the count assertion, `let row = verification_content.lines().find(|l| l.starts_with(&pattern)).expect("row"); assert!(row.contains("| PASS |"), "{task_id} is not PASS: {row}");` and, after the loop, `assert!(!verification_content.contains("| FAIL |"))`.
2. `docs/VERIFICATION.md`: add `## Transcripts` with one fenced block per task ID (heading form `### Transcript — T1.2` — never `| T1.2 |`) containing the exact command (`cargo test --features server --test <file>`) and the `test result: ok. N passed …` line **pasted from a fresh run**; end with the full-suite wall clock. The Boss will grep every named test against the tree. For cross-checking, this audit's run produced: `backup_tests` 11, `calendar_tests` 14, `ci_tests` 5, `config_tests` 4, `db_tests` 9, `docs_tests` 17, `health_pool_closed_tests` 1, `health_tests` 2, `http_tests` 14, `loop_tests` 1, `palette_tests` 6, `photo_tests` 7, `profiles_tests` 6, `pwa_tests` 16, `realtime_tests` 17, `router_tests` 12, `routine_tests` 10, `screensaver_tests` 5, `service_tests` 2, `storage_tests` 9 (+1 ignored), `tls_tests` 7, `tv_tests` 22, `whiteboard_tests` 3, lib unit tests 191 — 391 passed, 0 failed, 3 min 19 s.
3. Resolve the `loop_tests` contradiction: run `cargo test --features server --test loop_tests` 20× in fresh processes, and replace both the "20 / 20" row and the "residual failure rate … runs 8-11 and 9-12" paragraph with the one true count from that run (this audit's ×10 count is in the table at the top of this file).

### Q2-07 — exact pins in the xtask (Q1-17, verbatim)

`xtask/Cargo.toml`: `resvg = "=0.45.1"`, `usvg = "=0.45.1"`, `tiny-skia = "=0.11.4"`, `image = "=0.25.10"` (`anyhow` may stay a range); root `Cargo.toml` dev-deps `resvg = "=0.45.1"`, `rqrr = "=0.10.1"`. `cargo update -p resvg@0.41.0 --precise 0.45.1` (and `usvg`) so `Cargo.lock` carries one resvg/usvg tree; re-run `cargo run -p xtask -- icons` and commit the regenerated PNGs if their bytes change (the `docs_tests` dimension/safe-zone assertions must still pass). Add to `tests/ci_tests.rs`: parse `xtask/Cargo.toml` and assert every `resvg`/`usvg`/`tiny-skia` line contains `"=`. `Cargo.toml`/`Cargo.lock` are Boss-serialised (§P4): this agent must be the only one in its wave touching them.

## Low observations (not blocking, no action required this round)

- `POST /api/logout` has no `same_origin_or_absent` check (a cross-site POST with the cookie revoked the session in the spot check); `SameSite=Lax` keeps a real browser from attaching the cookie to such a POST, so the exposure is theoretical. Adding the same guard as `/api/login` is a two-line change.
- `api::whiteboard::undo_last_stroke(client_id)` trusts the caller-supplied `client_id`; origins are fanned out in every `Draw`, so any connected client can undo another's last stroke. T2.3(c) is client-honoured, not server-enforced. A family whiteboard makes this cosmetic.
- `realtime::snapshot`'s contiguous `latest` bookmark stops advancing past any undone or compacted `seq` forever; harmless today because both clients always `RequestSnapshot { since_seq: 0 }`.
- `docs/RECOVERY.md` mode 1 "unstyled text … no CSS variables resolve without the wasm client" is now wrong after Q1-01: a missing `public\` gives a **styled but inert** kiosk (`/tailwind.css` is served from the binary).
- `docs/DEV_WINDOWS.md` step 4 verifies Tailwind with `--version`, which the standalone binary does not support (`ci.yml` documents the exit-9 trap and uses `--help`).
- `db::hard_reset_board`'s doc comment ("`seq` is derived from `MAX(seq)`, so deleting every row is enough to restart it") is stale since Q1-09's in-process counter; `tls.rs` still calls `base64` "the legacy photo path" dependency.
- `service.rs::install_global_logger`'s panic hook calls `tracing::error!`; a panic raised while `ServiceLogger::append_line` holds its `Mutex` would re-enter the same lock.
- `/tv` on the HTTP origin still links `/manifest.webmanifest` (308 on every kiosk load) — carried from round 1.
