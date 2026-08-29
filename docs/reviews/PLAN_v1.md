# Sheffield Family Calendar & Routine Hub — Bridge-the-Gap Plan

**Status:** DRAFT v1 — awaiting red/purple/white team review, then owner approval
**Author:** Fable 5 (boss/orchestrator)
**Date:** 2026-08-29
**Repo:** https://github.com/lonzosheffield/Sheffield-Family-Calendar (cloned to `C:\Family Calendar`)

---

## 0. The objective (owner's words, distilled)

> Build the Sheffield Family Calendar & Routine Hub **in Rust, whole stack**. It runs
> **local-first on the home network** (no cloud dependency). The primary display is a
> **Fire TV** (Fire OS) in **kiosk mode**. Phones use a **companion PWA** off the same
> server. Everything the README promises — routine tracker, calendar, whiteboard,
> photo tasks, screensaver — must actually work. Design inspiration images to follow.

### Non-negotiables
1. **Rust is the stack.** Server, client, shared types, tooling glue. Third-party *viewers* (a browser on the TV, a browser on a phone) are not "stack"; any JS is limited to the unavoidable few lines a browser demands (service-worker bootstrap) and is generated/served by Rust.
2. **Local-first.** Works with the internet unplugged. Google Calendar is an *optional* upstream, never a dependency.
3. **Fire TV is the kiosk.** Not a Fire *tablet* — the README is wrong about this. A Fire TV has **no touchscreen**; input is a D-pad remote. This single fact reshapes the UI.
4. **PWA companion** on phones (Android + iOS) must be installable and useful.
5. **Fully autonomous execution** after plan approval — no checkpoints.

---

## 1. Where the code is today (assessment from a full read of all 2,041 lines)

### What exists and looks sound
| Area | State |
| --- | --- |
| Dioxus 0.6.3 fullstack skeleton (`router` + `fullstack`), feature-gated `web` / `server` | Clean, idiomatic |
| SQLite schema (routine templates, daily logs, custom tasks) + seeds + 8 integration tests | Solid; tests are real |
| `#[server]` fns for routine/tasks/calendar/screensaver | Reasonable |
| WebSocket broadcast bus (`/ws`, `tokio::sync::broadcast`) + client pump | Works in the happy path |
| Whiteboard canvas w/ DPR scaling, normalized coords, palette | Good client code |
| Google Calendar service-account poller (JWT → token → events) | Correct shape |
| Tailwind + Sheffield palette | Fine |

### Gaps, bugs, and wrong assumptions (ranked by impact on the objective)

