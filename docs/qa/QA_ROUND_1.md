VERDICT: FAIL

# Fable QA — Round 1

**Auditor:** Fable 5, fresh context (no prior conversation), 2026-08-30.
**Scope:** `docs/PLAN.md` §0/§3/§5, `docs/reviews/PURPLE_TEAM.md` §P3/§P5.4/§P5.5, `docs/VERIFICATION.md`, `docs/NON_RUST.md`, `docs/HANDOFF.md`, and the full diff `5769946..54180f6` (125 files, every Rust/SQL/config/doc file read in full). `docs/BLOCKED.md` does not exist.
**Tiers** are the *originally assigned* tier from PLAN §3 (H/S/O), which is the tier that receives the fix.

## What was run

| Gate | Result |
| --- | --- |
| `cargo fmt --check` | exit 0 |
| `cargo clippy --features server --all-targets -- -D warnings` | exit 0 |
| `cargo clippy --features web --target wasm32-unknown-unknown -- -D warnings` | exit 0 |
| `cargo test --features server` | exit 0 — 166 lib unit tests + 27 integration binaries, all green (`realtime_tests` 12/12 in 130 s, `whiteboard_tests` 3/3, `loop_tests` 1/1) |
| `cargo tree -d --features server` | no duplicate `axum`/`tower-http`/`hyper`; no `openssl-sys`, `native-tls` or `aws-lc-rs` in the tree; every §P5.4 crate resolves to its pinned version (`dioxus 0.7.10`, `axum 0.8.9`, `sqlx 0.8.6`, `rustls 0.23.43`, `tokio-rustls 0.26.4`, `rcgen 0.14.10`, `mdns-sd 0.21.0`, `fast_qr 0.13.1`, `argon2 0.6.0`, `rrule 0.14.0`, `windows-service 0.8.1`, `tokio-tungstenite 0.28`) — except the xtask pins, Q1-17 |

Acceptance spot checks executed by hand (beyond the suite):

1. **T3.1 / T0.6 / T1.3** — booted `target/debug/family-hub.exe run` with `FAMILY_HUB_DATA_DIR` in a scratch directory and ephemeral ports: `/health` 200 with all 8 keys (`migration_version: 3`, `days_to_expiry: 396`); `/m` on the HTTP origin → 308 to `https://…:18443/m`; `/ca.crt` → 200 `application/x-x509-ca-cert`; `/tv` → 200 — **but the stylesheet `href` is the un-rewritten manganis placeholder and the page contains no wasm bundle** (Q1-01).
2. **T1.4** — `parent_setup_status` generated `<data>\setup-code.txt`; 50 wrong `set_initial_parent_pin` calls were answered back-to-back with no delay (Q1-02); the real code set the PIN; 12 *parallel* wrong `verify_parent_pin` calls were all answered (Q1-03); the correct PIN still verified afterwards (no lockout — good).
3. **T2.5 / T2.7** — `POST /api/upload_photo` (12 MP fixture, no session) → 201 and the file appeared under `uploads/`; `POST /api/upload_screensaver_image` (no session) → 200 (Q1-07). The re-encode itself worked (`task-2-….jpg`).
4. **T3.1 logging** — after ~3 minutes the service log held 293 KB / 543 `TRACE` lines of `dioxus_core` VNode diffs (Q1-05).
5. **T3.4** — WCAG ratios recomputed independently for the phone Routine tab pairs (Q1-15).
6. **T0.2 / T0.8** — dependency-tree assertions above.

## Findings

PASS requires zero Critical/High/Med. There are 1 Critical, 4 High, 12 Med. Low observations are listed at the end and are not blocking.

