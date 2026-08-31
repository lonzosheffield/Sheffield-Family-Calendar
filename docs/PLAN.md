# Sheffield Family Calendar & Routine Hub — Plan v2

**Status:** v2 — post red/purple/white review. **APPROVED by the owner 2026-08-29** with one addition (Phase 3.5, Fable QA loop). **EXECUTED 2026-08-29 → 2026-08-30:** all 34 tasks merged to `main` (runs `wf_a4f253d4-9d7` + `wf_d57bfb45-d60` + Boss follow-up); Fable QA loop reached **VERDICT: PASS at round 4** (17 → 7 → 3 → 0 Med+ findings; `docs/qa/`). Remaining owner steps: `docs/OWNER_CHECKLIST.md`. Low observations: `docs/qa/QA_ROUND_4.md`, `docs/RESIDUAL.md`. **Phase 4 / P4.1 design pass EXECUTED 2026-08-30** from the owner's wall poster (`docs/design/inspiration/`, direction in `docs/design/DESIGN_DIRECTION.md`): tasks D4.1–D4.4 merged; fresh-context Fable design QA **FAIL (2 Med) → fixed → PASS** (`docs/qa/QA_DESIGN_ROUND_{1,2}.md`). Profiles named Isaiah, Nathaniel, Simeon, Ezekiel (migration 0004). Still open: P4.2 DNS-01 cert (optional), P4.4 Raspberry Pi move, P4.5 sqlx 0.9 bump, and the Low observations in the two design QA reports.
**Author:** Fable 5 (boss/orchestrator). Reviews: `docs/reviews/RED_TEAM.md` (31 findings), `docs/reviews/WHITE_TEAM.md` (verdict REWORK, 12 required changes), `docs/reviews/PURPLE_TEAM.md` (resolutions). v1 preserved at `docs/reviews/PLAN_v1.md`.
**Date:** 2026-08-29 · **Repo:** https://github.com/lonzosheffield/Sheffield-Family-Calendar · **Local:** `C:\Family Calendar`

---

## 0. Objective and non-negotiables

Build the Sheffield Family Calendar & Routine Hub **in Rust, whole stack**, **local-first on the home LAN**, with a **Fire TV in kiosk mode** as the primary display and a **companion PWA** on phones. Everything the README promises — morning routine per child, calendar, collaborative whiteboard, photo tasks, ambient screensaver — must work.

1. **Rust is the stack.** Viewers (a browser on the TV, a browser on a phone) are not stack. Every non-Rust component is declared, priced and justified in `docs/NON_RUST.md` (9 rows, pre-filled by T0.1): `sw.js` (~40 lines, served from Rust), Tailwind standalone binary (build-time only), Fully Kiosk Browser (kiosk shell, one device branch), `adb`, GitHub Actions YAML, wasm-bindgen glue, browser/WebView, Amazon Silk, Android SDK (only if Phase C is ever revived — it is not planned).
2. **Local-first** = no cloud dependency; works with the internet unplugged. SQLite is the source of truth; Google Calendar is an optional feed. Single-writer server; no multi-master sync.
3. **The Fire TV is a display driven by a D-pad remote** (no touch). **Scope line:** routine completion, profile switching, panel navigation, screensaver dismiss and the join-QR overlay are fully operable from the remote. Drawing, photo capture, calendar editing and administration are phone-only.
4. **PWA** on phones (Android Chrome; iOS Safari with a one-time trusted CA) — installable, offline app shell, queued mutations.
5. **Autonomy means code + server-side validation autonomy.** Every task's acceptance test runs unattended on this Windows PC with no second device and no human. Anything needing the TV, a phone, a reboot or elevation is in **Appendix A — Owner Verification Checklist**, executed by the owner *after* the run.

---

## 1. State of the code (verified by build: `docs/BASELINE.md` — 8/8 tests, clippy clean both targets)

2,029 lines of Rust. Dioxus 0.6.3 fullstack skeleton, SQLite via sqlx, `#[server]` fns, axum WebSocket broadcast, canvas whiteboard, Google service-account poller. The skeleton is sound; the gaps below are real and each maps to a task.