| # | Finding | Why it matters | Sev |
| --- | --- | --- | --- |
| G1 | **Kiosk UI is touch-first; target is a Fire TV with a D-pad remote.** Buttons ("Maximize", profile circles, routine rows, pen palette) have no focus management; whiteboard drawing needs a pointer. | The headline device can't drive the app. | **Critical** |
| G2 | **No kiosk deployment story at all.** Nothing about how the TV opens the app, stays awake, survives reboot, or hides the browser chrome. | The product doesn't exist on the TV without it. | **Critical** |
| G3 | **WebSocket client never reconnects.** If the server restarts or Wi-Fi blips, the kiosk silently stops updating forever. | 24/7 kiosk goes stale. | **Critical** |
| G4 | **No midnight rollover.** Routine `use_resource` keys on `routine_version`/`user_id` only; at 00:00 the kiosk still shows yesterday's checkmarks until someone toggles something. | Morning routine is wrong every morning. | High |
| G5 | **Whiteboard has no persistence/replay.** Strokes are broadcast, never stored. Any late joiner (or the TV after a reload) sees a blank board. | Whiteboard is unusable as a "family board". | High |
| G6 | **PWA is a stub.** `manifest.json` has `icons: []`, there is no service worker, no offline shell. Chrome/Android will not offer install. `start_url` is `/mobile`, which only shows the routine. | "Companion PWA" doesn't exist yet. | High |
| G7 | **No HTTPS / no LAN naming.** PWAs (service worker, install prompt, camera `capture`) require a secure context. Phones would have to type an IP. | PWA can't install; every phone needs the raw IP. | High |
| G8 | **Screensaver images likely 404.** `list_screensaver_images` returns `/assets/screensaver/x.jpg`, but Dioxus's static handler only serves hashed `asset!()` outputs; only `/uploads` has a `ServeDir`. | Feature is dead on arrival. | High |
| G9 | **Mobile route is routine-only.** No calendar, no whiteboard, no way to control the TV from a phone. | Phone as the *input device* for a touchless TV is the natural fix for G1 — but the route doesn't support it. | High |
| G10 | **Calendar is "today, Google-only, read-only".** No local events, no week view, no way to add an event without Google. Poll interval 15 min; first poll happens immediately (OK). | Not local-first; a family calendar needs more than today. | High |
| G11 | **Profiles are hardcoded `"Boy 1..4"`** with a DB `CHECK (user_id BETWEEN 1 AND 4)`. | Can't name the kids; can't add a 5th person/parent. | Med |
| G12 | **Custom tasks never expire and aren't dated.** A "photo task" from last month sits in "Extra tasks" forever. | Clutter; conflicts with the daily-routine mental model. | Med |
| G13 | **Server rebroadcasts *any* valid `WsMessage` verbatim**, including `CalendarUpdated` and `RoutineUpdated`. A client can spoof server-originated events. | LAN-only, but it's a footgun; also blocks adding privileged messages (e.g. `SetView`). | Med |
| G14 | **Photo upload rides a server-fn as base64.** Large phone photos (5–12 MB) → ~16 MB JSON body → likely body-limit / memory pain; no client-side resize; extension is always `.jpg` regardless of type. | Photo tasks fail on modern phones. | Med |
| G15 | **Dioxus 0.6.3 is end-of-life; 0.7.x is current.** Small codebase (2k LOC) makes the migration cheap now and expensive later. | Ecosystem drift, unfixed bugs. | Med |
| G16 | **README toolchain is Linux-only**; dev box is Windows 11. No CI. No release/packaging. | Reproducibility. | Med |
| G17 | **No auth of any kind.** Acceptable on a trusted LAN for kids' routines, but parent-only actions (renaming profiles, editing templates, deleting) need at least a PIN. | Kids will "administer" the hub. | Low–Med |
| G18 | `serve_dioxus_application` + `dioxus_cli_config::fullstack_address_or_localhost()` binds `127.0.0.1` unless `dx serve --addr 0.0.0.0` / env is set; the production binary needs its own bind config. | Phones can't reach it. | Med |
| G19 | Design: no inspiration images received yet; current UI is generic Tailwind cards. | Owner wants a specific look. | Low (blocked on input) |

---

## 2. Architecture decisions

### D1 — Kiosk input model: **the TV is a display; phones are the controllers**
The Fire TV remote gets a minimal, robust D-pad experience (cycle panels, toggle a routine item, dismiss screensaver). Everything richer — drawing, photo tasks, adding calendar events, admin — happens on the phone PWA. A phone can also **remote-control what the TV shows** (`SetView`, `SetActiveProfile`) over the existing WebSocket bus. This turns G1 from "rewrite the UI for D-pad" into "make the TV view keyboard-navigable + make the phone the pen".

*Alternative rejected:* Bluetooth mouse/air-mouse on the TV. Works, but a family won't keep one charged; the phone path is needed anyway.

### D2 — Kiosk runtime: **Fully Kiosk Browser (sideloaded) → optional Rust-native Android shell later**
Phase A uses Fully Kiosk Browser on Fire TV: full-screen, keep-awake, launch-on-boot (via its own setting or the "Launch on Boot" helper), auto-reload on error, remote admin. It is a *viewer*, not stack, and it's the fastest path to "it's on the TV".
Phase C (stretch) replaces it with a **Dioxus `mobile`/Android build** of the same crate — a Rust-owned APK, Leanback launcher entry, `FLAG_KEEP_SCREEN_ON`, immersive mode — sideloaded via `adb`. Everything in Phase A is still needed (D-pad UX, TLS, reconnection), so nothing is thrown away.

*Fire OS notes to verify in review:* Fire OS 7/8 = Android 9/11; Amazon WebView (Chromium-based); ADB sideload via "Apps from unknown sources"; system screensaver must be set to *Never*; no true single-app lock without MDM.

### D3 — Networking on the LAN: **stable name + Rust-issued TLS**
- **Naming:** the server advertises `familyhub.local` over mDNS (`mdns-sd` crate, pure Rust) *and* the kiosk screen shows a **QR code** (`qrcode` crate → SVG) with the URL so a phone can join in one scan. Owner should also set a DHCP reservation for the server machine as belt-and-braces.
- **TLS:** the server generates a **local CA + leaf cert** on first run (`rcgen`), serves HTTPS with `rustls` (`axum-server` or `hyper-rustls`), and keeps plain HTTP on a second port that **redirects** to HTTPS *except* for `/ca.crt` (one-tap install of the root cert on each phone/TV). This is the only way a LAN PWA gets a secure context without exposing the house to the internet. Windows-side, the same CA is trusted once via `certutil`.
- Bind `0.0.0.0` explicitly; port configurable via `FAMILY_HUB_ADDR`.