| # | Task | Tier | File:line | Sev | Description | Solution (verbatim detail in the section below) |
| --- | --- | --- | --- | --- | --- | --- |
| Q1-01 | T3.1 | S | `src/server/service.rs:801`, `src/server/router.rs:357`, `src/client/app.rs:9`, `docs/OWNER_CHECKLIST.md:59`, `docs/DEV_WINDOWS.md:98-124`, `.github/workflows/ci.yml:286` | **Critical** | The binary the owner is told to install as the service (`family-hub.exe`, from `cargo build --release`) serves `/tv` with the manganis placeholder as the stylesheet `href` (503, unstyled) and, with no `public/` bundle beside it, **no wasm client at all** — no hydration, no WebSocket, no D-pad handler. Verified live. `ensure_public_dir_exists` creates an *empty* `public/` and hides the problem. The runbooks never mention the dx bundle; CI archives nothing shippable. | Serve the stylesheet from the binary at a stable URL, fail loudly when the wasm bundle is missing, make `install` refuse without it, have CI assemble `family-hub.exe + public/`, and fix the three runbooks. |
| Q1-02 | T1.4 | S | `src/server/auth.rs:234-248` | High | `set_initial_pin` has no backoff and no serialisation: the 6-digit setup code (10⁶ keyspace) can be brute-forced from any LAN device in minutes during the window between install and the owner setting the PIN, seizing the parent PIN. Verified: 50 wrong codes answered instantly. | Route setup-code failures through the same counter/backoff/serialising gate as `verify_pin`. |
| Q1-03 | T1.4 | S | `src/server/auth.rs:325-341` | High | The PIN backoff is a per-request `sleep`, not a gate: N parallel wrong guesses are all answered within one delay, so throughput is bounded only by argon2 CPU (release ≈ hundreds/s on 14 cores → 10⁶ PINs in about an hour) instead of by the schedule. argon2 also runs on the async worker thread. | Hold an async `Mutex` across verify + sleep; run argon2 in `spawn_blocking`. |
| Q1-04 | T3.1 | S | `src/server/service.rs:966-1012`, `src/server/router.rs:396-429`, `src/server/service.rs:1235` | High | `router::run` panics on a bind/DB/PKI failure inside `runtime.spawn(...)`; the panic goes to stderr (nowhere under the SCM), the service stays **RUNNING serving nothing**, SCM recovery never fires and no failure actions are configured. The "startup failure logged within 5 s" test never exercises the startup path (it logs a hand-written `tracing::error!`). | Install a panic hook that logs; make `run` fallible; have the service report `Stopped` with a non-zero exit code when the server task ends; set SCM failure actions in `install`; replace the synthetic test with a real one. |
| Q1-05 | T3.1 | S | `src/server/service.rs:643-646, 657-680` | High | `ServiceLogger::enabled` returns `true` for every level and `max_level_hint` is absent, so every `dioxus_core`/`hyper`/`sqlx` TRACE event is formatted and `flush()`ed to `familyhub.log` on the request hot path. §P5.5 default 33 (`info` in the service, `debug` behind `FAMILY_HUB_LOG`) is not implemented. Verified: 293 KB / 543 TRACE lines in ~3 idle minutes; 10 MB × 5 will churn in well under an hour of real use and bury every real error. | Level filter (INFO default, `FAMILY_HUB_LOG` override), `max_level_hint`, buffered writes. |
| Q1-06 | T2.5 | S | `src/server/api/routine.rs:136-168`, `src/server/db.rs:467-495, 692-722` | Med | The v1 base64 `create_photo_task` server fn — which T2.5 was to *replace* — is still mounted at `/api/create_photo_task`, unauthenticated, and `db::write_photo` writes the decoded bytes to `/uploads/task-*.jpg` with **no sniff, allowlist or re-encode** (R-23c bypass) and no `due_date`. | Delete the endpoint and the raw-write path; make `insert_custom_task` take an already-stored path. |
| Q1-07 | T2.5 / T2.7 | S | `src/server/api/photos.rs:92, 330`, `src/server/api/screensaver.rs:109` | Med | `POST /api/upload_photo`, `POST /api/upload_screensaver_image` and `delete_custom_task` require no session, while calendar CRUD does. Any LAN client can create photo tasks for any child, delete any child's task (ids are sequential), and fill `screensaver/` with unbounded 25 MiB posts. Verified 201/200 with no token. PLAN §0.3/§P5.5 default 35 puts photo capture and administration on the phone behind the parent PIN. | Require the parent session (multipart `auth` field / fn argument), 401 otherwise; phone dialog gated on `session::is_parent()`. |
| Q1-08 | T1.5 | S | `src/server/api/routine.rs:100-118, 239-255`, `src/server/db.rs:647-668`, `src/client/components/routine.rs:79-84`, `tests/http_tests.rs:48-63, 300, 350` | Med | `claim_mutation` commits the key in its own statement *before* the write; if the write fails the key stays claimed and every retry/replay returns `Ok(())` doing nothing (silent loss, reported as success). This is exactly the "200/`null` instead of 500" flake recorded in HANDOFF (wave-3 close): the http tests use constant keys in a `temp/familyhub-http-tests-<pid>` DB that is never deleted, and Windows recycles PIDs. Separately, idempotency keys are `performance.now()-counter` with no per-client component, so two devices can mint the same key and the second toggle is dropped. | Claim + write in one transaction; salt keys with a per-page-load nonce; unique keys and a cleaned scratch dir in the tests. |
| Q1-09 | T2.3 | S | `src/server/api/realtime.rs:355-421` | Med | Write-behind `record_stroke`: rows are inserted by detached tasks, out of order and after the broadcast; a `RequestSnapshot` in the window misses strokes, a crash loses in-flight strokes, and the two recorded flakes (`loop_tests` ~15 %, `whiteboard_tests` 500-stroke count) are this race. Boss decided at the 2-a rerun close to fix it; never applied. | One ordered persistence task draining an `mpsc` channel and committing batches in `seq` order. |
| Q1-10 | T1.2 | O | `src/server/api/realtime.rs:637-640, 792-815` | Med | A `Draw` is validated only for point count. `color` is an unbounded `String`, `width` may be `NaN`/`1e308`, points may be outside 0..1, and `WebSocketUpgrade` keeps tungstenite's 64 MiB default message size — a multi-megabyte frame from any LAN client is accepted, written to SQLite and fanned out to the TV's wasm JSON parser (kiosk freeze). | Cap WS message/frame size at 256 KiB and validate the stroke fields. |
| Q1-11 | T1.4 | S | `src/client/components/mobile/session.rs:19-47`, `src/server/router.rs`, `docs/HANDOFF.md` H-19 | Med | Contract (PLAN §3 T1.4, §5.9 default 31): session token as an `HttpOnly`/`Secure`/`SameSite=Lax` cookie on the HTTPS origin. Delivered: a bearer UUID in `localStorage`, scheduled as a Boss micro-commit at the 2-a/2-b boundary and never applied; PLAN was not amended (§5.2). | Either a Boss commit amending PLAN T1.4/§5.9, or the login route + cookie extractor below (with the `Origin` check that cookies then require on `/ws`). |
| Q1-12 | T2.4 | O | `src/client/components/tv/model.rs:221`, `src/client/components/tv/surface.rs:300-307`, `src/client/components/tv/shell.rs:183-186, 245-248` | Med | W3 is only half landed: on the television (the primary display) a failed `get_today_events` renders "Nothing on the calendar today." — indistinguishable from an empty day. HANDOFF T2.4 H-22 recorded the three-step fix; never applied. | Carry `CalendarState<Vec<CalendarEvent>>` in `TvModel` and render Loading/Error/Empty. |
| Q1-13 | T1.2 | O | `src/server/api/realtime.rs` (no emitter), `docs/PROTOCOL.md:126`, `src/client/components/tv/clock.rs:26-31` | Med | `ServerMessage::Health { stale, last_update }` is documented as the badge's freshness signal (D5, PROTOCOL.md) but nothing ever sends it; the kiosk works around it with a 20 s HTTP poll of `tv_clock`. Docs do not match code. | Publish `Health` from a 25 s heartbeat in `router::run` (Boss guidance at the 2-a close) and have the kiosk tracker consume it. |
| Q1-14 | T2.7 | S | `src/server/api/screensaver.rs:279-286`, `src/client/components/screensaver.rs:62-63, 106, 115-117` | Med | The scheduled screensaver has no enable path (`ScreensaverSchedule::default()` is the only instance ever constructed) and, if it were enabled, the overlay it forces can only be dismissed by a pointer (`onclick`/`onpointerdown`) or by another phone `SetView` — the remote cannot clear it, contradicting §P5.5 default 35 ("screensaver dismiss … fully operable from the TV remote"). | `[screensaver] schedule_hour` in `FamilyHubConfig`; keydown activity clears the scheduled overlay. |
| Q1-15 | T3.4 | O | `tests/palette_tests.rs:606` (`surface_sources`), `src/client/components/routine.rs:156, 256, 287, 329, 365`, `src/client/components/whiteboard.rs:137` | Med | The WCAG scan covers `tv/**` and `mobile/**` only, but `/m` renders `routine.rs`, `calendar.rs` and `whiteboard.rs`. Measured on the phone Routine tab: `text-red-500` on white **3.76:1**, `text-sheffield-accent` on paper **3.11:1**, white on `bg-sheffield-accent` **3.17:1** at 18 px bold (not "large"), `text-sheffield-light` on white **2.16:1**, white on `bg-sheffield-light` discs **2.16:1**. Contract: "WCAG AA for every token pair". | Five class substitutions, then widen the scan to `components/**` and add the red pairs to the table. |
| Q1-16 | T3.3 | H | `tests/docs_tests.rs:785-805`, `docs/VERIFICATION.md` | Med | The T3.3 test asserts each task ID appears once but **not** that none is `FAIL` (contract: "asserts every task ID … appears exactly once and none is FAIL"); `VERIFICATION.md` has no per-task command transcript or timings (contract: "command transcript", "per-task pass/fail and timings"). | Add the PASS assertion; regenerate the doc with a transcript block per task. |
| Q1-17 | T0.7 | H | `xtask/Cargo.toml:7-9`, `Cargo.lock:4114, 5563` | Med | `resvg = "0.41"`, `usvg = "0.41"`, `tiny-skia = "0.11"` are caret ranges; §P5.4 says resvg/tiny-skia are "pinned exactly in `Cargo.toml` by T0.7". The lockfile now carries **two** resvg/usvg trees (0.41.0 for xtask, 0.45.1 for the T1.3 dev-dep). | Pin xtask to `=0.45.1` / `=0.11.4`, regenerate the icons, assert the pins in `ci_tests`. |