| # | Finding | Sev | Fixed by |
| --- | --- | --- | --- |
| G1 | Whiteboard and stroke-width slider are pointer-only; focus order is nondeterministic. Native `<button>`s are D-pad-reachable already, so the TV is *partly* usable today. | Critical | T2.1, T2.3 |
| G2 | No kiosk deployment story (boot, wake, chrome-hiding). | Critical | T0.0, T3.2 |
| G3 | WS client never reconnects (`realtime.rs:73,99`); outbound dies with it. | Critical | T1.2 |
| G20 | **Server closes the socket on `broadcast::RecvError::Lagged`** (`api.rs:218`, capacity 256) while the whiteboard sends one message per `pointermove` — a child scribbling bricks the TV until power-cycle. | Critical | T1.2 |
| G4 | No midnight rollover; `today()` isn't a reactive dependency (`routine.rs:20-26`). | High | T1.2, T1.5 |
| G5 | Whiteboard strokes never persisted; late joiners see a blank board. | High | T2.3 |
| G21 | Single-slot `inbound_stroke` signal drops segments that arrive between renders; no `ResizeObserver`, so maximise/restore wipes the bitmap. | High | T2.3 |
| G6 | PWA stub: `icons: []`, no service worker, **and** the manifest is served from a hashed `/assets/` path so its scope excludes `start_url` — install fails even with icons. | High | T0.6, T2.2 |
| G7 | No TLS → phones have no secure context → no SW / install / camera capture. | High | T1.3 |
| G8 | Screensaver URLs 404 (no `ServeDir` for `assets/screensaver`) and the directory is empty. | High | T0.6, T0.7, T2.7 |
| G9 | `/mobile` is routine-only; no calendar/board/TV-remote. | High | T2.2 |
| G10 | Calendar is today-only, Google-only, in-memory (`OnceLock` cache — empty after every restart), never returns to empty (`calendar.rs:16-23`), DST `.single()` fallback silently treats local as UTC. | High | T2.4 |
| G14 | Photo upload is base64 through a server fn; **axum's default body limit is 2 MB** → every modern phone photo gets a 413. Extension always `.jpg`. | High | T2.5 |
| G22 | **`toggle_custom_task` never broadcasts** (`api.rs:110-124`) — phone ticks never reach the TV. No ownership check; any client can toggle any sibling's task. | High | T1.5 |
| G23 | All data paths are CWD-relative (`db.rs:11,14`, `api.rs:144`) — under a Windows service they land in `C:\Windows\System32`. | High | T0.5 |
| G24 | SQLite journal mode never set (no WAL), no `busy_timeout`, single pool → readers block writers 24/7. | Med | T1.1 |
| G25 | Nothing is ever deleted (tasks, photos, strokes) and there is no backup; a file copy of a live WAL DB is corrupt. | High | T1.6 |
| G11 | Profiles hardcoded `"Boy 1..4"`; two CHECK constraints (`db.rs:62,77`); TV shows bare digits. | Med | T1.4 |
| G12 | Custom tasks undated, never expire. | Med | T1.5, T2.5 |
| G13 | Server rebroadcasts any client JSON verbatim, incl. `CalendarUpdated` (spoofable); every client draws its own strokes twice (self-echo). | Med | T1.2 |
| G15 | Dioxus 0.6.3 is superseded and unmaintained (0.7.10 current). | Med | T0.4 |
| G16 | README toolchain is Linux-only; `sqlx` carries a pointless `tls-native-tls` (OpenSSL on Linux); no CI. | Med | T0.1, T0.2, T0.8 |
| G17 | No auth at all; parent-only actions need a server-enforced PIN. | Med | T1.4 |
| G18 | Binds `127.0.0.1` via `fullstack_address_or_localhost()` (reads bare `IP`/`PORT`). | Med | T0.5 |
| G26 | Test suite is DB-only (8 tests); router, WS, server fns, poller are untested — a migration could break all of them and stay green. | Med | T0.3 |
| G19 | No design assets; UI is generic Tailwind. Inspiration images not yet received. | Low | T3.4, Phase 4 |

---

## 2. Architecture decisions (v2)

### D1 — TV is a display; phones are controllers; **the TV is self-sufficient for the routine**
A child completes their whole routine on the TV with the remote (Up/Down = profile, Left/Right = panel, Enter = toggle, Back = restore). Phones do everything richer, and can remote-control the TV (`SetView`, `SetActiveProfile`) — gated by a parent session or scoped to the sender's own profile.

### D2′ — Kiosk shell is a **runbook choice with zero code impact**, selected per detected device
Because of D3′ the TV just loads a URL over plain HTTP. Three branches, all documented by T3.2, one promoted by T0.0's probe:
- **A — Fire OS** (Fire OS 5/6/7/8/14/16): Fully Kiosk Browser ≥ 1.61.2 (PLUS licence ~$11 one-off), boot-launch after `SYSTEM_ALERT_WINDOW` + `GET_USAGE_STATS` granted over adb, `settings put secure sleep_timeout 0`, screensaver → Never, HDMI-CEC off on the television. Vendor disclaims Fire OS; declared in `NON_RUST.md` with an exit criterion.
- **B — Vega OS** (Fire TV Stick 4K Select 2025 / Stick HD 2026: no sideloading at all): Amazon Silk with a bookmark to the kiosk URL; no boot auto-launch ("Alexa, open Silk" after a power cut).
- **B′ — Vega + boot resilience wanted**: a ~$35 Google TV box or a retired Android tablet running Fully Kiosk. Priced upgrade, not a dependency.
- Phase C (Rust-native Android shell) is **cut, not deferred** — WebView app you'd own, Leanback manifest injection, full Android toolchain, impossible on Vega.