### D4 — Local-first data: **SQLite is the source of truth; Google is an optional feed**
- New `events` table (local calendar events; recurring via RRULE string; `source = local|google`).
- Google poller **upserts** into `events` (so events survive offline); a `google_sync_state` row tracks the sync token. Phones/TV read only from SQLite.
- Calendar UI: **Today** (kiosk) + **Week** (kiosk & phone) + add/edit on phone.
- Whiteboard strokes persisted in a `whiteboard_strokes` table (append-only, `board_id`, cleared by a `cleared_at` watermark); joiners get a `Snapshot` on connect.
- Custom tasks get `due_date` (default today) and auto-hide after their day.
- Profiles table replaces the hardcoded array: `id, name, color, avatar, is_parent, sort_order`. Seed with the four boys; editable from the phone (parent PIN).
- Migrations move to numbered SQL files under `migrations/` using `sqlx::migrate!` (embedded at compile time — still one binary).

### D5 — Realtime bus hardening
- Server-authoritative envelope: clients send `ClientMessage`, server broadcasts `ServerMessage`. Server never rebroadcasts client JSON verbatim (fixes G13).
- Client pump: exponential-backoff reconnect (1s → 30s cap), heartbeat ping every 20s, **resync on reconnect** (bump all versions + request whiteboard snapshot).
- Add `SetView`, `SetActiveProfile`, `WhiteboardSnapshot`, `Heartbeat`, `DayRolled` messages.
- Server runs a **midnight tick** (tokio task, `chrono::Local`) that broadcasts `DayRolled` → clients refetch (fixes G4).

### D6 — PWA
- Real `manifest.json` (icons 192/512 PNG + maskable, `start_url: /m`, `display: standalone`, `scope: /`).
- A ~40-line `sw.js` served by axum from a Rust `include_str!` — precaches the app shell (wasm, js glue, css, icons), network-first for server fns, cache-first for `/uploads` and screensaver images. Registered from Rust via `web_sys`. **This is the one JS file in the project and it is documented as such.** (Writing the SW body in wasm is possible but adds a JS bootstrap anyway; not worth it.)
- Offline behaviour: last-known routine/calendar rendered from cached responses; mutations while offline show a toast and are retried on reconnect (simple queue in `localStorage` via `gloo-storage`). Full CRDT sync is explicitly out of scope.

### D7 — Framework version: **migrate to Dioxus 0.7.x in Phase 0**
Do it before adding features, not after. Codebase is small; 0.6 is unsupported. Migration touches `asset!` options, `dioxus::launch` / `dioxus::serve`, server-fn crate 0.7, and the axum 0.8 bump. Pin exact versions (`dx` CLI via `cargo binstall dioxus-cli@0.7.x`).
*Fallback:* if migration blows the Phase 0 budget, ship Phase 0 on 0.6.3 and schedule 0.7 as its own phase. Reviewers: please pressure-test this.

### D8 — Fire TV D-pad UX (10-foot UI)
- Global key handler (`keydown` on `window`): Left/Right cycle the maximized panel, Up/Down move a focus cursor within the panel, Enter/Center activates, Back restores the grid. Focus ring is a thick `ring-sheffield-sun` outline.
- Fonts ≥ 28px body / 44px headings at 1080p; 5% overscan safe-area padding; high contrast; no hover-only affordances.
- Screensaver dismiss on any key; idle timeout configurable (default 10 min); server can `SetView(Screensaver)` on a schedule (e.g., 9 pm).
- The TV can show a **"Scan to join"** QR overlay (from D3) via the remote's Menu key.

### D9 — Run the server on this Windows PC first, Raspberry Pi/mini-PC later
Single release binary + `assets/` + `family.db`. Ship a **Windows Service** wrapper (`windows-service` crate, pure Rust) so it starts at boot. Cross-compile target `aarch64-unknown-linux-gnu` kept building in CI so a Pi is a drop-in later. Docker is *not* used (breaks mDNS and adds a non-Rust layer).

---

## 3. Workstreams and phases

Each task lists: owner agent (model), inputs, outputs, acceptance test. "Boss" = Fable 5 orchestrator who reviews every merge.