## Solutions

### Q1-01 — ship a kiosk that renders and hydrates from `family-hub.exe`

1. **`src/client/app.rs`** — stop routing the stylesheet through manganis:
   ```rust
   // delete: const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");
   // in App(): document::Link { rel: "stylesheet", href: "/tailwind.css" }
   ```
2. **`src/server/router.rs`** — serve it from the binary (same pattern as `sw.js`), in `build_router` before `.serve_dioxus_application(...)`:
   ```rust
   .route("/tailwind.css", get(|| async {
       (
           [(header::CONTENT_TYPE, "text/css"), (header::CACHE_CONTROL, "no-cache")],
           include_str!("../../assets/tailwind.css"),
       )
   }))
   ```
   Add to `tests/router_tests.rs`: `GET /tailwind.css` → 200, `text/css`, body contains `.p-\[5\%\]`; and to `tests/http_tests.rs::http_tv_serves_dashboard_with_panel_markers`: `assert!(body.contains(r#"href="/tailwind.css""#)); assert!(!body.contains("This should be replaced by dx"));`.
3. **`src/server/router.rs::ensure_public_dir_exists`** — after `create_dir_all`, detect a missing client bundle and say so:
   ```rust
   let has_wasm = std::fs::read_dir(public_path.join("assets"))
       .map(|entries| entries.flatten().any(|e| e.path().extension().is_some_and(|x| x == "wasm")))
       .unwrap_or(false);
   if !has_wasm {
       tracing::error!(path = %public_path.display(),
           "no wasm client bundle in the public directory: /tv and /m will render but never hydrate — \
            copy target/dx/family-calendar/release/web/public next to the executable or set DIOXUS_PUBLIC_PATH");
   }
   ```
   Expose `pub fn public_bundle_present(public_dir: &Path) -> bool` (the same check) and unit-test it with a scratch dir containing `assets/x.wasm`.