### D3′ — **Split origins: TV on HTTP, phones on HTTPS**
```
:8080  HTTP   /tv  kiosk (TV)  + /ws /assets /uploads /ca.crt /health     — 308→HTTPS only for /m*, manifest, sw.js
:8443  HTTPS  /m   phone PWA   + wss, /manifest.webmanifest, /sw.js, everything
```
The TV needs a page, a WebSocket and a canvas — none require a secure context. This dissolves the v1 conflict (Android WebView rejects user-installed CAs; Fire TV has no CA install UI on Fire OS 8+). Phones get an `rcgen` local CA (10 y) + leaf (**397 days**, explicit `not_before`/`not_after`, SANs = reserved IP + all non-loopback IPv4 + `familyhub.local` + `localhost`), auto re-issued at 30 days remaining with hot reload, served by `tokio-rustls` (`ring` provider, installed first line of `main`). CA private key ACL-restricted and excluded from backups. `CertSource` is a trait so a DNS-01 public cert (`instant-acme`) can be added in Phase 4 with no rewrite. Kiosk URL of record: `http://<dhcp-reserved-ip>:8080/tv`; `.local` via one `mdns-sd` daemon is a convenience (won't resolve on Fire OS 7/8). The TV's QR overlay encodes the **raw-IP HTTPS phone URL**.

### D4 — SQLite is the source of truth
`sqlx::migrate!` embedded, numbered migrations with one owner, `build.rs` rerun-if-changed, startup baselining of an existing v1 `family.db`. WAL, `synchronous=NORMAL`, `busy_timeout=30s`, **two pools** (read 5 / write 1). Tables: `profiles`, `events` (local + Google, windowed full-replace polling — no sync token), `whiteboard_strokes` (one row per **stroke**, `seq`, `cleared_at` watermark, keep last 2,000), `custom_tasks.due_date`, `settings`, `mutation_log` (idempotency). Nightly `VACUUM INTO` backups (14 retained) + uploads snapshot + restore drill. Delete paths for tasks (+ photo file), stroke compaction, 30-day photo retention, log rotation.

### D5 — Realtime protocol v2 (normative spec: `docs/reviews/PURPLE_TEAM.md` §P2c, landed as `docs/PROTOCOL.md` by T1.2)
Server-authoritative `ClientMessage`/`ServerMessage` split; server-minted `ClientId` stamped on fan-out (echo suppression, unspoofable); strokes batched one message per animation frame (≤ 30/s); server token bucket 40/s burst 80; `Lagged` → resubscribe + `Resync`, **never close**; per-connection outbound queue 256 drop-oldest; broadcast capacity 1024; heartbeat 20 s / dead at 2 missed; reconnect `1,2,4,8,15,30…` s ±20 % jitter; midnight tick recomputed every iteration with `.earliest()` (DST-safe) that also forces a calendar poll and runs retention. Messages: `Hello, Pong, Resync, Draw, BoardCleared, Snapshot, RoutineUpdated{user_id,date}, TasksUpdated, ProfilesUpdated, CalendarUpdated, DayRolled, SetView, SetActiveProfile, Health`.

### D6 — PWA
`/manifest.webmanifest` and `/sw.js` served from **root** by explicit axum routes (`scope: "/"`, `start_url: "/m"`, icons 192/512 + maskable generated by a Rust `xtask` with `resvg`). `sw.js` ≈ 40 lines from `include_str!` — app-shell precache, network-first server fns, cache-first uploads. Offline queue is a pure-Rust struct in `localStorage`: every mutation carries its **intended date + idempotency key**. Per-platform promise documented: Android replays on reconnect; **iOS replays on next app open** (no Background Sync).

### D7′ — Dioxus 0.6.3 → **=0.7.10**, in Phase 0, one path (no 0.6 fallback)
Real break list (v1 was wrong): (1) no `server_fn` crate — server fns rewritten in `dioxus-fullstack-core`; (2) `ServerFnError` is a non-generic Dioxus enum → `api.rs:175-177` and every call site; (3) `ServeConfigBuilder` removed, `serve_dioxus_application` now returns `Router<()>` → `main.rs:21-31` won't compile; (4) axum `^0.8.4`, `tower-http ^0.6.8`; (5) **`multipart` already enabled** — free for T2.5; (6) `event.files()` → `Vec<FileData>` (`routine.rs:229-233`); (7) **`onsubmit` submits by default** unless `prevent_default()` — silent; (8) server-fn codec form → JSON; (9) prelude shrunk; (10) `asset!` and `fullstack_address_or_localhost()` unchanged (the latter removed from release path anyway); (11) pin `=0.7.10`, `cargo binstall dioxus-cli@0.7.10`, never `0.8.0-alpha`. Gated by the 14-point test in §3 T0.4, which names the break each assertion proves.

### D8 — 10-foot UI
Deterministic focus order (golden file), thick `ring-sheffield-sun` focus ring, ≥ 28 px body / ≥ 44 px headings, 5 % overscan padding, no `hover:`-only affordances, key map `ArrowUp/Down/Left/Right/Enter/Backspace/MediaPlayPause` (**no Esc** — Fire TV remotes have none), `?keys=1` key-code debug overlay so the owner can report real codes, permanent "updated HH:MM" + red disconnected badge after 90 s of silence.

### D9 — Runs on this Windows 11 PC as a service
`family-hub.exe install|uninstall|start|stop|status|run|tv-probe` via `windows-service::service_manager` (no PowerShell scripts). Tokio runtime built inside `service_main`; file + Event Log logging as the first statement; all paths absolute from `FAMILY_HUB_DATA_DIR` (default `%ProgramData%\FamilyHub`) and logged at startup. Docker not used (breaks mDNS). aarch64 CI job cut (code stays portable).

---

## 3. Tasks (34 in the run; every acceptance test is agent-executable on this PC)

Tiers: **H** Haiku · **S** Sonnet · **O** Opus · **B** Boss. Full acceptance detail for each task is in `docs/reviews/PURPLE_TEAM.md` §P3 and is normative; the table below is the contract. Agents never weaken an acceptance test.

### Phase 0 — floor
| ID | Title | Deps | Tier | Acceptance (summary) |
| --- | --- | --- | --- | --- |
| T0.0 | Device-ID gate: probe TV via adb (IP from `FAMILY_HUB_TV_IP` / `docs/device.toml` / paired devices), classify FIRE_OS / VEGA_OS / UNKNOWN, write `docs/FIRE_TV.md` with all three branches (detected one promoted) + `docs/OWNER_CHECKLIST.md` Device row. **Never fails the run on outcome.** | — | S | `#[test]`: `FIRE_TV.md` line 1 matches `^STATUS: (FIRE_OS\|VEGA_OS\|UNKNOWN)`; headings `## Branch A/B/B′` present; checklist has a Device row |
| T0.1 | `docs/NON_RUST.md` (9 rows), `docs/DEV_WINDOWS.md` (step 1 = PATH prefix; Tailwind standalone `tailwindcss-windows-x64` v3.4.17), fix `tailwind.config.js` dead `./index.html` glob | — | H | `#[test]` asserts ≥ 9 rows + required strings; config has no `index.html` |
| T0.2 | Dependency hygiene: drop `sqlx` `tls-native-tls`; pin `sqlx =0.8.6`; add `web-sys` features (`Navigator, ServiceWorkerContainer, ServiceWorker, ResizeObserver, File, Blob, FormData, Performance`); `build.rs` rerun-if-changed=migrations; add `uuid`, `image`, dev `tokio-tungstenite` | — | S | `cargo tree` shows no `openssl-sys`/`native-tls`; clippy ×2 clean; 8/8 tests |
| T0.3 | HTTP/WS integration harness **on 0.6.3** (`tests/http_tests.rs`: in-process server, ephemeral port; GET `/`, `/mobile`; one server-fn round trip; `/ws` upgrade + fan-out) | T0.2 | S | ≥ 5 new `http_*`/`ws_*` tests, each asserting a status/body; suite green twice back-to-back |
| T0.4 | **Dioxus 0.7.10 migration** (D7′, all 11 items) | T0.3 | O | 14-point gate: fmt, clippy ×2 `-D warnings`, all tests green; `/`+`/m` 200 with markers; JSON server-fn round trip; mutating fn + structured error; WS fan-out; `Vec<FileData>` handler compiles; `cargo tree -d` no dup axum/tower-http/hyper; no `server_fn`/`ServeConfigBuilder` in tree; every form audited for `prevent_default`; `dx build --platform web --release` exits 0 |
| T0.5 | `FamilyHubConfig` (`familyhub.toml` + env): `FAMILY_HUB_DATA_DIR`, `FAMILY_HUB_ADDR` (0.0.0.0:8080), `FAMILY_HUB_TLS_ADDR` (0.0.0.0:8443); all paths absolute + logged; remove `fullstack_address_or_localhost()` from release path | T0.4 | S | `#[test]` boots with temp data dir → `family.db` there **and not** in CWD; `grep` for relative path literals in `src/` empty; default addr asserted |
| T0.6 | `build_router() -> Router` + `run(config)`; root routes `/manifest.webmanifest`, `/sw.js`, `/ca.crt`, `/health` (stubs); `ServeDir` for `/uploads` and screensaver; `/` → 308 `/tv`; `/tv`, `/m`; **`src/main.rs` < 25 lines and frozen thereafter** | T0.5 | S | `oneshot` assertions on 9 routes; screensaver fixture → 200 `image/jpeg`; manifest → `application/manifest+json`; `main.rs` line count |
| T0.7 | Assets: 3 CC0 screensaver JPEGs; `tests/fixtures/photo_12mp.jpg`; PWA icons via Rust `xtask` (`resvg`/`tiny-skia`) from a Sheffield-palette monogram | T0.1 | H | `cargo run -p xtask -- icons` writes 3 PNGs with asserted dimensions + maskable safe zone; fixture decodes ≥ 4000×3000 |
| T0.8 | CI: fmt, clippy ×2, tests, `dx build --release`, Tailwind rebuild **fail-on-diff**, `cargo tree -d` check, Windows-x64 release. No aarch64. | T0.4 | S | YAML parsed by a `#[test]` for the 7 steps; each command also run locally, exit 0 |

### Phase 1 — foundations
| ID | Title | Deps | Tier | Acceptance (summary) |
| --- | --- | --- | --- | --- |
| T1.1 | Migrations & storage (sole owner of `migrations/`): `0001_init` (baselines existing DB), `0002_core` (events, strokes, due_date, sync state, settings, mutation_log); WAL/NORMAL/busy 30 s; two pools; `wal_checkpoint(TRUNCATE)` on tick; DST helper `.earliest()` | T0.6 | O | v1 `family.db` fixture migrates with every log row intact; restore drill; pragmas asserted; 20 concurrent writers zero `SQLITE_BUSY`; DST unit test |
| T1.2 | Realtime protocol v2 (D5 in full) + `docs/PROTOCOL.md`; splits `api.rs` into `api/{mod,realtime,routine,profiles,calendar,screensaver}.rs` | T0.6 | O | 9-point suite: backoff schedule; lag → `Resync`, socket stays open; 8×30 msg/s×30 s load, zero closes, p99 < 250 ms, RSS +< 50 MB; echo origin; spoof dropped; auth on `SetView`; kill+restart reconnect < 30 s with `Snapshot`; midnight on both DST dates for NY + London; rate-limit isolates the offender |
| T1.3 | TLS + PKI + dual listener + mDNS + QR (D3′). `CertSource::SelfSignedCa` behind a trait; `tokio-rustls` (not `axum-server`); `rcgen =0.14.10`; one `mdns-sd` daemon; `fast_qr` SVG | T0.6 | O | rustls client with the CA handshakes :8443 and gets `/health` 200; `/ca.crt` parses as CA; `/m` on :8080 → 308, `/tv` → 200; leaf validity 396–398 d; SANs cover host IPv4s; 29-days-left leaf triggers hot re-issue; mDNS A query answered; QR decodes to `https://<ip>:8443/m` |
| T1.4 | Profiles + settings + parent PIN: `0003_profiles` (drop both CHECKs → FKs); server fns; **6-digit** PIN, argon2id `=0.6.0`, server-enforced, session token (30 d, HttpOnly/Secure/Lax, HTTPS origin only), exponential backoff no lockout; first-run setup code to log + `<data>\setup-code.txt` (**not** shown on the TV — Boss decision `docs/HANDOFF.md` T2.1 H-24, amended here at the QA round 2 close per Q2-01). **Replace `tests/db_tests.rs:148-157` with an FK-violation test.** | T1.1, T1.2 | S | FK violation; rename emits `ProfilesUpdated` on a 2nd WS client; 5th/6th profile OK; 10 bad PINs → delays ≥ 2ⁿ ms; privileged fn without session errors |
| T1.5 | Date correctness + authz + missing broadcasts: explicit `date` on every mutation (±1 day validated); idempotency keys deduped via `mutation_log`; ownership checks; `toggle_custom_task` publishes `TasksUpdated`; `today().unwrap_or_default()` → explicit `Error` state | T1.1, T1.2 | S | yesterday's date writes yesterday's row; 3 days ago rejected; same key twice = one change; cross-user toggle rejected; `TasksUpdated` observed; error state unit test |
| T1.6 | Backup/retention/delete: nightly `VACUUM INTO` + uploads snapshot, 14 retained; delete task + photo; stroke compaction (keep 2,000); photo retention 30 d; log rotation 10 MB×5; PKI excluded | T1.1 | S | backup under open writer opens clean (file copy asserted to fail/differ); restore drill; 20→14 retention; file removed with task; compaction count; no `.key` in backups |
| T1.7 | `/health` JSON (db, last poll, cert `not_after` + days, disk free, WS clients, uptime, migration version) + TV staleness badge state machine | T1.1, T1.2, T1.3 | S | 8 keys typed; pool closed → 503; badge on at > 90 s, off within 2 s; expiry matches leaf |

### Phase 2 — the two surfaces
| ID | Title | Deps | Tier | Acceptance (summary) |
| --- | --- | --- | --- | --- |
| T2.1 | Kiosk 10-foot UI (D8): focus system, overscan, type scale, full routine by remote, receives `SetView`/`SetActiveProfile`, QR overlay, `?keys=1` overlay, Up/Down profiles, Left/Right panels, Backspace = Back | T1.2, T1.4 | O | golden focus-order file + focus-ring class on every focusable; injected `SetView`/`SetActiveProfile` change rendered view; pure key-handler transition tests; every routine item reachable in ≤ 12 presses; typography/overscan class allowlist grep. **No screenshot review.** |
| T2.2 | Phone PWA (D6): root manifest + `sw.js` (≤ 6 KB) + `web_sys` registration; tabs Routine · Calendar · Board · TV Remote · Settings; offline queue struct with date + idempotency; sends `SetView`/`SetActiveProfile`; `docs/PWA.md` per-platform promise | T1.2, T1.3, T1.4 | O | manifest fields asserted (scope `/`, start_url `/m`, ≥ 2 icons incl. maskable, no hashes); `sw.js` 200 `text/javascript` with install/activate/fetch; queue: 3 entries → 3 calls → replay idempotent → 48 h expiry drops with toast event; doc test. **No Lighthouse.** |
| T2.3 | Whiteboard v2 per `docs/PROTOCOL.md`: persistence per stroke, snapshot replay, `cleared_at`, undo-own-last, drained inbound queue, `ResizeObserver` + repaint-from-log, **one board** | T1.1, T1.2 | S | 500 strokes → fresh client `Snapshot` in seq order; clear → empty snapshot → rows compacted; undo removes only caller's last; 50 queued draws all rendered; resize triggers repaint |
| T2.4 | Calendar v2: events in SQLite, local CRUD, Today + Week (Sunday start, server-local time), windowed Google poll with full-window replace (fixture-driven, no service account), `rrule =0.14.0` `all(limit)` for local recurrence, `Loading/Empty/Error` states, tick forces poll. **ICS import cut.** | T1.1, T1.2 | O | fixture poll 3→2 removes the missing event; 02:30 daily rule across both US and UK DST transitions; DST week has 7 correct days; pathological RRULE bounded within 2 s; last event deleted → `Empty` |
| T2.5 | Photo tasks v2: axum `Multipart` route with `DefaultBodyLimit::max(25 MiB)` on that route only; client downscale ≤ 1600 px; server allowlist + re-encode (`image`) to jpg/png/webp; `nosniff` + `attachment` on `/uploads`; `due_date`; delete task + file | T1.1, T1.5, T0.7 | S | 12 MP fixture → 2xx < 3 s, stored ≤ 400 KB; unraised route → 413; `.svg` → 415 nothing written; PNG-as-.jpg re-encoded with correct ext; headers present; yesterday's task hidden; delete removes row + file |
| T2.6 | Cross-surface loop test (phone→TV) in Rust | T2.1, T2.2, T2.3 | S | two WS clients: authed `SetView` reaches TV < 1 s; unauthed not delivered; stroke arrives with `origin == phone`; kill+restart → both resync < 30 s |
| T2.7 | Screensaver completion: phone upload via T2.5 pipeline; placeholders wired; idle 10 min; scheduled `SetView(Screensaver)` off by default | T2.5 | S | list ≥ 3 with every URL 200 `image/jpeg`; upload appears; idle state machine fires at 600 s; schedule off emits nothing |

### Phase 3 — ship
| ID | Title | Deps | Tier | Acceptance (summary) |
| --- | --- | --- | --- | --- |
| T3.1 | Windows service (D9): subcommands via `service_manager`; runtime inside `service_main`; `StartPending` checkpoints; logging first; rotation; firewall + power-plan configured **by the `install` subcommand** (elevated — owner runs it, A3) | T0.5, T1.3, T1.6 | S | **CWD test:** `run` with CWD `C:\Windows\System32` creates `family.db` only under `%ProgramData%\FamilyHub`; deliberate startup failure logged within 5 s; 20 MB of logs → rotated files under cap; `install`/`uninstall` code paths unit-tested against a mocked `service_manager` (real install is A3) |
| T3.2 | Runbooks: `docs/FIRE_TV.md` final (three branches, detected one promoted), `docs/OWNER_CHECKLIST.md` (Appendix A), `docs/DEV_WINDOWS.md`, `docs/PWA.md`, `docs/RECOVERY.md` | T0.0, T3.1, T2.6 | O | string-match test for `sleep_timeout`, `HDMI-CEC`, `SYSTEM_ALERT_WINDOW`, `GET_USAGE_STATS`, `Screensaver`, `Silk`, PLUS price; checklist ≥ 8 numbered steps with pass criteria; recovery ≥ 4 failure modes; all internal links resolve |
| T3.3 | Verification pass: re-run every acceptance test → `docs/VERIFICATION.md`; **Boss additionally runs the release binary and opens `http://127.0.0.1:8080/tv` and `https://127.0.0.1:8443/m` in Chrome via browser automation, capturing screenshots** (closes the "no page ever rendered" gap without a human) | all | H + B | one row per task ID, none FAIL; screenshots embedded/linked |
| T3.4 | Palette-faithful polish on the existing Sheffield palette (no owner sign-off) | T2.1, T2.2 | O | WCAG AA contrast computed in Rust for every token pair; ≤ 6 type sizes all ≥ 28 px on `/tv`; overscan class on every `/tv` container; no `hover:`-only affordance on `/tv` |

### Phase 3.5 — Fable QA loop (owner's addition, approved 2026-08-29)
| ID | Title | Deps | Tier | Acceptance (summary) |
| --- | --- | --- | --- | --- |
| T3.5 | **Fable QA in a fresh context.** A Fable 5 agent with no prior conversation context reads `docs/PLAN.md`, `docs/reviews/PURPLE_TEAM.md` §P3, `docs/VERIFICATION.md`, and the full diff of `main` since commit `5769946`, then audits every task's work by Opus, Sonnet and Haiku against its acceptance contract *and* against the standard (correctness, Rust idiom, security on a kids' LAN, 24/7 robustness, no weakened tests, no undeclared non-Rust). It writes `docs/qa/QA_ROUND_<n>.md`: verdict **PASS** or a numbered findings list, each with task ID, file:line, severity, and a **concrete solution**. Every finding is sent back to the **originally assigned tier** for that task (escalating per §5 if it fails), which applies Fable's solution; then a **new fresh-context Fable QA** runs. Loop until **PASS** or 4 rounds; after round 4 any remaining findings go to `docs/RESIDUAL.md` with Fable's solution attached. | T3.3, T3.4 | **Fable (fresh context)** + original tiers | `docs/qa/QA_ROUND_<final>.md` first line is `VERDICT: PASS`; each earlier round's findings are all marked `FIXED` in the following round; baseline + full test suite green on `main` after the final round |