### Phase 0 — Green build on Windows + 0.7 migration  (serial; blocks everything)
| ID | Task | Agent | Accept |
| --- | --- | --- | --- |
| 0.1 | Toolchain: rustup stable, wasm32 target, `dx` (pinned), Tailwind 3.4.17, `adb`. Document in `docs/DEV_WINDOWS.md`. | Boss (in progress) | `cargo --version`, `dx --version`, `tailwindcss --help`, `adb version` all succeed |
| 0.2 | `cargo test --features server` and both clippy targets pass **as-is** on 0.6.3. Record baseline. | Sonnet | Green, output pasted into `docs/BASELINE.md` |
| 0.3 | Migrate to Dioxus 0.7.x (+ axum 0.8, server-fn 0.7). Keep behaviour identical. | Opus | Same tests green; `dx serve --platform web` renders `/` and `/mobile` |
| 0.4 | Fix G18 (bind `0.0.0.0`, `FAMILY_HUB_ADDR`) and G8 (serve `assets/screensaver` via `ServeDir`). | Sonnet | `curl http://<lan-ip>:8080/` from another device works; screensaver image URL returns 200 |
| 0.5 | GitHub Actions CI: fmt, clippy (server + wasm), test, release build (windows-x64 + aarch64-linux). | Sonnet | CI green on `main` |

### Phase 1 — Foundations that everything else needs  (parallel after Phase 0)
| ID | Task | Agent | Accept |
| --- | --- | --- | --- |
| 1.1 | Realtime hardening (D5): message envelope split, reconnect/backoff/heartbeat, midnight `DayRolled`. | Opus | Kill server, restart → kiosk recovers < 30 s without reload; unit test for backoff schedule; integration test for envelope validation |
| 1.2 | Migrations → `sqlx::migrate!` + new tables: `profiles`, `events`, `whiteboard_strokes`, `custom_tasks.due_date`, `google_sync_state`. Data-migrate the seeded routine. | Opus | Tests green; migrating an existing `family.db` from v1 keeps routine logs |
| 1.3 | TLS + mDNS + QR (D3): `rcgen` CA/leaf on first run, `rustls` listener, HTTP→HTTPS redirect, `/ca.crt`, `mdns-sd` advertise `familyhub.local`, `qrcode` SVG component. | Opus | Phone on LAN: scan QR → install CA → `https://familyhub.local` green padlock; `wss://` works |
| 1.4 | Profiles: table, server fns, phone UI to rename/recolor; parent PIN (argon2 hash in `settings`). Remove `CHECK 1..4`. | Sonnet | Rename "Boy 1" → real name on phone; TV updates live |

### Phase 2 — The two surfaces  (parallel after Phase 1)
| ID | Task | Agent | Accept |
| --- | --- | --- | --- |
| 2.1 | **Kiosk / 10-foot UI** (D8): D-pad navigation, focus system, overscan-safe layout, big type, QR overlay, TV honours `SetView`/`SetActiveProfile`. | Opus | Drive the whole TV view with arrow keys + Enter + Esc on desktop; screenshot at 1920×1080 reviewed by Boss |
| 2.2 | **Phone PWA** (D6): manifest + icons, `sw.js` served from Rust, install prompt, bottom-tab layout `/m` → Routine / Calendar / Board / TV-remote / Settings. Offline shell + mutation retry queue. | Opus | Lighthouse PWA installable; airplane-mode shows cached routine; toggle while offline replays on reconnect |
| 2.3 | **Whiteboard v2**: persistence + snapshot replay, phone as pen (touch), TV as display, undo-last-stroke, multiple named boards (optional). | Sonnet | Draw on phone → appears on TV; reload TV → drawing still there; Clear works everywhere |
| 2.4 | **Calendar v2** (D4): local events CRUD on phone, Today + Week views, Google poller upserts to `events`, optional ICS import (`icalendar` crate). | Opus | Add "Dentist 3pm" on phone offline-from-Google → shows on TV; Google event appears after poll; week view correct across DST |
| 2.5 | **Photo tasks v2** (G12/G14): client-side downscale (canvas → JPEG ≤ 1600px), multipart upload route instead of base64 server fn, `due_date`, daily auto-hide, correct extension by MIME. | Sonnet | 12 MP phone photo uploads < 3 s; stored file ≤ 400 KB; yesterday's task hidden today |