4. **`src/server/service.rs::install_with`** — refuse to register a service that cannot serve the kiosk:
   ```rust
   let public = exe_path.parent().map(|d| d.join("public")).unwrap_or_default();
   if std::env::var_os("DIOXUS_PUBLIC_PATH").is_none() && !crate::server::router::public_bundle_present(&public) {
       return Err(ServiceError::Io(io::Error::new(io::ErrorKind::NotFound,
           format!("{} has no wasm client bundle; copy target/dx/family-calendar/release/web/public beside family-hub.exe first", public.display()))));
   }
   ```
   Add a `MockServiceHost` test that `install_with` fails with `NotFound` when the bundle is absent and succeeds when a scratch `public/assets/app.wasm` exists (point `current_exe` at it via a new `install_with_exe(host, runner, exe_path)` seam).
5. **`.github/workflows/ci.yml`** — replace the last step with:
   ```yaml
   - name: Windows-x64 release build (service host + dx bundle)
     shell: pwsh
     run: |
       cargo build --features server --release --bin family-hub
       if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
       New-Item -ItemType Directory -Force dist/FamilyHub | Out-Null
       Copy-Item target/release/family-hub.exe dist/FamilyHub/
       Copy-Item -Recurse target/dx/family-calendar/release/web/public dist/FamilyHub/public
       if (-not (Get-ChildItem dist/FamilyHub/public/assets -Filter *.wasm)) { Write-Error "no wasm bundle"; exit 1 }
   - uses: actions/upload-artifact@v4
     with: { name: FamilyHub-windows-x64, path: dist/FamilyHub }
   ```
   (`tests/ci_tests.rs::ci_workflow_has_the_seven_named_steps` matches on the step name prefix — keep "Windows-x64 release build".)
6. **Docs.** `docs/OWNER_CHECKLIST.md` step 3: "Put **both** `family-hub.exe` (from `cargo build --release --features server`) **and** the `public\` folder from `dx build --platform web --release` (`target\dx\family-calendar\release\web\public`) in `C:\Program Files\FamilyHub\`; `install` refuses to run without `public\assets\*.wasm`." `docs/DEV_WINDOWS.md` step 5: delete the false bullet "Run Tailwind CLI to compile `input.css` → `output.css`" (nothing does this; the CSS is committed) and step 6: replace `cargo run --features server --release` with the same two-artefact recipe + `.\family-hub.exe run`. `docs/RECOVERY.md` "TV is blank": add "a `/tv` that shows unstyled text or never reacts to the remote means `public\` is missing beside the service binary — see checklist step 3". Add to `tests/docs_tests.rs::t3_2_owner_checklist_has_eight_numbered_steps_each_with_a_pass_criterion` the assertion `assert!(checklist.contains("public"))` and to `docs/VERIFICATION.md` Finding 1 the resolution.

### Q1-02 — backoff and serialisation on the setup code

`src/server/auth.rs`, `set_initial_pin`: replace the two early-return `Err(AuthError::InvalidSetupCode)` sites with a single failure path that shares `verify_pin`'s counter and gate:
```rust
let _serial = pin_gate().lock().await;                       // see Q1-03
let expected = db::get_setting(pool, SETUP_CODE_SETTING).await?;
let matches = expected.as_deref().is_some_and(|e| !e.is_empty() && constant_time_eq(setup_code, e));
if !matches {
    let attempt = bump_pin_failures();
    tokio::time::sleep(backoff_delay(attempt)).await;
    return Err(AuthError::InvalidSetupCode);
}
```
Add to `tests/profiles_tests.rs::setting_the_initial_pin_requires_the_real_setup_code`: five wrong codes in a row, assert each answer takes ≥ 2ⁿ ms and `auth::current_pin_failures()` advanced. Also decide the "+ TV" clause of PLAN T1.4 explicitly: either the Boss commits a PLAN amendment ("log + `setup-code.txt` only; the TV never shows it") or T1.4 implements HANDOFF T2.1 H-24's `parent_setup_code()` gated to the HTTP kiosk listener and `pin_set == false`. Leaving the clause unimplemented with no PLAN commit violates §5.2.

### Q1-03 — make the backoff a gate

`src/server/auth.rs`:
```rust
static PIN_GATE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
fn pin_gate() -> &'static tokio::sync::Mutex<()> { &PIN_GATE }

pub async fn verify_pin(pool: &SqlitePool, pin: &str) -> Result<String, AuthError> {
    let _serial = pin_gate().lock().await;           // one attempt at a time, sleep included
    let stored_hash = db::get_setting(pool, PIN_HASH_SETTING).await?.ok_or(AuthError::PinNotSet)?;
    let pin_owned = pin.to_string();
    let correct = is_valid_pin_format(pin)
        && tokio::task::spawn_blocking(move || verify_pin_hash(&pin_owned, &stored_hash))
            .await
            .map_err(|e| AuthError::Storage(e.to_string()))?;
    // ... unchanged from here
}
```
Use the same gate in `set_initial_pin` (Q1-02) and `spawn_blocking` for `hash_pin` in `set_initial_pin`/`change_pin`. `tests/profiles_tests.rs` gains: 8 wrong PINs fired with `tokio::join!` must take ≥ Σ 2ⁿ ms (n = 1..8, 510 ms) of wall clock, proving serialisation. The existing sequential assertions are unchanged.

### Q1-04 — a service that fails to start must stop, and say so