### Phase 4 — post-run, owner-gated (outside the autonomous run)
P4.1 design pass when inspiration images arrive (default: T3.4's output ships). P4.2 `CertSource::AcmeDns01` via `instant-acme` if the owner buys a domain or iOS refuses the private root (the trait seam from T1.3 makes this a bolt-on). P4.3 device install per Appendix A. P4.4 Raspberry Pi move. P4.5 `sqlx` 0.9 bump.

---

## 4. Orchestration

**Waves** (parallel within a wave; Boss squash-merges and re-runs the baseline between waves):
0-a `T0.0 T0.1 T0.2` → 0-b `T0.3` → 0-c `T0.4` → 0-d `T0.5` → 0-e `T0.6 T0.7 T0.8` → 1-a `T1.1 T1.2 T1.3` → 1-b `T1.4 T1.5 T1.6 T1.7` → 2-a `T2.1 T2.2 T2.3 T2.4 T2.5` → 2-b `T2.6 T2.7` → 3 `T3.1 → T3.2 → T3.3`, with `T3.4` parallel to T3.1/T3.2.

**File ownership** (no two parallel tasks touch the same file — full table in `PURPLE_TEAM.md` §P4): `main.rs` frozen after T0.6; `router.rs` T0.6 → T1.3 → T2.5 (different waves); `migrations/` T1.1 only, T1.4 adds `0003` only, later numbers assigned by Boss; `shared/types.rs` T1.2 then T1.4; `api/` split by T1.2 so T1.4/T1.5 own different files; `Cargo.toml` T0.2 then T0.4, later additions via Boss micro-commits between waves. A task needing a file it doesn't own writes `docs/HANDOFF.md`; Boss applies between waves.

**Roster:** Boss (Fable) plans, assigns, reviews every diff against the acceptance contract, merges, pushes, runs T3.3's browser check. Opus: T0.4, T1.1–T1.3, T2.1, T2.2, T2.4, T3.2, T3.4. Sonnet: T0.0, T0.2, T0.3, T0.5, T0.6, T0.8, T1.4–T1.7, T2.3, T2.5–T2.7, T3.1. Haiku: T0.1, T0.7, T3.3 (+ fmt sweeps / changelog between waves).

**Scale, honestly:** 34 tasks ≈ 34–50 agent invocations with retries; Rust clean builds are ~4–7 min on this box and each task builds several times. Expect **many hours of wall clock** (likely 8–16), run unattended.

---

## 5. Autonomy policy

1. Two attempts per task at its tier; on the second failure escalate one tier (H→S→O) with transcripts, one attempt. Failure at Opus → **halt that branch only**, write `docs/BLOCKED.md` (task, transcripts, last 200 lines, failing assertion, hypothesis); other branches continue.
2. **Never weaken an acceptance test.** Criteria change only by a Boss commit to this file, logged in `docs/HANDOFF.md`.
3. Wall-clock per attempt: Haiku 30 min, Sonnet 90 min, Opus 180 min.
4. Wave gate: every task PASS or BLOCKED; Boss re-scopes or records to `docs/RESIDUAL.md`. Boss never pauses for the owner.
5. Whole-run halt only if `main`'s baseline goes red and can't be restored in one attempt, or ≥ 3 tasks in one wave are BLOCKED.
6. Git: branch `phase-<N>/<task-id>` per task in its own worktree; agents never touch `main` and **never push**; Boss squash-merges (`<task-id>: summary` + passed assertions) and pushes at wave boundaries; no force-push, no `--no-verify`.
7. Every agent shell begins: `$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"; $env:RUST_BACKTRACE="1"; $env:FAMILY_HUB_DATA_DIR="$env:TEMP\familyhub-test"`. `~/.cargo/dx06` (dx 0.6.3) is not used.
8. **Version pins** (normative table with traps: `PURPLE_TEAM.md` §P5.4): `dioxus =0.7.10`, `dx 0.7.10`, `axum =0.8.9`, `sqlx =0.8.6`, `rustls =0.23.43` (`ring`), `tokio-rustls =0.26.4`, `rcgen =0.14.10`, `mdns-sd =0.21.0`, `fast_qr =0.13.1`, `argon2 =0.6.0`, `rrule =0.14.0`, `windows-service =0.8.1`, `uuid 1`, `image 0.25`, dev `tokio-tungstenite 0.28`, Tailwind standalone 3.4.17.
9. **Stated defaults** (36, normative: `PURPLE_TEAM.md` §P5.5): ports 8080/8443; TV origin HTTP; 6-digit PIN; leaf 397 d; one board, 2,000 strokes; backups 14; photo retention 30 d; upload route 25 MiB; staleness 90 s; week starts Sunday; server-local time everywhere; no Google service account assumed (fixtures); screensaver schedule off; log `info`, 10 MB×5.

---

## 6. Assumptions
- A1 Server runs on this Windows 11 PC (14 cores, 535 GB free); Pi later is a Phase 4 move.
- A2 **Confirmed by owner 2026-08-29:** the display is an **Insignia NS-50F301NA22 — a 50" Fire TV Edition television** (not a stick) on **Fire OS 7.7.1.5 = Android 9 / API 28**, ADB debugging ON, IP `10.0.0.178` (`docs/device.toml`). → **Branch A (Fire OS + Fully Kiosk)** is promoted; Vega is off the table for this device. Being a TV rather than a stick: the "disable HDMI-CEC on the television" step becomes "disable the TV's own sleep/power-saver timers"; Fully Kiosk can be sideloaded via adb directly. All three branches are still documented in case the display is ever replaced.
- A3 **Confirmed by owner 2026-08-29: only the two parents (mom and dad) have phones; the four boys do not.** The TV path must be fully self-sufficient for a child's routine (D1/D8 scope line); the phone PWA is a parents' controller/admin surface. Phone OS mix still unknown (Android Chrome and/or iOS Safari).
- A4 Google Calendar via service account stays optional; no credentials exist in the run.
- A5 Owner will do a DHCP reservation for the PC (A2 in the checklist).

## 7. Residual risks (after all changes — full table `PURPLE_TEAM.md` §P6)
RR-1 hydration in a real browser is only proven by the Boss screenshot step in T3.3 and by A5; RR-2 Fully Kiosk boot-launch on Fire OS is vendor-unreliable (B′ box is the priced fallback); RR-3 Vega/Silk has no boot auto-launch; RR-4 iOS may refuse the private root (P4.2 is the escape hatch); RR-5 `rrule` DST beyond the two tested boundaries; RR-6 iOS offline replay is on next open; RR-7 `mdns-sd` on multi-homed Windows (raw-IP QR mitigates); RR-8 synthetic vs real pointer load; RR-9 the PC is a single point of failure with no alerting beyond `/health` and the TV badge; RR-10 four declared non-Rust exceptions; **RR-11 T0.4 is the single serial choke point**; RR-12 design remains a Phase 4 item.

---

## Appendix A — Owner Verification Checklist (after the run; delivered as `docs/OWNER_CHECKLIST.md`)
A1 read the TV's model/OS (Settings → My Fire TV → About) and confirm/choose the branch · A2 DHCP reservation for the PC · A3 run `family-hub.exe install` from an elevated prompt, reboot the PC, `http://<ip>:8080/health` answers with nobody logged in · A4 Branch A: sideload Fully Kiosk ≥ 1.61.2 + PLUS, adb permission grants, `sleep_timeout 0`, screensaver Never, HDMI-CEC off — three consecutive TV reboots land on the kiosk; A4′ Branch B: Silk bookmark · A5 navigate the whole TV UI with the real remote (`?keys=1` to report codes) — a child completes a routine · A6 install the CA on each phone from `/ca.crt` (iOS: enable full trust) — padlock on `https://<ip>:8443/m` · A7 install the PWA on Android + iOS · A8 airplane-mode toggle replays dated correctly · A9 real 12 MP photo < 3 s · A10 drop real screensaver photos · A11 pull the network 5 min → badge → recovers < 30 s · A12 (optional) enable DNS-01.

**Delivered numbering (T3.2, wave 3):** `docs/OWNER_CHECKLIST.md` renumbers these as steps 1–13 (1→A1, 2→A2, 3→A3, 4→new parent-PIN step, 5→A4/A4′, 6→A5 … 13→A12) and is the article the owner follows; the A-ids above are the planning names and are kept for traceability.