### Phase 3 — Ship it to the living room  (serial-ish)
| ID | Task | Agent | Accept |
| --- | --- | --- | --- |
| 3.1 | Windows Service wrapper (`windows-service` crate) + `install.ps1`/`uninstall.ps1`; logs to `%ProgramData%\FamilyHub\logs`. | Sonnet | Reboot PC → hub is reachable without login |
| 3.2 | Fire TV runbook `docs/FIRE_TV.md`: enable ADB/unknown sources, sideload Fully Kiosk (+ Launch-on-Boot), install CA, set URL, keep-awake, disable system screensaver, remote-admin password, recovery steps. Scriptable bits via `adb` in `scripts/firetv.ps1`. | Sonnet (Boss verifies against the real device's constraints) | A fresh Fire TV reaches the kiosk after reboot with no remote interaction |
| 3.3 | End-to-end verification pass: Chrome on desktop pretending to be TV (keyboard) + phone (DevTools device mode) + real phone on LAN. Record GIF. | Boss + Sonnet | All acceptance tests above re-run and pasted into `docs/VERIFICATION.md` |
| 3.4 | Design pass once inspiration images arrive (G19): typography, color usage, panel composition; keep palette. | Opus (frontend-design skill) | Owner sign-off |

### Phase C (stretch, only after Phase 3 is proven) — Rust-native Fire TV shell
| ID | Task | Agent | Accept |
| --- | --- | --- | --- |
| C.1 | `dx build --platform android` of the same crate as a Leanback app pointing at the LAN server; keep-screen-on, immersive, auto-start receiver; `adb install`. | Opus | TV boots straight into the Rust APK; Fully Kiosk uninstalled |

---

## 4. Agent roster & orchestration rules

- **Boss (Fable 5):** owns the plan, assigns work, reviews every diff against acceptance criteria, runs the final verification, holds the Rust-only line.
- **Opus** for anything touching architecture, async/WS, TLS, migration, 10-foot UX, PWA.
- **Sonnet** for well-specified feature work, CI, scripts, docs, tests.
- **Haiku** for mechanical tasks only: formatting sweeps, doc link checks, asset generation scripts, changelog assembly.
- Every task runs in an isolated worktree, must leave `cargo fmt --check`, both `clippy` targets, and `cargo test --features server` green, and must **add tests** for new server logic.
- Merge order follows phase dependencies; within a phase, parallel.
- Any deviation from Rust (beyond the documented `sw.js`) requires Boss sign-off and a line in `docs/NON_RUST.md`.
- No task is "done" without its acceptance test output captured.

---

## 5. Assumptions (flag if wrong)

- A1. The server will run on this Windows 11 PC to start (14 cores, plenty of disk); a Pi may replace it later.
- A2. The Fire TV is a stick/cube on Fire OS 7 or 8 with ADB debugging available. It is on the same Wi-Fi/VLAN as the phones (no client isolation).
- A3. Phones are a mix of Android (Chrome) and possibly iOS (Safari). iOS PWA install requires the CA profile to be installed *and* trusted (Settings → About → Certificate Trust Settings) — a documented manual step.
- A4. Four kids + parents; parents administer from their phones with a PIN. No per-user login.
- A5. Google Calendar via service account is kept as-is (the calendar must be shared with the service-account email). Consider OAuth device flow later if the owner wants their *personal* calendar without sharing it.
- A6. "Local-first" means *no cloud dependency*, not multi-master offline editing. The server is the single writer.

## 6. Out of scope (for now)
Multi-home sync, voice (Alexa) integration, cloud backup (a nightly `family.db` copy to a NAS folder is a one-liner and can be added), user accounts, native iOS app.

## 7. Risks the reviewers should hammer on
- R1. Dioxus 0.7 migration cost vs. benefit (D7).
- R2. TLS-on-LAN UX: will family members actually install a CA? Is there a simpler path that still yields a secure context (e.g., Tailscale is non-Rust and non-local; skip)?
- R3. Fire OS behaviours: launch-on-boot reliability, Amazon WebView feature gaps (Pointer Events, canvas, WebSocket over self-signed TLS with a user-installed CA).
- R4. D-pad UX complexity — is "phone as controller" enough, or does the TV need richer input?
- R5. Photo upload path via multipart in Dioxus 0.7 fullstack (custom axum route alongside `serve_dioxus_application`).
- R6. mDNS on Windows (Bonjour absent) and on Android (`.local` resolution is flaky on some Android versions) — QR with raw IP fallback is the safety net.
- R7. Body-size limits in Dioxus server fns; where the limit lives after 0.7.
- R8. Windows Service + `dx`-built assets: locating `public/` at runtime when CWD is `C:\Windows\System32`.