1. `src/server/service.rs::install_global_logger` — after installing the subscriber:
   ```rust
   std::panic::set_hook(Box::new(|info| {
       tracing::error!(%info, "panic");
       eprintln!("{info}");
   }));
   ```
2. `src/server/router.rs::run` — change the signature to `pub async fn run(config: FamilyHubConfig) -> Result<(), RunError>` where `pub enum RunError { DataDir(std::io::Error), Db(sqlx::Error), Pki(PkiError), Bind { addr: SocketAddr, err: std::io::Error }, Tls(TlsError), Serve(std::io::Error) }` (with `Display`), replacing every `.expect(...)` in `run` with `map_err` + `?`, and `tracing::error!(%err, "hub failed to start")` at the single `Err` exit. `src/main.rs` (frozen — Boss edit, stays < 25 lines): `if let Err(err) = rt.block_on(router::run(cfg)) { eprintln!("{err}"); std::process::exit(1) }`.
3. `run_console`: `if let Err(err) = runtime.block_on(router::run(config)) { tracing::error!(%err, "family-hub run: startup failed"); std::process::exit(1) }`.
4. `scm::run_service`: keep the `JoinHandle` and watch it:
   ```rust
   let handle = runtime.spawn(async move { crate::server::router::run(config_for_run).await });
   // ... report Running as today, then:
   let mut exit_code = ServiceExitCode::Win32(0);
   loop {
       match stop_rx.recv_timeout(Duration::from_secs(1)) {
           Ok(()) => break,
           Err(std::sync::mpsc::RecvTimeoutError::Timeout) if handle.is_finished() => {
               tracing::error!("FamilyHub service: the server task ended unexpectedly");
               exit_code = ServiceExitCode::ServiceSpecific(1);
               break;
           }
           Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
           Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
       }
   }
   ```
   and report `Stopped` with `exit_code`.
5. `WindowsServiceHost::create` — after `set_description`:
   ```rust
   use windows_service::service::{ServiceAction, ServiceActionType, ServiceFailureActions, ServiceFailureResetPeriod};
   service.update_failure_actions(ServiceFailureActions {
       reset_period: ServiceFailureResetPeriod::After(Duration::from_secs(86_400)),
       reboot_msg: None, command: None,
       actions: Some(vec![
           ServiceAction { action_type: ServiceActionType::Restart, delay: Duration::from_secs(5) },
           ServiceAction { action_type: ServiceActionType::Restart, delay: Duration::from_secs(30) },
           ServiceAction { action_type: ServiceActionType::Restart, delay: Duration::from_secs(60) },
       ]),
   }).map_err(|e| ServiceError::Scm(e.to_string()))?;
   service.set_failure_actions_on_non_crash_failures(true).map_err(|e| ServiceError::Scm(e.to_string()))?;
   ```
   (the `ServiceAccess` in `create_service` needs `| ServiceAccess::START | ServiceAccess::CHANGE_CONFIG`). Record the recovery policy in `docs/RECOVERY.md`.
6. Replace `a_deliberate_startup_failure_is_logged_within_five_seconds` with an integration test in `tests/service_tests.rs`: bind a `TcpListener` on `127.0.0.1:0`, spawn `family-hub.exe run` with `FAMILY_HUB_ADDR` = that address and a scratch data dir, `wait()` with a 10 s deadline, assert the process exited non-zero within 5 s and `<data>\logs\familyhub.log` contains `failed to bind`.

### Q1-05 — log level

`src/server/service.rs`:
```rust
pub struct ServiceLogger { file: Mutex<io::BufWriter<std::fs::File>>, path: PathBuf, event_source: String, max_level: tracing::Level }

fn level_from_env() -> tracing::Level {
    match std::env::var("FAMILY_HUB_LOG").ok().as_deref().map(str::to_ascii_lowercase).as_deref() {
        Some("trace") => tracing::Level::TRACE,
        Some("debug") => tracing::Level::DEBUG,
        Some("warn") => tracing::Level::WARN,
        Some("error") => tracing::Level::ERROR,
        _ => tracing::Level::INFO,          // §P5.5 default 33
    }
}
impl tracing::Subscriber for ServiceLogger {
    fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool { *metadata.level() <= self.max_level }
    fn max_level_hint(&self) -> Option<tracing::level_filters::LevelFilter> { Some(tracing::level_filters::LevelFilter::from_level(self.max_level)) }
    // ...
}
```
In `append_line`, write through the `BufWriter` and call `flush()` only when `level <= WARN` (pass the level in) or every 64 lines; flush on `Drop`. Unit tests: default level drops a `tracing::debug!` and keeps `tracing::info!`; `FAMILY_HUB_LOG=debug` keeps the debug line (serialise with the existing `ENV_LOCK`). Document `FAMILY_HUB_LOG` in `docs/DEV_WINDOWS.md` and `docs/RECOVERY.md` "Hub unreachable".

### Q1-06 — remove the raw base64 write path

Delete `create_photo_task` from `src/server/api/routine.rs` and from the `pub use routine::{...}` list in `src/server/api/mod.rs`; delete `db::write_photo` and the `base64` import in `db.rs`. Change `db::insert_custom_task(pool, user_id, title, photo_path: Option<&str>) -> Result<u32, sqlx::Error>` to store the given already-stored web path (it becomes a thin wrapper over `insert_custom_task_with_due_date(.., None)`). Update the eight call sites in `tests/backup_tests.rs` (226, 378, 615, 665), `tests/db_tests.rs` (110, 139, 188) and `tests/routine_tests.rs` (369, 423, 463): where they passed `Some("aGVsbG8=")`, first `std::fs::write(dir.join("t.jpg"), b"hello")` and pass `Some("/uploads/t.jpg")`; `db_tests::custom_task_stores_photo_on_disk` becomes "stores the given path and the file remains". `base64` stays a dependency for `tls.rs`.

### Q1-07 — parent session on uploads and delete

`src/server/api/photos.rs::upload_photo_handler` and `screensaver.rs::upload_screensaver_image_handler`: read an `auth` text field; before processing `photo`, `if crate::server::auth::require_session(auth.as_deref().unwrap_or_default()).is_err() { return (StatusCode::UNAUTHORIZED, "a parent session is required").into_response(); }` (the field must precede `photo` in the body; document that in the handler doc table and have the client append it first). `delete_custom_task(auth: SessionToken, user_id: u32, task_id: u32)` calls `require_session(&auth)` first. Client: `routine.rs::upload::submit` gains `auth: Option<String>` (`session::token()`), the JS appends `form.append('auth', auth ?? '')` first; `CustomTaskRow::on_delete` passes `session::token().unwrap_or_default()`; `PhotoTaskDialog`'s Save button is `disabled` and shows "Sign in with the parent PIN under Settings to add tasks" when `!session::is_parent()`. Tests: `photo_tests.rs`/`screensaver_tests.rs` mint a token with `family_calendar::server::auth::issue_session()` and add `text_part("auth", token)` as the first part; add `t2_5_g_an_upload_without_a_parent_session_is_401_and_writes_nothing` and the screensaver twin.

### Q1-08 — transactional idempotency and unique keys

1. `src/server/db.rs`: make `claim_mutation`, `set_routine_completion` and `set_custom_task_completion` generic over `impl sqlx::SqliteExecutor<'_>` (replace `pool: &SqlitePool` with `executor: impl sqlx::SqliteExecutor<'_>`; the bodies are unchanged).
2. `src/server/api/routine.rs`, both toggles:
   ```rust
   let mut tx = pool.begin().await.map_err(super::to_server_error)?;
   let claimed = crate::server::db::claim_mutation(&mut *tx, &idempotency_key, "toggle_routine_task", user_id, &payload).await.map_err(super::to_server_error)?;
   if !claimed { return Ok(()); }
   crate::server::db::set_routine_completion(&mut *tx, user_id, template_id, completed, &date).await.map_err(super::to_server_error)?;
   tx.commit().await.map_err(super::to_server_error)?;
   ```
   (a failed write rolls the claim back, so the replay can succeed).
3. `src/client/components/routine.rs::new_idempotency_key`: `format!("{:016x}-{}-{n}", client_nonce(), now_millis())` where `fn client_nonce() -> u64` is a `OnceLock<u64>` seeded from `crate::client::realtime::entropy_seed()` (make it `pub(crate)`) mixed with `unit_random()`.
4. `tests/http_tests.rs`: `init_test_env` does `let _ = std::fs::remove_dir_all(&base);` before `create_dir_all`, and the two constant keys become `format!("http-test-toggle-routine-roundtrip-{}", std::process::id())` / `…-error-{pid}`. Add to `tests/routine_tests.rs`: a toggle for `user_id = 99` (FK failure) followed by the same key for a valid user must apply — proving the failed claim was released.

### Q1-09 — ordered stroke persistence

`src/server/api/realtime.rs`:
```rust
struct PendingStroke { board_id: i64, seq: i64, client_id: String, stroke: Stroke }
static STROKE_TX: OnceLock<tokio::sync::mpsc::UnboundedSender<PendingStroke>> = OnceLock::new();

fn stroke_writer() -> &'static tokio::sync::mpsc::UnboundedSender<PendingStroke> {
    STROKE_TX.get_or_init(|| {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<PendingStroke>();
        tokio::spawn(async move {
            while let Some(first) = rx.recv().await {
                let mut batch = vec![first];
                while let Ok(more) = rx.try_recv() { batch.push(more); }
                let Ok(pool) = crate::server::db::pool().await else { continue };
                let Ok(mut tx) = pool.begin().await else { continue };
                for p in &batch {
                    let points = crate::server::db::stroke_points_json(&p.stroke);
                    if let Err(err) = crate::server::db::insert_stroke_at_seq(&mut *tx, p.board_id, p.seq, &p.client_id, &p.stroke.color, p.stroke.width, &points).await {
                        tracing::error!(%err, seq = p.seq, "failed to persist stroke");
                    }
                }
                if let Err(err) = tx.commit().await { tracing::error!(%err, "stroke batch commit failed"); }
            }
        });
        tx
    })
}
```
`record_stroke` mints `seq` as today and `let _ = stroke_writer().send(PendingStroke { .. });`. `insert_stroke_at_seq` takes `impl sqlx::SqliteExecutor<'_>`. Strokes now commit in `seq` order (one writer, one transaction per burst — 240 msg/s becomes a handful of commits per second, so `t1_2_3`'s 250 ms budget holds), and `snapshot` should additionally return `latest` as the highest *contiguous* seq (`for row in rows { if row.seq != latest + 1 { break } latest = row.seq }`) so a bookmark can never skip a row still in the channel. Keep `wait_for_row_count` in the tests (harmless); run `loop_tests` 20× and `whiteboard_tests` 20× to confirm the flake is gone and record the counts in `docs/VERIFICATION.md`.

### Q1-10 — bound what a client may send

`src/server/api/realtime.rs`: `pub const MAX_WS_MESSAGE_BYTES: usize = 256 * 1024;` and in `ws_handler`: `upgrade.max_message_size(MAX_WS_MESSAGE_BYTES).max_frame_size(MAX_WS_MESSAGE_BYTES).on_upgrade(...)`. Add `pub fn valid_stroke(stroke: &Stroke) -> bool`:
```rust
stroke.points.len() >= 1 && stroke.points.len() <= MAX_STROKE_POINTS
    && stroke.color.len() <= 32 && stroke.color.starts_with('#') && stroke.color[1..].chars().all(|c| c.is_ascii_hexdigit())
    && stroke.width.is_finite() && (0.5..=64.0).contains(&stroke.width)
    && stroke.points.iter().all(|p| p.x.is_finite() && p.y.is_finite() && (0.0..=1.0).contains(&p.x) && (0.0..=1.0).contains(&p.y))
```
used in the `Draw` arm (drop + `tracing::warn!` on failure). Unit tests for each rejected shape, and one `realtime_tests` case that a 1 MiB text frame closes only the sending socket (tungstenite `Capacity` error) while another client keeps receiving. The test-side `stroke_with(marker, ..)` helpers that put a marker in `color` (`loop_tests`, `whiteboard_tests`, `realtime_tests`) must move the marker into `width`-independent data — use `"#" + 6 hex digits` derived from the index (`format!("#{:06x}", i)`) and decode it back in the assertions.

### Q1-11 — cookie session (or a PLAN amendment)

If the Boss keeps the contract: `src/server/router.rs` adds `POST /api/login` (`axum::Json<{pin: String}>` → `auth::verify_pin` → `Set-Cookie: fh_session=<token>; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age=2592000`) and `POST /api/logout` (revoke + expired cookie); `src/server/auth.rs` adds `pub fn session_from_headers(headers: &HeaderMap) -> Option<String>` (parse `cookie` for `fh_session`) and `pub async fn require_parent() -> Result<(), ServerFnError>` that inside a server fn does `let headers: HeaderMap = extract().await?; require_session(&session_from_headers(&headers).unwrap_or_default())`; every `auth: SessionToken` parameter in `api::profiles`/`api::calendar` becomes optional (`if auth.is_empty() { require_parent().await? } else { require_session(&auth)? }`); `ws_handler(upgrade, headers: HeaderMap)` sets `Connection { parent_cookie: session_from_headers(&headers).is_some_and(|t| is_valid_session(&t)), .. }` and `authorised()` ORs it — **and** because cookies are now ambient, `ws_handler` and `/api/login` must reject requests whose `Origin`/`Sec-Fetch-Site` header is not same-origin (`http(s)://<Host>`), returning 403. `mobile/session.rs` keeps only `is_parent()` (probe `GET /api/session` → 204/401) and `settings.rs` posts to `/api/login`. Tests: `profiles_tests` login → cookie flags asserted; WS `SetView` with the cookie and no `auth` is delivered; a cross-origin `Origin` on `/ws` is 403. If the Boss instead amends PLAN T1.4 and §5.9 default 31 to "bearer token held by the PWA, passed explicitly", record it in HANDOFF and close H-19.

### Q1-12 — Loading/Empty/Error on the television

Exactly HANDOFF T2.4 H-22: `TvModel.events: CalendarState<Vec<CalendarEvent>>` (`use crate::client::components::calendar::CalendarState`); `tv/fixture.rs` and every `TvModel { events: ... }` in `tests/tv_tests.rs`/`tests/palette_tests.rs` wrap with `CalendarState::Ready(vec)` (or `Empty`); `tv/shell.rs`: `events: CalendarState::resolve(events_resource.read_unchecked().clone().map(|r| r.map_err(|e| e.to_string())), Vec::is_empty)`; `TvLayout::of` uses `model.events.ready().map_or(0, Vec::len)` (add `pub fn ready(&self) -> Option<&T>` to `CalendarState`); `surface.rs::calendar_panel` renders `Loading` → "Loading the calendar…", `Error(_)` → "Can't reach the hub's calendar — check the hub" (both `TV_HEADING text-slate-600`), `Empty` → the existing sentence, `Ready` → the list. The golden focus file does not move (no ids in the three non-`Ready` arms). Add `tests/tv_tests.rs::t2_4_e_a_failed_calendar_fetch_is_not_rendered_as_an_empty_day`.

### Q1-13 — send `Health`

`src/server/router.rs::run`, after `screensaver::ensure_background_tasks()`:
```rust
tokio::spawn(async {
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(25));
    loop {
        tick.tick().await;
        crate::server::api::realtime::publish(&crate::shared::types::ServerMessage::Health {
            stale: false,
            last_update: chrono::Local::now().to_rfc3339(),
        });
    }
});
```
(`router::run`, not `ws_handler`, so `tests/realtime_tests.rs`'s "nothing arrives" assertions keep their meaning). In `tv/shell.rs` the proof-of-life effect also reads `(bus.stale)()` and `(bus.connected)()`; `CLOCK_POLL_SECS` may then rise to 60. `docs/PROTOCOL.md` §"Health": "sent every 25 s; `stale` is reserved (always `false`) until a Google poll exists to be stale about". Test: `tests/realtime_tests.rs` boots `build_http_router` via `router::run` is too heavy — instead add `realtime::spawn_health_heartbeat(interval)` and a test that a connected client receives `Health` within 2× the interval.

### Q1-14 — a schedule that can be enabled and dismissed

`src/server/config.rs`: `pub screensaver_schedule_hour: Option<u32>` from env `FAMILY_HUB_SCREENSAVER_HOUR` or `[screensaver] schedule_hour` (0..=23, else `None` + warn), logged in `ensure_dirs_and_log`. `screensaver::ensure_background_tasks(schedule: ScreensaverSchedule)` built by `router::run` from the config (`enabled: hour.is_some()`); the self-start calls pass `ScreensaverSchedule::default()`. `src/client/components/screensaver.rs`: in the activity future, when `current != last_seen && (state.current_view)() == MaximizedView::Screensaver { state.current_view.set(MaximizedView::None) }` (window `keydown` already feeds `activity`, so the remote dismisses it); drop the two pointer handlers on the overlay `div` (activity covers them) so `screensaver.rs` can join `tests/tv_tests.rs::the_kiosk_never_reaches_for_a_pointer_event`. Config test in `tests/config_tests.rs` for both sources; unit test that a scheduled overlay clears on activity.

### Q1-15 — AA on the phone's Routine tab

`src/client/components/routine.rs`: line 365 `text-red-500 hover:bg-red-50` → `text-red-700 hover:bg-red-50` (6.5:1); line 156 `text-sm font-bold text-sheffield-accent` → `rounded-full bg-sheffield-accent px-2 text-sm font-bold text-slate-800` (4.62:1); line 256 `bg-sheffield-accent … text-white` → `bg-sheffield-dark … text-white` (4.99:1) and the same on `whiteboard.rs:137`; line 329 `text-sheffield-light` → `text-slate-600`; line 287 `bg-sheffield-light text-xl font-bold text-white` → `bg-sheffield-light text-xl font-bold text-slate-800` (9.0:1). Then `tests/palette_tests.rs::surface_sources()` walks `src/client/components/**/*.rs` (skip `palette.rs`), `PALETTE_TOKENS` gains `red-50 #FEF2F2`, `red-200 #FECACA`, `red-600 #DC2626`, `red-700 #B91C1C`, and `PALETTE_PAIRS` gains (`text-red-700`, `bg-red-50`, body), (`text-red-600`, `bg-white`, body), (`text-white`, `bg-red-600`, body), (`text-red-700`, `bg-white`, body). Rebuild `assets/tailwind.css` once (Boss) — the new classes must be in the committed CSS or CI's fail-on-diff step goes red.

### Q1-16 — T3.3 contract

`tests/docs_tests.rs::t3_3_every_task_id_appears_exactly_once_in_verification`: after the count assertion, `let row = verification_content.lines().find(|l| l.starts_with(&pattern)).expect("row"); assert!(row.contains("| PASS |"), "{task_id} is not PASS: {row}");` and `assert!(!verification_content.contains("| FAIL |"))`. `docs/VERIFICATION.md`: add "## Transcripts" with one fenced block per task containing the exact command (`cargo test --features server --test <file> -- <filter>`) and the `test result: ok. N passed … finished in X s` line from a fresh run, plus the wall clock of the full suite; update the results table's Summary column to cite the transcript. Keep every task ID appearing once in a `| Txx |` row (the transcript headings must not use that form).

### Q1-17 — exact pins in the xtask

`xtask/Cargo.toml`: `resvg = "=0.45.1"`, `usvg = "=0.45.1"`, `tiny-skia = "=0.11.4"`, `image = "=0.25.10"`, `anyhow = "1.0"` (not a §P5.4 crate, may stay a range); root `Cargo.toml` dev-deps `resvg = "=0.45.1"`, `rqrr = "=0.10.1"`. `cargo update -p resvg@0.41.0 --precise 0.45.1` (or `cargo update` for the workspace) so `Cargo.lock` carries one resvg/usvg tree; re-run `cargo run -p xtask -- icons` and commit the regenerated PNGs if their bytes change (the `docs_tests` dimension/safe-zone assertions must still pass). Add to `tests/ci_tests.rs`: parse `xtask/Cargo.toml` and assert every `resvg`/`usvg`/`tiny-skia` line contains `"=`.

## Low observations (not blocking, no action required this round)

- `/tv` on the HTTP origin links `/manifest.webmanifest`, which 308s to HTTPS on every kiosk load (recorded by the Boss).
- `upload_photo_handler` writes the file before the row insert; a bad `user_id` leaves an orphan file (reaped by the 30-day purge). `delete_profile`'s cascade also orphans photo files until the purge.
- `create_profile` validates neither `name` length nor `color` format; `TvProfile` falls back to the primary blue for an unparsable colour, so this is cosmetic.
- `mutation_log` has no retention; at family scale it is a few thousand rows a year.
- `docs/DEV_WINDOWS.md` step 5 claims the build "runs Tailwind CLI" — folded into Q1-01's doc fix.
- `service.rs::tv_ip_from_env_or_device_toml` reads `docs/device.toml` relative to the CWD; acceptable for an owner-run CLI, not for the service path (it is never called there).
- `FamilyHubConfig::load()` re-reads `familyhub.toml` on every server-fn call (`api::profiles::data_dir`, `db::upload_dir`, `screensaver_dir`); cache it in a `OnceLock` when convenient.
