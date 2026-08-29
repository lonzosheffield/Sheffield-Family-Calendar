# PURPLE TEAM — Resolution & Change-List for PLAN v2

**Author:** Purple Team (defence / synthesis — resolves Red and White into decisions)
**Date:** 2026-08-29
**Inputs:** `docs/PLAN.md` v1 · `docs/reviews/RED_TEAM.md` (R-01…R-31) · `docs/reviews/WHITE_TEAM.md` (A–I, W1–W11, H-1…H-26) · `docs/BASELINE.md` (green: 8/8 tests, clippy clean both targets)
**Purpose:** every accepted finding maps to a concrete task, default, or pin below. Fable applies P2–P5 wholesale to produce PLAN v2.

**Verdict:** the plan is salvageable with **one architectural change** and **one honesty change**.

- *Architectural:* **the TV never uses TLS.** That single decision dissolves R-02, W-F21, the D2/D3 incompatibility, half of R-03, and makes the kiosk shell a swappable, zero-code-impact choice — which in turn dissolves R-01 (Vega OS) as a *blocker* and demotes it to a runbook branch.
- *Honesty:* **"autonomous" means code + server-side validation autonomy, not physical-device validation.** Every task below has an acceptance test an agent runs on this Windows PC with no second device and no human. Everything requiring the TV, a phone, or a reboot moves to **Appendix A — Owner Verification Checklist**, explicitly outside the run.

With those two, Phase C is cut, no task has a human in its acceptance path, and the run is one approval end-to-end.

---

## P1. Disposition table

### Red Team findings

| ID | Disposition | Resolution |
| --- | --- | --- |
| **R-01** Vega OS / device unidentified | **Accept-modified** | Reject the "confirm before approval" gate — it is a human checkpoint. Accept the substance: the TV surface becomes a **plain HTTP URL in any browser** (P2a), so no code depends on the device. **T0.0** probes the device non-blockingly and writes all three runbook branches; **T3.2** publishes them. Phase C cut (R-29). Device answer is required only in Appendix A. |
| **R-02** CA cannot be installed on Fire TV | **Accept** | Resolved by not needing it: TV is HTTP-only. Local CA is installed on **phones only** (2–4 devices, parents). → P2a, **T1.3**. |
| **R-03** Fully Kiosk vendor-disclaims Fire OS; paid; ADB grants; sleep timer; HDMI-CEC | **Accept-modified** | Reject "move the spike into Phase 0" — an agent cannot run it. Accept everything else: Fully Kiosk is **one of three runbook branches**, declared in `docs/NON_RUST.md` with its €8.90/$10.99 cost, its ADB permission grants, `sleep_timeout 0`, screensaver→Never and HDMI-CEC as mandatory runbook steps, plus a named fallback. → **T0.1**, **T0.0**, **T3.2**, Appendix A. |
| **R-04** nothing has been compiled | **Closed** | `docs/BASELINE.md` is green. Only the PATH-prefix survives → **P5** (every agent shell prepends `~/.cargo/bin`). §1 of PLAN v2 may keep its claims; the line count is corrected to 2,029 (W11). |
| **R-05** half the acceptance gates need a human | **Accept** | Every task in **P3** has an agent-runnable acceptance test. Device/phone/reboot gates → **Appendix A**. Non-negotiable 5 is restated as *code and server-side validation autonomy*. |
| **R-06** `RecvError::Lagged` closes the socket; unthrottled `pointermove` | **Accept in full** | Full protocol spec in **P2c**, implemented in **T1.2**, load-tested in **T1.2** acceptance. |
| **R-07** D7 mis-describes the migration | **Accept** | D7 rewritten in **P2b**; implemented in **T0.4**. |
| **R-08** axum `DefaultBodyLimit` = 2 MB; photo upload already broken | **Accept** | Re-graded **High**. → **T2.5**: explicit `DefaultBodyLimit::max(25 MiB)` on the upload route only, client downscale first, 12 MP fixture committed by **T0.7**. |
| **R-09** `.local` won't resolve on the TV; Windows is fine; SANs must cover the IP; one `ServiceDaemon` | **Accept** | Kiosk URL is the **reserved IP**, `.local` is a convenience. QR payload is the raw-IP HTTPS phone URL. Leaf SANs cover both. Single `ServiceDaemon`, FQDN with trailing dot, explicit interface bind, UDP/5353 firewall rule. → **T1.3**, **T3.1**. DHCP reservation → Appendix A (owner). |
| **R-10** 825-day cap; rcgen year-4096 default; no renewal; DNS-01 never considered | **Accept** | Leaf **397 days** (inside every cap), explicit `not_before`/`not_after`, auto re-issue at 30 days remaining, hot reload, expiry surfaced on `/health` and the TV. DNS-01 evaluated in **P2a** and **shipped** as a config-selectable `CertSource` (**T1.8**), default off. CA key ACL'd and excluded from backup. → **T1.3**, **T1.7**, **T1.8**. |
| **R-11** 2.1's acceptance is a desktop keyboard, not a remote; no Esc; Menu absent | **Accept** | **T2.1** acceptance becomes an **enumerable focus-order assertion** plus an inbound-`SetView` integration test. Esc dropped from the design; Back/Backspace + an on-screen Back target. A **key-code debug overlay** ships so the owner can report real codes. Remote navigation → Appendix A. |
| **R-12** do the kids have phones? TV must be self-sufficient | **Accept** | Product line written into PLAN v2: **a child completes their entire routine on the TV with the remote alone.** Whiteboard drawing, photo tasks, calendar editing and admin are phone-only, stated as scope, not an accident. → **T2.1**. |
| **R-13** parallel migration authorship; checksum bricking; stale proc-macro | **Accept** | `migrations/` has **exactly one owner (T1.1)**. Every later migration number is assigned **in the plan** (P4). Standing rule: never edit an applied migration. `build.rs` with `cargo:rerun-if-changed=migrations`. Restore-and-re-migrate drill in T1.1's acceptance. |
| **R-14** service CWD, runtime placement, invisible logs, firewall, sleep, rotation | **Accept** | Split: path/config discipline lands early in **T0.5** (so nothing else hard-codes a relative path); service mechanics in **T3.1**. |
| **R-15** offline queue lacks idempotency/date; iOS has no Background Sync; Lighthouse gone | **Accept** | Explicit date + client idempotency key on every mutation (**T1.5** server side, **T2.2** client side). Per-platform offline promise written down. Lighthouse gate replaced with four HTTP/SW assertions (**T2.2**). |
| **R-16** manifest served from a hashed `/assets/` path → scope invalid | **Accept** | `/manifest.webmanifest` and `/sw.js` served from **root** by explicit axum routes, `scope: "/"`. → **T0.6** (routes), **T2.2** (content). |
| **R-17** backup is not a one-liner and is out of scope | **Accept** | **T1.6** in Phase 1: `VACUUM INTO` nightly, uploads snapshot, retention, and a **restore drill inside the acceptance test**. |
| **R-18** nothing is ever deleted; unbounded growth | **Accept** | Delete paths for custom tasks (+ their photo file), stroke compaction, photo retention, log rotation, disk usage on `/health`. One DB row **per stroke**, not per segment. → **T1.6**, **T2.5**, **T2.3**, **T1.7**. |
| **R-19** `syncToken` conflicts with `timeMin/timeMax/orderBy`; upsert never deletes; no RRULE library | **Accept-modified** | **Decision: windowed polling, no sync token, full replace of the window per poll.** Simplest and correct at family scale; `status:"cancelled"` is irrelevant because the window is replaced. `rrule =0.14.0` for **local** events only, always `all(limit)`. **ICS import cut from v1** (R-25). → **T2.4**. |
| **R-20** WAL/journal mode never set; no busy_timeout; upgrade deadlocks | **Accept** | `journal_mode=WAL`, `synchronous=NORMAL`, `busy_timeout=30s`, **two pools** (read max 5, write max 1), periodic `wal_checkpoint(TRUNCATE)`. → **T1.1**. |
| **R-21** the suite is DB-only; migration has no real gate | **Accept** | **T0.3** lands an HTTP/WS integration harness **on 0.6.3, before the migration**, and it is the migration's gate. |
| **R-22** single-slot `inbound_stroke` drops segments; no `ResizeObserver` | **Accept** | Queue drained per render (or drawn directly from the WS handler) + `ResizeObserver` that re-syncs DPR and **repaints from the stroke log**. → **T2.3**. |
| **R-23** no ownership checks; ungated `SetView`; MIME→stored XSS; 4-digit PIN | **Accept** | Ownership checks on every mutating fn; `SetView`/`SetActiveProfile` require a parent session **or** are scoped to the sender's own profile; server-side extension allowlist + **server-side re-encode** to JPEG/PNG/WebP + `Content-Disposition`/`X-Content-Type-Options` on `/uploads`; **6-digit PIN**, server-enforced, argon2id, exponential backoff (no lockout). → **T1.4**, **T1.5**, **T2.5**. |
| **R-24** `today().unwrap_or_default()`; write-time clock read | **Accept** | Date is a **parameter** on every mutation, validated server-side within ±1 day. `unwrap_or_default()` replaced by an explicit error state — the kiosk never renders "nothing done today" on a fetch failure. Midnight-boundary test. → **T1.5**, **T1.2**. |
| **R-25** no budget; ICS / multiple boards / aarch64 CI / Phase C are inflation | **Accept** | **Cut:** ICS import, multiple named boards, the aarch64 CI job, Phase C. Budget expressed as attempts + wall-clock stop-loss per task → **P5**. |
| **R-26** `sqlx` `tls-native-tls` pulls OpenSSL | **Accept** | Dropped in **T0.2**. sqlx stays pinned at `=0.8.6` (do **not** chase 0.9.0 mid-run — new repo org, no benefit here). |
| **R-27** Rust-purity accounting is inaccurate; Tailwind pin inconsistent | **Accept** | `docs/NON_RUST.md` pre-filled with all nine rows in **T0.1**. Tailwind pinned to the **standalone `tailwindcss-windows-x64` v3.4.17 binary (no Node)**, and the pin is *justified*: v4 changes the config model entirely and there is no Rust equivalent; unlike Dioxus 0.6.3 it is a build-time asset compiler with no security or API surface in the shipped binary. |
| **R-28** in-memory event cache; `is_empty()` fallback; DST `.single()` | **Accept** | Events persisted to SQLite; explicit `Loading / Empty / Error` states replace `is_empty()`; DST ambiguity resolved deterministically to the **earliest** offset with a unit test. → **T1.1**, **T2.4**. |
| **R-29** Phase C is far more than a recompile | **Accept** | **Phase C is cut**, not deferred. Recorded in PLAN v2 §6 as *explicitly not planned*, with R-29's reasons. |
| **R-30** nothing tells anyone the hub is broken | **Accept** | **T1.7**: `/health` (DB, last poll, cert expiry, disk free, WS client count, uptime) + a permanent "updated HH:MM" and a disconnected badge on the TV. |
| **R-31** screensaver directory is empty; 0.4 cannot pass | **Accept** | **T0.7** commits 3 CC0 placeholder JPEGs + a 12 MP test fixture; **T2.7** adds the phone upload path. |

### White Team blocking (I-1…I-7) and recommended (I-8…I-12)

| ID | Disposition | Resolution |
| --- | --- | --- |
| **I-1** add task 0.0, halt/escalate on Vega | **Accept-modified** | Task **T0.0** added; **it never halts the run.** Escalation is a row in `docs/OWNER_CHECKLIST.md`, not a stop. Justified because nothing downstream depends on the answer once the TV is HTTP-only and Phase C is cut. |
| **I-2** resolve D2/D3 incompatibility; pick (a) DNS-01 or (b) TV-HTTP | **Accept** | **Primary = (b)** TV on HTTP, phones on HTTPS via local CA. **(a) DNS-01 shipped as an opt-in upgrade** (T1.8), default off. Full reasoning + scoring in **P2a**. |
| **I-3** correct D7's factual errors | **Accept** | D7 rewritten in **P2b**. |
| **I-4** remove human checkpoints; 3.4 out; 2.1 focus-order assertion | **Accept** | 3.4 becomes **T3.4 "palette-faithful polish"** with objective acceptance (contrast ratios, type scale, overscan) and **no owner sign-off**; the inspiration-image design pass moves to **Phase 4 (post-run)**. 2.1 gets the focus-order assertion. |
| **I-5** replace "Lighthouse PWA installable" | **Accept** | Four concrete assertions in **T2.2**. |
| **I-6** fix sequencing (1.4→1.2, 0.4/1.3 bind overlap, add build_router, phone→TV acceptance) | **Accept** | Full re-sequence in **P3/P4**. Bind logic is owned once by **T0.5/T0.6**; TLS layers on it in T1.3. `build_router()` is **T0.6**. Phone→TV loop is its own task **T2.6**. |
| **I-7** write the retry/escalation/halt policy | **Accept** | **P5.1**. |
| **I-8** 3.2 → Opus and gate on 0.0; 2.3 → Opus or a protocol from 1.1 | **Accept-modified** | **T3.2 → Opus** (it must reason across three device branches). **T2.3 stays Sonnet**, but the stroke/snapshot/echo protocol is written by Opus in **T1.2** and T2.3 only implements it — White's own second option, and it keeps Opus load down. |
| **I-9** add W1–W6, W10 to §1 | **Accept** | Added as G20–G26 in PLAN v2 §1 and each mapped to a task below. |
| **I-10** pre-fill NON_RUST.md; convert `install.ps1` to exe subcommands | **Accept** | **T0.1** and **T3.1** (`family-hub.exe install|uninstall|start|stop|status` via `windows-service::service_manager`). No PowerShell install scripts. `scripts/firetv.ps1` also dropped — it becomes `family-hub.exe tv-probe` shelling out to `adb`. |
| **I-11** specify crate pins and traps | **Accept** | **P5.4** pins table, with the traps named per crate. |
| **I-12** commit screensaver placeholders + generate PWA icons | **Accept** | **T0.7** (Haiku, `resvg`/`tiny-skia`). |

### White Team missed findings

| ID | Disposition | Resolution |
| --- | --- | --- |
| **W1** `toggle_custom_task` publishes no WS message | **Accept** | **T1.5** — publishes `TasksUpdated { user_id, date }`. |
| **W2** every stroke drawn twice on the originator | **Accept** | `ClientId` origin field; clients ignore their own echo. → **P2c**, **T1.2**. |
| **W3** calendar can never return to empty | **Accept** | Explicit `Loading/Empty/Error`; `is_empty()` fallback deleted. → **T2.4**. |
| **W4** Google cache never invalidated at midnight | **Accept** | Moot once events are in SQLite; additionally the midnight tick **forces a poll**. → **T2.4**. |
| **W5** task 1.4 breaks `tests/db_tests.rs:148-157` | **Accept** | **T1.4** is instructed explicitly: replace that test with a **foreign-key** violation test against `profiles`. Written into the task, not left to guess. |
| **W6** `ProfilesUpdated` missing from D5 | **Accept** | In the P2c message list; emitted by profile writes, consumed via `profiles_version`. |
| **W7** `RoutineUpdated { .. }` discards `user_id` | **Accept** | Client matches on `user_id` and refetches only the affected profile. → **T1.2**. |
| **W8** two `tower-http` versions in the lock file | **Accept-modified** | Informational; after T0.4 the duplicate should vanish. CI (**T0.8**) asserts `cargo tree -d` reports no duplicate `tower-http`/`axum`/`hyper`. |
| **W9** committed `tailwind.css` drifts; config globs a missing file | **Accept** | **T0.1** fixes the glob; **T0.8** CI rebuilds the CSS and **fails on any diff**. |
| **W10** missing `web-sys` features for SW registration | **Accept** | **T0.2** adds `Navigator`, `ServiceWorkerContainer`, `ServiceWorker`, `ResizeObserver`, `File`, `Blob`, `FormData`, `Performance`. |
| **W11** line count 2,041 vs 2,029 | **Accept** | Corrected in PLAN v2 §1. |

**Rejected outright:** nothing of substance. Three *recommendations* are rejected while their findings are accepted: R-01's pre-approval device gate, R-03's Phase-0 physical spike, and I-1's halt-on-Vega — all three are human checkpoints, and P2a removes the need for them.

---

## P2. The three hard decisions

### (a) Kiosk runtime + TLS — **DECIDED**

#### The reframe that resolves the conflict

D2 and D3 are incompatible only because v1 assumed the TV needs HTTPS. It does not.

The TV needs: a rendered page, a WebSocket, and a canvas. It does **not** need a service worker, an install prompt, `getUserMedia`, camera `capture`, geolocation, or any other secure-context API — all of those live on the phone. `ws://` on an `http://` origin is a same-origin, non-mixed-content upgrade and works everywhere.

So: **split the origins.**

```
:8080  HTTP   →  /tv   kiosk view      (TV)          + /ws, /assets, /uploads, /ca.crt, /health
                 /m*   308 → HTTPS      (phones that typed http)
:8443  HTTPS  →  /m    phone PWA        (phones)      + /wss, /manifest.webmanifest, /sw.js, everything
```

One binary, one router (`build_router()`), two listeners. The client picks `ws`/`wss` from `window.location.protocol`. The QR code encodes the **HTTPS phone URL with the reserved IP**.

Consequences: R-02 (CA on TV) is void. W-F21 (WebView user-CA rejection) is void. Fully Kiosk's "Ignore SSL Errors" trap is void. And because the TV surface is now *just a URL*, **the kiosk shell becomes a runbook choice with zero code impact** — which is what defuses R-01.

#### Option scoring (1 = poor, 5 = excellent)

| Option | Rust purity | Autonomy | 24/7 robustness | Kid/parent UX | Cost | **Σ** |
| --- | --- | --- | --- | --- | --- | --- |
| **P-A. TV plain HTTP + phones HTTPS via local CA** | 4 — kiosk shell is a viewer; PKI, mDNS, QR all Rust | **5** — every part buildable and testable on this PC, offline | **5** — no expiry can break the TV; cert failure degrades phones only | 4 — one-time CA install on 2–4 phones (parents), documented | 5 — £0 (+£9 Fully Kiosk if Fire OS) | **23** |
| **P-B. DNS-01 public cert on an owned domain → LAN IP** | 4 — `instant-acme` 0.8.5 is pure Rust, Tokio+rustls, 2.5 M downloads, actively maintained | **2** — agents cannot obtain or renew a cert without a domain + DNS API token + internet; untestable end-to-end in the run | 3 — renewal needs internet every ~60 d; a failed renewal silently kills *both* surfaces; public DNS publishes the internal IP | **5** — nothing to install on any device, ever | 3 — domain ~£10/yr + a DNS provider with an API | **17** |
| **P-C. Rust-native Android shell (Dioxus android) shipping its own CA** | 5 | **1** — Android SDK/NDK/JDK 17/CMake/Gradle (the largest non-Rust toolchain in the project), raw-XML Leanback manifest injection, D-pad focus written from scratch, **and impossible on Vega** | 2 — an unmaintained kiosk shell you now own | 3 | 2 | **13** |
| **P-D. v1 as written — CA on every device incl. the TV** | 4 | 1 | **1** — technically infeasible (R-02 + F21) | 1 | 4 | **11** |
| **P-E. Phones only, no TV** | 5 | 5 | 5 | **1** — the product does not exist | 5 | — |

#### Decision

> **PRIMARY: P-A.** TV on plain HTTP at `http://<reserved-ip>:8080/tv`. Phones on `https://<reserved-ip>:8443/m` with the `rcgen` local CA installed once per phone. Cert source is a trait; the self-signed CA is the default and needs no internet, no domain and no configuration.
>
> **FALLBACK / UPGRADE: P-B**, implemented as the second `CertSource` variant (**T1.8**) and shipped **off**. If the owner ever supplies a domain and a DNS-provider token, `certs.mode = "acme_dns01"` in `familyhub.toml` removes every CA install with no code change and no redeploy. This is also the escape hatch if an iOS device refuses the private root.
>
> **REJECTED: P-C** (R-29, W-F11, and Vega), **P-D** (infeasible), **P-E** (not a product).

```rust
// server/tls.rs — the seam that makes P-B a config change, not a rewrite
pub enum CertSource {
    /// Default. rcgen local CA + 397-day leaf, auto re-issued at 30 days remaining.
    SelfSignedCa,
    /// Opt-in. instant-acme 0.8.5, DNS-01 via a `DnsProvider` impl. Never the default.
    AcmeDns01 { domain: String, provider: DnsProvider, contact: String },
}
pub trait CertProvider { async fn current(&self) -> Result<CertifiedKey>; async fn renew_if_due(&self) -> Result<bool>; }
```

`AcmeDns01` is testable without a domain: the `DnsProvider` trait gets a `MockProvider`, and the renewal scheduler, the "due at 30 days" predicate and the hot-reload path are unit-tested against fixture certs. Only the live LE round-trip is owner-verified (Appendix A).

#### Kiosk shell — chosen per device branch, no code impact

| Branch | Detected | Shell | Auto-start on boot | Notes |
| --- | --- | --- | --- | --- |
| **A. Fire OS** (`ro.build.version.name` ∈ 5/6/7/8/14/16) | via adb | **Fully Kiosk Browser ≥ 1.61.2** (PLUS, €8.90/$10.99, one-off) | Yes, after `SYSTEM_ALERT_WINDOW` + `GET_USAGE_STATS` granted over adb; vendor calls it "slow" | Also required: `settings put secure sleep_timeout 0`, screensaver → Never, HDMI-CEC off on the TV, Graphics Acceleration → None if video stutters. Vendor-disclaimed → declared in `NON_RUST.md` with an exit criterion. |
| **B. Vega OS** (Fire TV Stick 4K Select 2025 / Stick HD 2026; no unknown-sources toggle, no adb install) | adb refuses / version starts `1.` | **Amazon Silk** (Chromium, present on Vega; "Alexa, open Silk"), bookmark to the kiosk URL | **No** — manual relaunch after a power cut | Fully acceptable because the TV is HTTP-only: no cert, no install, nothing to configure. Degraded on boot resilience only. |
| **B′. Vega + owner wants boot resilience** | owner choice | **~£35 Google TV / onn 4K box** or a wall-mounted retired Android tablet, running Fully Kiosk | Yes | Recorded as the priced upgrade, not a plan dependency. Amazon Signage Stick (~$99) is the vendor-blessed but expensive variant. |
| **C. Unknown** (TV off / not paired / no IP supplied) | adb cannot connect | All three branches published; owner picks | — | **The run does not stop.** |

Because the TV loads a URL over HTTP, **switching branches is a bookmark change.** That is the whole point.

#### Task 0.0 — device-identification gate (non-blocking)

**Decision rule, executed by the agent:**

1. TV IP from, in order: `$env:FAMILY_HUB_TV_IP` → `docs/device.toml` (`tv_ip = "..."`) → an already-paired device in `adb devices`.
2. If an IP exists: `adb connect <ip>` then `adb shell getprop ro.product.model ro.product.name ro.build.version.name ro.build.version.release ro.build.version.sdk`.
3. Classify:
   - non-empty `ro.build.version.name` ∈ {5,6,7,8,14,16} → **FIRE_OS**
   - adb connects but props are empty, or the version string starts `1.` → **VEGA_OS**
   - adb cannot connect, or no IP is available → **UNKNOWN**

**Outcome handling — none of which stops the run:**

| Outcome | What the agent writes | Downstream effect |
| --- | --- | --- |
| FIRE_OS | `docs/FIRE_TV.md` with `STATUS: FIRE_OS`, model/build recorded, **Branch A** promoted to the top, B/B′ retained as "if this device is ever replaced" | none — Phase A–D proceed unchanged |
| VEGA_OS | `STATUS: VEGA_OS`, **Branch B** promoted, B′ priced, Branch A marked N/A with the Vega reason | none |
| UNKNOWN | `STATUS: UNKNOWN — OWNER INPUT REQUIRED`, all three branches published, with a 3-line "read Settings → My Fire TV → About and pick your branch" preamble | none; one row added to `docs/OWNER_CHECKLIST.md` |

Everything device-independent (which is all code) is built regardless. **Only Appendix A is conditional.**

---

### (b) Dioxus 0.6.3 → 0.7.10 — **MIGRATE IN PHASE 0. CONFIRMED.**

Both reviews recommend it; the codebase is 2,029 lines; 0.6 is superseded and unmaintained (**not** "end-of-life" — Dioxus has published no EOL policy, 0.6.3 is not yanked, and 0.6 docs are still hosted; the argument for migrating stands without the overstatement). Migrating later means redoing the PWA, multipart and realtime work.

**The v1 fallback ("ship Phase 0 on 0.6.3") is deleted.** It would require `cargo binstall dioxus-cli@0.6.3 --force` (the installed `dx` is 0.7.10), it forfeits the free multipart support, and a conditional framework version is exactly the kind of branch that breaks an autonomous run. There is one path.

#### D7, rewritten correctly

| # | Change | Effect on this codebase |
| --- | --- | --- |
| 1 | **There is no "server-fn crate 0.7."** `dioxus-fullstack` 0.7.10 does not depend on `server_fn` at any version. Server functions were rewritten in-house on raw axum (`dioxus-fullstack-core` + `dioxus-fullstack-macro`). | All 8 `#[server]` fns in `src/server/api.rs` are re-checked, not version-bumped. |
| 2 | **`ServerFnError` is now a non-generic Dioxus enum.** | `src/server/api.rs:175-177` (`to_server_error`) and every `ServerFnError::new(...)` call site is rewritten. |
| 3 | **`ServeConfigBuilder` is removed**, and `serve_dioxus_application` changed signature (now returns `Router<()>`, not `Self`). | `src/main.rs:21-31` **will not compile**. Replaced by the 0.7 entrypoint (`dioxus::serve` / `dioxus::server::router(App)`) inside `build_router()`. |
| 4 | **axum `^0.7` → `^0.8.4`**, `tower-http ^0.6.8`. | Path-param syntax, `Multipart` location, middleware signatures. Also resolves W8's duplicate `tower-http`. |
| 5 | **`multipart` is already enabled** by `dioxus-fullstack` 0.7.10's axum dependency. | **Positive.** R5 / R-08 / T2.5 get `axum::extract::Multipart` for free on a nested route — no extra feature, no extra crate. |
| 6 | **`event.files()` changed from `Option<Arc<dyn FileEngine>>` to `Vec<FileData>`.** Undocumented break. | Hits `src/client/components/routine.rs:229-233` exactly. |
| 7 | **`onsubmit` semantics inverted** — 0.6 auto-suppressed the native submit; 0.7 submits by default unless you call `prevent_default()`. **Silent behaviour change, no compile error.** | Every form in the client must be audited; the migration test asserts no page reload on submit. |
| 8 | **Default server-fn codec changed from URL-encoded to JSON.** | Wire-format change; covered by the round-trip assertion below. |
| 9 | **Prelude shrunk** — `use_drop`, `Runtime`, `queue_effect`, `provide_root_context` removed. | Explicit imports where used. |
| 10 | **Unchanged, contrary to v1's claims:** `ImageAssetOptions` still exists; single-arg `asset!()` unchanged (0.7 additionally allows hashless); **`fullstack_address_or_localhost()` still exists** — but it reads the bare `IP`/`PORT` env vars, so **T0.5 removes it from the release path** in favour of `FAMILY_HUB_ADDR`. | — |
| 11 | **Pinning.** `dioxus = "=0.7.10"`, `dioxus-cli-config = "=0.7.10"`, CLI `cargo binstall dioxus-cli@0.7.10` (**`@0.7.x` is not valid binstall syntax** — it takes a semver req; use the exact version or `--version "^0.7"`). **Never resolve to `0.8.0-alpha.*`.** 0.7.2 was breaking within the patch line, and 0.7.0/0.7.1 carry the Windows `dx serve --addr 0.0.0.0` → `AddrNotAvailable (os 10049)` bug (#4981, fixed by #5358) — 0.7.10 avoids it. | — |

#### T0.4 acceptance test — exact, agent-executable

Gate 1 — **the harness from T0.3 must pass unchanged** (this is why T0.3 exists and runs first, on 0.6.3):

1. `cargo test --features server` — 8 DB tests **plus** the T0.3 HTTP/WS tests, all green.
2. `cargo clippy --features server --all-targets -- -D warnings` — clean.
3. `cargo clippy --features web --target wasm32-unknown-unknown -- -D warnings` — clean.
4. `cargo fmt --check` — clean.

Gate 2 — **migration-specific assertions** (new tests in `tests/http_tests.rs`, all `#[tokio::test]` against an in-process server on an ephemeral port):

5. `GET /` → 200, body contains `Morning Routine`.
6. `GET /m` → 200, body contains the mobile marker string.
7. Round-trip **one server function over the new JSON codec**: `POST` to the `today` server-fn endpoint with `Content-Type: application/json` → 200, body parses as a `YYYY-MM-DD` string. *(Directly proves break #8.)*
8. Round-trip a **mutating** server fn (`toggle_routine_task`) and assert the DB row changed. *(Proves break #2 — `ServerFnError` on both the ok and err paths; include one deliberate error case asserting a structured error body, not a panic.)*
9. `GET /ws` with an upgrade → 101; send a `WsMessage::Draw`; assert a second connected client receives it. *(Proves the axum 0.8 WS layer.)*
10. A file-input handler compiled against `Vec<FileData>` exists and a unit test constructs one. *(Proves break #6 compiles.)*
11. `cargo tree -d` reports **no duplicate** `axum`, `tower-http`, or `hyper` major versions. *(W8.)*
12. `grep -r "server_fn" src/ Cargo.toml` returns nothing, and `grep -r "ServeConfigBuilder" src/` returns nothing. *(Proves breaks #1 and #3 were actually addressed rather than feature-gated away.)*
13. Every `form` element in `src/client/` either calls `prevent_default()` or is annotated `// intentional native submit`. Asserted by a `#[test]` that greps the client sources. *(Proves break #7 was audited — the only silent break.)*
14. `dx build --platform web --release` exits 0. *(Not `dx serve` — no long-running process in an unattended run.)*

**Stop-loss:** 2 attempts at Opus; on the second failure, halt the branch, write `docs/BLOCKED.md` with the last 200 lines of compiler output and the specific break number reached. Do not fall back to 0.6.3.

---

### (c) Realtime protocol v2 — **SPECIFIED** (implemented by T1.2)

#### Envelope

Server-authoritative split. Clients send `ClientMessage`; the server broadcasts `ServerMessage`. **The server never rebroadcasts client JSON verbatim** (fixes G13). Unknown/unauthorised client messages are dropped with a `tracing::warn`, never echoed.

```rust
// shared/types.rs
pub struct ClientId(pub Uuid);        // generated by the SERVER at upgrade, sent to the client in Hello

pub enum ClientMessage {
    Hello { protocol: u8 },                            // protocol = 2
    Ping { nonce: u64 },
    Draw { board_id: i64, stroke: Stroke },            // ONE stroke = many segments, one message
    ClearBoard { board_id: i64 },
    SetView { view: View, auth: Option<SessionToken> },
    SetActiveProfile { user_id: i64, auth: Option<SessionToken> },
    RequestSnapshot { board_id: i64, since_seq: i64 },
}

pub enum ServerMessage {
    Hello { client_id: ClientId, protocol: u8, server_time: DateTime<Local>, today: NaiveDate },
    Pong { nonce: u64 },
    Resync { reason: ResyncReason },                   // Lagged | ServerRestart | ClientRequested
    Draw { board_id: i64, seq: i64, origin: ClientId, stroke: Stroke },
    BoardCleared { board_id: i64, seq: i64, origin: ClientId },
    Snapshot { board_id: i64, seq: i64, strokes: Vec<Stroke> },
    RoutineUpdated { user_id: i64, date: NaiveDate },
    TasksUpdated  { user_id: i64, date: NaiveDate },   // W1
    ProfilesUpdated,                                    // W6
    CalendarUpdated { date: NaiveDate },
    DayRolled { date: NaiveDate },
    SetView { view: View },
    SetActiveProfile { user_id: i64 },
    Health { stale: bool, last_update: DateTime<Local> },
}
```

- **`ClientId` origin.** Assigned **by the server** at upgrade (a v4 UUID), returned in `Hello`, and stamped on every `Draw`/`BoardCleared` the server fans out. **Clients discard messages whose `origin` equals their own id** — this fixes W2 (double-draw) without the client being able to spoof another client's identity.
- **`RoutineUpdated`/`TasksUpdated` carry `user_id` and `date`;** clients refetch only the matching profile (W7).
- `SetView`/`SetActiveProfile` from a client require a valid parent session token **or** are rejected unless `user_id` equals the sender's own active profile (R-23b). Server re-emits them as `ServerMessage` only after that check.

#### Stroke batching and rate limiting (R-06)

**Client (`whiteboard.rs`):**
- `onpointerdown` opens a stroke; `onpointermove` **appends to a local `Vec<StrokePoint>` and paints locally** — it does **not** send.
- A `requestAnimationFrame`-driven flush (via `gloo-timers` at 33 ms if `rAF` is inconvenient) emits **at most one `ClientMessage::Draw` per frame**, carrying every point accumulated since the last flush, with the points simplified by a 0.002-normalized-unit distance threshold.
- Hard cap: **≤ 30 messages/second per client**; excess frames coalesce into the next flush. `onpointerup`/`onpointerleave` force a final flush and close the stroke.
- Net effect: a child scribbling for 10 s produces ≤ 300 messages of ~20 points each, instead of ~6,000 single-segment messages.

**Server:**
- Per-connection token bucket: **40 msg/s burst 80**. Over budget → drop the message and `tracing::warn`; three consecutive seconds over budget → send `Resync { reason: Lagged }` and close *that* client only.
- Per-connection **bounded outbound queue of 256** with **drop-oldest**. If ≥ 32 messages are dropped, enqueue a single `Resync` instead of the backlog.
- Broadcast channel capacity raised 256 → **1024**.

#### `RecvError::Lagged` — the kill switch (R-06a)

```rust
loop {
    match receiver.recv().await {
        Ok(msg) => { if outbound.send(msg).await.is_err() { break; } }          // client gone
        Err(RecvError::Lagged(n)) => {
            tracing::warn!("client {client_id} lagged {n} messages; resyncing");
            // receiver is ALREADY repositioned to the oldest retained message by tokio;
            // no resubscribe needed, but resubscribe defensively to drop the backlog:
            receiver = sender().subscribe();
            if outbound.send(ServerMessage::Resync { reason: ResyncReason::Lagged }).await.is_err() { break; }
            continue;                                    // NEVER break on Lagged
        }
        Err(RecvError::Closed) => break,                 // server shutting down
    }
}
```

On `Resync` the client: bumps every version signal, re-requests `Snapshot { since_seq: 0 }`, and refetches routine/tasks/calendar for the current date. It **does not** tear down the socket.

Also fixed: when the client pump exits, its `rx` is dropped and outbound sends silently no-op (G3's missing half) — the client's send half is now owned by the reconnect supervisor and is re-created on every reconnect.

#### Heartbeat and reconnect

- **Heartbeat:** client sends `Ping { nonce }` every **20 s**; server replies `Pong`. Missing **2 consecutive** Pongs (≥ 45 s) → the client treats the socket as dead and reconnects. Server independently drops a connection that has sent nothing for **90 s**.
- **Reconnect backoff:** `1s, 2s, 4s, 8s, 15s, 30s, 30s, …` with **±20 % jitter**, capped at 30 s, reset to 1 s after 60 s of a healthy connection. Unit-tested as a pure function `backoff(attempt: u32) -> Duration` (R-06 / H-26).
- On every successful reconnect: send `Hello`, receive the new `ClientId`, request `Snapshot`, bump all versions.

#### Midnight tick — DST-safe (F29)

```rust
loop {
    let now  = Local::now();
    let next = (now.date_naive() + Days::new(1))
        .and_hms_opt(0, 0, 0).unwrap()
        .and_local_timezone(Local)
        .earliest()                                     // deterministic on the spring-forward gap
        .unwrap_or_else(|| now + Duration::hours(24));
    tokio::time::sleep((next - now).to_std().unwrap_or(Duration::from_secs(60))).await;
    let today = Local::now().date_naive();
    publish(&ServerMessage::DayRolled { date: today });
    calendar::force_poll().await;                        // W4
    retention::run_daily(today).await;                   // strokes, photos, logs, WAL checkpoint
}
```

**Recomputed every iteration** — no accumulated drift, correct on 23- and 25-hour local days.

#### T1.2 acceptance (all `#[tokio::test]`, no browser)

1. `backoff(0..10)` matches the exact schedule; jitter within ±20 %.
2. **Lag test:** one slow client + a producer emitting 5,000 messages; assert the slow client receives a `Resync`, the socket **stays open**, and the *fast* client received all 5,000.
3. **Load test:** 8 concurrent Rust WS clients × 30 msg/s × 30 s (7,200 messages). Assert zero closed sockets, p99 broadcast latency < 250 ms, server RSS growth < 50 MB.
4. **Echo test:** client A sends `Draw`; A receives it back with `origin == A.client_id`; B receives it with `origin == A.client_id`. Assert A's renderer skips it.
5. **Spoof test:** a client sends a raw `{"CalendarUpdated":...}` payload; assert no other client receives anything.
6. **Auth test:** unauthenticated `SetView` is rejected; the same message with a valid session token is broadcast.
7. **Reconnect test:** start server → connect Rust WS client → kill the server task → restart → assert the client re-established **and received `Snapshot`** in < 30 s wall clock.
8. **Midnight test:** inject a clock at 23:59:58 local on both DST transition dates for `America/New_York` and `Europe/London`; assert the tick fires once, at the correct instant, on each.
9. **Rate-limit test:** a client emitting 200 msg/s is throttled, warned and then resynced — and **no other client is affected**.

---

## P3. Rewritten task list (replaces PLAN §3 wholesale)

Legend — **Tier:** H = Haiku, S = Sonnet, O = Opus, B = Boss (Fable). Every acceptance test runs unattended on this Windows PC via `cargo`, `dx build`, or a file assertion. **No task's acceptance requires a second device, a phone, a reboot, or a human.**

### Phase 0 — floor (mostly serial; blocks everything)

| ID | Title | Depends on | Tier | Acceptance test (agent-executable) |
| --- | --- | --- | --- | --- |
| **T0.0** | Device-ID gate — probe the TV, write all three kiosk branches | — | **H** | `docs/FIRE_TV.md` exists; line 1 matches `^STATUS: (FIRE_OS\|VEGA_OS\|UNKNOWN)`; the file contains headings `## Branch A`, `## Branch B`, `## Branch B′`; `docs/OWNER_CHECKLIST.md` contains a `Device` row. A `#[test]` in `tests/docs_tests.rs` asserts all four. **Never fails the run on outcome.** |
| **T0.1** | `docs/NON_RUST.md` pre-filled (9 rows: `sw.js`, Tailwind standalone, Fully Kiosk + cost, `adb`, GH Actions YAML, wasm-bindgen glue, browser/WebView, Silk, Android SDK-if-revived); pin Tailwind standalone `tailwindcss-windows-x64` v3.4.17 into `docs/DEV_WINDOWS.md`; fix `tailwind.config.js:3`'s dead `./index.html` glob | — | **H** | `#[test]` asserts `docs/NON_RUST.md` has ≥ 9 table rows and contains the strings `sw.js`, `tailwindcss-windows-x64`, `Fully Kiosk`, `adb`; `tailwind.config.js` contains no `index.html`; `docs/DEV_WINDOWS.md` step 1 is the PATH prefix. |
| **T0.2** | Dependency hygiene: drop `sqlx` `tls-native-tls`; pin `sqlx =0.8.6`; add `web-sys` features (`Navigator`, `ServiceWorkerContainer`, `ServiceWorker`, `ResizeObserver`, `File`, `Blob`, `FormData`, `Performance`); add `build.rs` with `cargo:rerun-if-changed=migrations`; add `uuid`, `image`, `tokio-tungstenite` (dev) | — | **S** | `cargo tree -e normal \| grep -c openssl-sys` = 0 and `grep -c native-tls` = 0; both clippy targets clean; `cargo test --features server` still 8/8; `build.rs` exists and emits the rerun line. |
| **T0.3** | HTTP/WS integration harness **on 0.6.3** — `tests/http_tests.rs`: in-process server on an ephemeral port, HTTP GET assertions, one server-fn round trip, `/ws` upgrade + fan-out | T0.2 | **S** | `cargo test --features server` green with ≥ 5 new tests named `http_*` / `ws_*`; each asserts a concrete status code or body substring (no `assert!(true)`); `cargo test` run twice back-to-back both green (no port leakage). |
| **T0.4** | **Dioxus 0.7.10 migration** (P2b, all 11 items) | T0.3 | **O** | The 14-point gate in **P2b**. |
| **T0.5** | Config & paths: `FamilyHubConfig` from `familyhub.toml` + env; `FAMILY_HUB_DATA_DIR` (default `%ProgramData%\FamilyHub`); `FAMILY_HUB_ADDR`; **remove `fullstack_address_or_localhost()` from the release path**; every DB/upload/screensaver/PKI/log path resolved absolutely from the base dir and **logged at startup** | T0.4 | **S** | `#[test]` sets `FAMILY_HUB_DATA_DIR` to a temp dir, boots the server, asserts `family.db` exists **inside it and nowhere else** (asserts `!Path::new("family.db").exists()` relative to CWD); `grep -rn "sqlite://family.db\|\"assets/uploads\"\|\"assets/screensaver\"" src/` returns nothing; a `#[test]` asserts `config.http_addr == 0.0.0.0:8080` by default. |
| **T0.6** | `build_router() -> Router` + `run(config)` refactor; root routes for `/manifest.webmanifest`, `/sw.js`, `/ca.crt`, `/health` stubs; `ServeDir` for `/uploads` **and** `assets/screensaver`; `/` → `/tv` redirect; `/tv` and `/m` routes | T0.5 | **S** | `#[test]` calls `build_router(&test_config())` and asserts, via `tower::ServiceExt::oneshot`, the status of 9 named routes; `GET /assets/screensaver/<fixture>.jpg` → 200 with `image/jpeg`; `GET /manifest.webmanifest` → 200 with `application/manifest+json`; `GET /` → 308 to `/tv`; `src/main.rs` is < 25 lines and contains no route definitions. |
| **T0.7** | Assets: 3 CC0 screensaver placeholder JPEGs; one 12 MP JPEG test fixture under `tests/fixtures/`; PWA icons 192/512 + maskable generated by a **Rust** `xtask` using `resvg`/`tiny-skia` from a Sheffield-palette monogram SVG | T0.1 | **H** | `cargo run -p xtask -- icons` exits 0 and writes 3 PNGs; `#[test]` asserts each PNG's dimensions and that the maskable one has ≥ 10 % safe-zone padding; `assets/screensaver/*.jpg` count ≥ 3; `tests/fixtures/photo_12mp.jpg` exists and decodes to ≥ 4000×3000 via the `image` crate. |
| **T0.8** | CI (`.github/workflows/ci.yml`): fmt, clippy ×2 (`-D warnings`), `cargo test --features server`, `dx build --platform web --release`, Tailwind rebuild + **fail on diff** (W9), `cargo tree -d` duplicate check (W8), Windows-x64 release build. **No aarch64 job** (R-25/R-26/E6). | T0.4 | **S** | `act` is not available, so: a `#[test]` parses the YAML and asserts the 7 named steps exist and that no step string contains `aarch64`; `cargo fmt --check`, both clippy invocations and the tailwind-diff command are each executed locally by the agent and exit 0. |

### Phase 1 — foundations (T1.1/T1.2/T1.3 parallel; then the rest)

| ID | Title | Depends on | Tier | Acceptance test |
| --- | --- | --- | --- | --- |
| **T1.1** | **Migrations & storage** — sole owner of `migrations/`. `0001_init.sql` reproduces today's schema with `CREATE TABLE IF NOT EXISTS`; startup baselining (if legacy tables exist and `_sqlx_migrations` does not, insert the 0001 row as applied). `0002_core.sql`: `events`, `whiteboard_strokes` (one row per **stroke**, `seq`, `board_id`, `cleared_at` watermark), `custom_tasks.due_date`, `google_sync_state`, `settings`. Pragmas: WAL, `synchronous=NORMAL`, `busy_timeout=30s`. **Two pools** (read max 5, write max 1). `wal_checkpoint(TRUNCATE)` on the midnight tick. | T0.6 | **O** | 8 existing tests green; new tests: (a) fresh DB → migrator runs → all tables present; (b) **a fixture copy of a v1 `family.db` with routine logs is baselined, migrated, and every log row survives**; (c) restore-from-backup drill: `VACUUM INTO` a copy, delete the original, restore, re-migrate, assert row counts identical; (d) `PRAGMA journal_mode` returns `wal`, `busy_timeout` returns 30000; (e) 20 concurrent writers via the write pool complete with zero `SQLITE_BUSY`; (f) a DST-ambiguity unit test on the date helper resolves to the **earliest** offset. |
| **T1.2** | **Realtime protocol v2** (P2c in full) — envelope split, `ClientId`, stroke batching, `Lagged` handling, bounded queue, rate limit, heartbeat, backoff, DST-safe midnight tick, `ProfilesUpdated`/`TasksUpdated`, `user_id`-scoped refetch. **Also writes the stroke/snapshot/echo protocol doc `docs/PROTOCOL.md` that T2.3 implements against.** | T0.6 | **O** | The 9-point suite in **P2c**, plus: `docs/PROTOCOL.md` exists and a `#[test]` asserts it documents every `ClientMessage`/`ServerMessage` variant by name (compile-time exhaustive match over the enums). |
| **T1.3** | **TLS + PKI + dual listener + mDNS + QR** — `CertSource::SelfSignedCa`; `rcgen 0.14` CA (10 y) + leaf (**397 d**, explicit `not_before`/`not_after`), SANs = reserved IP + every non-loopback IPv4 + `familyhub.local` + `localhost` + `127.0.0.1`; auto re-issue at 30 d remaining with hot reload; keys at `<data>\pki\` with restricted ACLs; `tokio-rustls` + `hyper-util` (**not** `axum-server`); `CryptoProvider` installed as the **first line of main**; HTTP :8080 (serves everything, 308s only `/m*`), HTTPS :8443; `/ca.crt` on both; **one** `mdns-sd` `ServiceDaemon`, FQDN with trailing dot; `fast_qr` SVG component rendering the HTTPS phone URL | T0.6 | **O** | (a) A `rustls` client with the generated CA in its root store completes a handshake against the live :8443 listener and `GET /health` → 200. (b) `GET http://127.0.0.1:8080/ca.crt` → 200, `application/x-x509-ca-cert`, parses as a valid X.509 CA. (c) `GET http://127.0.0.1:8080/m` → 308 to `https://…:8443/m`; `GET http://127.0.0.1:8080/tv` → **200, not a redirect**. (d) Leaf `not_after - not_before` is **≥ 396 and ≤ 398 days**; SAN list contains every non-loopback IPv4 of this host. (e) Injecting a leaf with 29 days remaining triggers re-issue and the listener serves the new cert **without restart**. (f) An mDNS `A` query for `familyhub.local` issued from this host is answered with this host's IP. (g) The QR SVG decodes (via a Rust QR decoder in a dev-dependency, or a byte-level round-trip of the encoder) to exactly `https://<ip>:8443/m`. |
| **T1.4** | **Profiles + settings + parent PIN** — `0003_profiles.sql`: `profiles(id,name,color,avatar,is_parent,sort_order)`, drop both `CHECK (user_id BETWEEN 1 AND 4)` constraints (`db.rs:62` **and** `db.rs:77`) in favour of FKs to `profiles`; server fns; **6-digit** PIN, argon2id `=0.6.0`, server-side enforcement, session token, exponential backoff on failure (no lockout); first-run setup code written to the log, to `<data>\setup-code.txt`, and displayed on the TV. **Explicitly: replace `tests/db_tests.rs:148-157` with a foreign-key violation test** (W5). | T1.1, T1.2 | **S** | New tests: FK violation for an unknown `user_id`; rename persists and emits `ProfilesUpdated` (asserted on a second WS client); a 5th and 6th profile can be created; PIN verify succeeds once and fails 10× with monotonically increasing delay ≥ 2^n ms; **PIN check enforced in the server fn, not the client** — asserted by calling the privileged server fn directly with no session and expecting an error. |
| **T1.5** | **Date correctness + authorization + missing broadcasts** — `toggle_routine_task(date, user_id, …)` and `toggle_custom_task(date, user_id, …)` take an **explicit date** validated within ±1 day server-side; client-generated idempotency key on every mutation, deduped in a `mutation_log` table; ownership check on every mutating fn; `toggle_custom_task` publishes `TasksUpdated` (W1); `today().unwrap_or_default()` replaced by an explicit `Error` state (R-24a) | T1.1, T1.2 | **S** | Tests: toggling with `date = yesterday` writes to yesterday's row; `date = 3 days ago` is rejected; the same idempotency key applied twice produces one row change; user 2 cannot toggle user 3's task (error, no write); `toggle_custom_task` emits `TasksUpdated` on a connected WS client; a simulated `today()` failure renders the `Error` variant (unit test on the component's state machine, not a browser). |
| **T1.6** | **Backup, retention, delete paths** — nightly `VACUUM INTO <data>\backups\family-YYYYMMDD-HHMM.db` + `uploads` snapshot; N-day retention (default 14 backups); delete-custom-task server fn that **also removes its photo file**; stroke compaction (hard-delete strokes before `cleared_at`, keep the last 2,000); photo retention 30 days; log rotation. **CA private key excluded from backups.** | T1.1 | **S** | Tests: (a) with a writer transaction open, `VACUUM INTO` produces a backup that opens cleanly and has identical row counts — and a **plain file copy under the same conditions is asserted to differ or fail**, proving the point; (b) restore drill: delete the live DB, restore the backup, boot, assert row counts; (c) 20 backups → 14 retained, oldest deleted; (d) deleting a task removes both the row and the file on disk; (e) compaction leaves exactly 2,000 strokes; (f) `backups/` contains no `.key` file. |
| **T1.7** | **`/health` + staleness surfacing** — JSON: DB reachable, last successful Google poll, **cert `not_after`**, days-to-expiry, disk free on the data volume, connected WS clients, uptime, migration version. TV shows a permanent "updated HH:MM" and a red disconnected badge when the socket is down or data is older than 90 s. | T1.1, T1.2, T1.3 | **S** | `GET /health` → 200 JSON with all 8 keys, correct types; with the DB pool closed → 503 and `"db": false`; a unit test on the staleness state machine asserts the badge turns on at > 90 s and off within 2 s of a message; `days_to_expiry` matches the leaf's actual `not_after`. |
| **T1.8** | **`CertSource::AcmeDns01`** — `instant-acme =0.8.5`, `DnsProvider` trait + a Cloudflare impl + `MockProvider`; renewal scheduler shared with T1.3; enabled only by `certs.mode = "acme_dns01"` in `familyhub.toml`. **Default: off.** | T1.3 | **O** | Tests against `MockProvider`: (a) the order flow requests a DNS-01 challenge, writes the TXT record via the provider, polls, and installs the returned cert into the same hot-reload slot T1.3 uses; (b) `renew_if_due` is true at 29 days and false at 31; (c) with `certs.mode` absent, `CertSource::SelfSignedCa` is selected and **no network call is made** (asserted by a provider that panics on use); (d) a config with `acme_dns01` but no token fails **at startup with a clear error**, not at renewal time. Live Let's Encrypt round-trip → Appendix A. |

### Phase 2 — the two surfaces (T2.1–T2.5 parallel; T2.6/T2.7 after)

| ID | Title | Depends on | Tier | Acceptance test |
| --- | --- | --- | --- | --- |
| **T2.1** | **Kiosk / 10-foot UI** — D-pad focus system with deterministic focus order; overscan-safe 5 % padding; ≥ 28 px body / ≥ 44 px headings; **a child completes a full routine on the TV alone** (R-12); receives `SetView`/`SetActiveProfile`; QR overlay; key-code debug overlay (`?keys=1`); Back/Backspace, **no Esc** (R-11); resolve D8's Left/Right vs the horizontal profile selector by making **Up/Down** switch profiles and Left/Right cycle panels | T1.2, T1.4 | **O** | (a) A `#[test]` walks the TV component tree and asserts the **exact ordered list of focusable element ids** matches a committed golden file — and that every one has a visible focus ring class. (b) An integration test injects `ServerMessage::SetView{Whiteboard}` and asserts the rendered view changed. (c) Same for `SetActiveProfile`. (d) A pure-function test on the key handler: for each of `ArrowUp/Down/Left/Right/Enter/Backspace/MediaPlayPause`, assert the resulting focus/view transition. (e) A test asserts every routine item is reachable from the profile selector in ≤ 12 key presses. (f) Typography/overscan assertions by grepping the compiled Tailwind classes against a committed allowlist. **No screenshot review.** |
| **T2.2** | **Phone PWA** — `/manifest.webmanifest` at root with `scope:"/"`, `start_url:"/m"`, icons from T0.7; `/sw.js` at root from `include_str!` (app-shell precache, network-first for server fns, cache-first for `/uploads` + screensaver); registration via `web_sys`; bottom tabs `/m` → Routine · Calendar · Board · TV Remote · Settings; offline mutation queue in `localStorage` with **date + idempotency key** (R-15); **sends** `SetView`/`SetActiveProfile` | T1.2, T1.3, T1.4 | **O** | (a) `GET /manifest.webmanifest` → 200, `application/manifest+json`, parses with `scope=="/"`, `start_url=="/m"`, `display=="standalone"`, ≥ 2 icons of which ≥ 1 `purpose` contains `maskable`. (b) `GET /sw.js` → 200, `text/javascript`, body ≤ 6 KB, contains `install`/`activate`/`fetch` listeners. (c) A `#[test]` asserts `start_url` is inside `scope` (R-16) and that neither path contains a content hash. (d) The offline queue is a **pure Rust struct** with tests: enqueue while offline → 3 entries with distinct keys and stamped dates; replay → 3 server calls; replay twice → still 3 effects (idempotent); an entry older than 48 h is dropped with a toast event. (e) `docs/PWA.md` states the per-platform offline promise (Android: Background Sync; **iOS: replay on next app open**) — asserted by a doc test. **No Lighthouse.** |
| **T2.3** | **Whiteboard v2** — implements `docs/PROTOCOL.md` from T1.2: persistence (one row per stroke), snapshot replay on connect, `cleared_at` watermark, undo-last-stroke, **replace the single-slot `inbound_stroke` signal with a drained queue** (R-22), `ResizeObserver` that re-syncs DPR and **repaints from the stroke log**. **One board (`board_id = 1`)**; multiple named boards cut. | T1.1, T1.2 | **S** | (a) Persist 500 strokes; open a fresh WS connection; assert `Snapshot` contains 500 in `seq` order. (b) `ClearBoard` moves `cleared_at`; the next `Snapshot` is empty; the rows are gone after compaction. (c) Undo removes exactly the last stroke of the **calling client**, not another's. (d) A unit test feeds 50 `Draw` messages into the client queue between two render ticks and asserts **all 50** are drawn (proves R-22a). (e) A unit test resizes the canvas model and asserts a repaint-from-log is issued (proves R-22b). |
| **T2.4** | **Calendar v2** — events persisted to SQLite; local CRUD; Today + Week views; **windowed** Google polling with **full replace of the window** per poll (R-19: no `syncToken`, no `orderBy`/`timeMin` conflict, deletions handled by construction); `rrule =0.14.0` for local recurrence, always `all(limit)`; DST-deterministic `rfc3339_local` (earliest offset); explicit `Loading/Empty/Error` states replacing `is_empty()` (W3); midnight tick forces a poll (W4). **ICS import cut.** | T1.1, T1.2 | **O** | (a) A **fixture-driven** Google poll (committed JSON response, no service account — H-24): 3 events upserted; re-poll with a 2-event response → the window contains exactly 2, the removed one is gone. (b) `rrule` DST test: a 02:30 daily recurring event expanded across **both** US and UK transitions produces the correct local times, with a named assertion per boundary. (c) Week view for a week containing a DST change has exactly 7 days and correct day boundaries. (d) `all(limit)` cap is enforced — a pathological RRULE returns at most `limit` and does not hang (test with a 2 s timeout). (e) Deleting the last event of a day makes the panel render `Empty`, not the stale event. |
| **T2.5** | **Photo tasks v2** — multipart route (axum 0.8 `Multipart`, free from T0.4) replacing the base64 server fn; **`DefaultBodyLimit::max(25 MiB)` on that route only** (R-08); client-side downscale to ≤ 1600 px JPEG; **server-side extension allowlist + re-encode** (`image` crate) to jpg/png/webp (R-23c); `Content-Type` forced, `X-Content-Type-Options: nosniff`, `Content-Disposition: attachment` on `/uploads`; `due_date`; delete task + file | T1.1, T1.5, T0.7 | **S** | (a) POST `tests/fixtures/photo_12mp.jpg` (a real 12 MP file from T0.7) to the upload route over loopback → **2xx**, elapsed < 3 s, stored file ≤ 400 KB. (b) The same POST to a route without the raised limit → 413 (proves the limit is the cause). (c) Uploading a file named `x.svg` with `image/svg+xml` → rejected 415, nothing written. (d) Uploading a valid PNG renamed `.jpg` → stored with the **correct** extension after re-encode. (e) `GET /uploads/<f>` carries `nosniff` and `attachment`. (f) A task with `due_date = yesterday` is absent from today's list; deleting it removes the row **and** the file. |
| **T2.6** | **Cross-surface loop test** (E4) — the phone→TV control path end-to-end, in Rust | T2.1, T2.2, T2.3 | **S** | One `#[tokio::test]`: boot the server; open two Rust WS clients tagged `phone` and `tv`; the phone sends `SetView{Calendar}` with a valid parent session; assert the TV client receives `ServerMessage::SetView` **within 1 s**; assert an unauthenticated phone's `SetView` is not delivered; the phone draws a stroke and the TV receives it with `origin == phone`; kill and restart the server and assert both clients resync within 30 s. |
| **T2.7** | **Screensaver completion** — phone upload route reusing T2.5's pipeline; placeholders wired; idle timeout 10 min; optional scheduled `SetView(Screensaver)` **off by default** | T2.5 | **S** | `GET /api/screensaver` lists ≥ 3 images and every returned URL returns 200 with `image/jpeg` (proves R-31/G8 end-to-end); uploading a new image makes it appear in the list; the idle-timeout state machine fires at 600 s in a unit test; with the schedule disabled, no `SetView` is emitted at the configured hour. |

### Phase 3 — ship (serial)

| ID | Title | Depends on | Tier | Acceptance test |
| --- | --- | --- | --- | --- |
| **T3.1** | **Windows service** — `family-hub.exe install\|uninstall\|start\|stop\|status\|run\|tv-probe` via `windows-service::service_manager` (**no PowerShell scripts** — I-10); tokio runtime built **inside `service_main`** (R-14); `StartPending` with incrementing checkpoints; file + Event Log output as the **first** statement; log rotation with a size cap; firewall rules for TCP 8080/8443 and UDP 5353; power plan set to never sleep/hibernate | T0.5, T1.3, T1.6 | **S** | (a) `family-hub.exe install` → `sc query FamilyHub` exists; `start` → RUNNING; `GET http://127.0.0.1:8080/health` → 200; `stop` → STOPPED; `uninstall` → gone. (b) **CWD test:** run the service binary with CWD forced to `C:\Windows\System32`; assert `family.db` is created under `%ProgramData%\FamilyHub` and **`C:\Windows\System32\family.db` does not exist**. (c) A deliberate startup failure appears in the log file within 5 s (proves logging precedes everything). (d) `netsh advfirewall firewall show rule name=FamilyHub*` lists 3 rules. (e) Writing 20 MB of log lines produces ≥ 2 rotated files and the newest is under the cap. **Physical reboot → Appendix A.** |
| **T3.2** | **Runbooks** — `docs/FIRE_TV.md` finalised (all three branches, with the branch chosen by T0.0 promoted); `docs/OWNER_CHECKLIST.md`; `docs/DEV_WINDOWS.md`; `docs/PWA.md`; `docs/RECOVERY.md` (what to do when the TV is blank, the cert expired, the DB is corrupt) | T0.0, T3.1, T2.6 | **O** | A `#[test]` asserts: every doc exists; `FIRE_TV.md` covers, by string match, `sleep_timeout`, `HDMI-CEC`, `SYSTEM_ALERT_WINDOW`, `GET_USAGE_STATS`, `Screensaver`, `Silk`, and the Fully Kiosk PLUS price; `OWNER_CHECKLIST.md` contains ≥ 8 numbered steps each with an explicit pass criterion; `RECOVERY.md` covers ≥ 4 named failure modes; **every internal doc link resolves** (link checker test). |
| **T3.3** | **Autonomous verification pass** — re-run every acceptance test in Phases 0–3, capture output into `docs/VERIFICATION.md` with per-task pass/fail and timings | all above | **H** (+ **B** review) | `docs/VERIFICATION.md` contains one row per task ID with a `PASS`/`FAIL` and a command transcript; a `#[test]` asserts every task ID in `PLAN.md §3` appears exactly once and none is `FAIL`. |
| **T3.4** | **Palette-faithful polish** — typography scale, spacing rhythm, panel composition, contrast, using the **existing** Sheffield palette. **No owner sign-off.** | T2.1, T2.2 | **O** | A `#[test]` asserts: every foreground/background token pair in the palette meets **WCAG AA (≥ 4.5:1 body, ≥ 3:1 large)**, computed in Rust from the hex values; the type scale uses ≤ 6 sizes, all ≥ 28 px on `/tv`; every `/tv` container carries the 5 % overscan padding class; no `hover:`-only affordance appears in a `/tv` component (grep assertion). |

### Phase 4 — post-run, owner-gated (**outside the autonomous run**)

`P4.1` design pass against inspiration images when they arrive (default if they never arrive: ship T3.4's output and stop — E7). `P4.2` enable `acme_dns01` if the owner buys a domain. `P4.3` device install per Appendix A. `P4.4` Raspberry Pi migration (code is already portable; the CI job stays cut until it is actually wanted).

**Haiku has real work:** T0.0, T0.1, T0.7, T3.3, plus standing `cargo fmt` sweeps and changelog assembly between phases.

---

## P4. Dependency graph, parallel groups, file ownership

### Edge list

```
T0.0 → (none)
T0.1 → (none)
T0.2 → (none)
T0.3 → T0.2
T0.4 → T0.3
T0.5 → T0.4
T0.6 → T0.5
T0.7 → T0.1
T0.8 → T0.4

T1.1 → T0.6
T1.2 → T0.6
T1.3 → T0.6
T1.4 → T1.1, T1.2
T1.5 → T1.1, T1.2
T1.6 → T1.1
T1.7 → T1.1, T1.2, T1.3
T1.8 → T1.3

T2.1 → T1.2, T1.4
T2.2 → T1.2, T1.3, T1.4
T2.3 → T1.1, T1.2
T2.4 → T1.1, T1.2
T2.5 → T1.1, T1.5, T0.7
T2.6 → T2.1, T2.2, T2.3
T2.7 → T2.5

T3.1 → T0.5, T1.3, T1.6
T3.2 → T0.0, T3.1, T2.6
T3.3 → all
T3.4 → T2.1, T2.2
```

### Parallel groups

| Wave | Tasks | Notes |
| --- | --- | --- |
| **0-a** | **T0.0, T0.1, T0.2** | Three agents, three disjoint file sets. |
| **0-b** | T0.3 | serial |
| **0-c** | **T0.4** | serial, Opus, the riskiest task in Phase 0 |
| **0-d** | T0.5 | serial |
| **0-e** | **T0.6, T0.7, T0.8** | T0.7 could have run in wave 0-a; scheduled here so T0.8's CI sees the final tree. |
| **1-a** | **T1.1, T1.2, T1.3** | Three Opus agents. Disjoint modules — see ownership table. |
| **1-b** | **T1.4, T1.5, T1.6, T1.7, T1.8** | Five agents. T1.4 owns migration `0003`; T1.5 touches no migration. |
| **2-a** | **T2.1, T2.2, T2.3, T2.4, T2.5** | Five agents, one per surface/feature module. |
| **2-b** | **T2.6, T2.7** | two agents |
| **3** | T3.1 → T3.2 → T3.3; **T3.4 parallel with T3.1/T3.2** | |

### File / module ownership — no two parallel tasks touch the same file

`src/main.rs` is reduced to < 25 lines by **T0.6** and is **frozen thereafter** — no Phase 1 or 2 task may edit it. This is the single change that removes E3's five-way conflict.

| File / directory | Sole owner | Later editors |
| --- | --- | --- |
| `src/main.rs` | T0.6 | **none** (frozen) |
| `src/server/router.rs` (`build_router`, `run`) | T0.6 | T1.3 (adds the TLS listener + `/ca.crt`), T2.5 (adds the multipart route) — **serialized: T1.3 is wave 1-a, T2.5 is wave 2-a** |
| `src/server/config.rs` | T0.5 | T1.3 (`certs` section), T1.8 (`acme` section) — sequential (T1.8 depends on T1.3) |
| `migrations/**` | **T1.1** | T1.4 adds `0003_profiles.sql` only. **No other task may add or edit a migration.** Numbers are assigned here: `0001_init`, `0002_core` (T1.1), `0003_profiles` (T1.4). Anything discovered later gets `0004+` from Boss. |
| `src/server/db.rs` | T1.1 | T1.5 (server-fn bodies), T1.6 (retention fns) — different waves |
| `src/shared/types.rs` | **T1.2** | T1.4 appends `Profile` (wave 1-b, after T1.2) |
| `src/server/api.rs::realtime` | T1.2 | none |
| `src/server/api.rs` (server fns) | T1.5 | T1.4 adds profile fns — **both wave 1-b: split the file first.** T1.2 lands `api/mod.rs`, `api/realtime.rs`, `api/routine.rs`, `api/profiles.rs`, `api/calendar.rs`, `api/screensaver.rs` as part of its refactor, so T1.4 and T1.5 own different files. |
| `src/client/realtime.rs` | T1.2 | none |
| `src/server/{pki,tls,mdns}.rs` | T1.3 | T1.8 (`tls.rs` `CertSource` enum only) |
| `src/server/acme.rs` | T1.8 | none |
| `src/server/health.rs` | T1.7 | none |
| `src/server/backup.rs` | T1.6 | none |
| `src/server/auth.rs` | T1.4 | none |
| `src/client/components/tv/**` | T2.1 | T3.4 (styling only) |
| `src/client/components/mobile/**` | T2.2 | T3.4 (styling only) |
| `src/client/components/whiteboard.rs` | T2.3 | none |
| `src/client/components/calendar.rs`, `src/server/calendar.rs` | T2.4 | none |
| `src/client/components/routine.rs` | T2.5 | (T1.5 lands its date/authz changes first, wave 1-b) |
| `assets/**`, `xtask/**` | T0.7 | T2.7 (screensaver uploads at runtime, not in-tree) |
| `docs/**` | per-task doc files, disjoint | T3.2 consolidates |
| `.github/workflows/**` | T0.8 | none |
| `Cargo.toml` | **T0.2**, then **T0.4** | Any later crate addition goes through Boss in a serialized micro-commit between waves — **agents in the same wave never both edit `Cargo.toml`.** |

**Rule:** if a task discovers it needs a file it does not own, it writes the request to `docs/HANDOFF.md` and Boss applies it between waves. It does not edit.

---

## P5. Autonomy policy

### P5.1 Retry, escalation, halt

1. **Two attempts per task at its assigned tier.** An attempt ends when the acceptance test suite is run and reported.
2. On the **second failure**, escalate **one tier** (Haiku→Sonnet, Sonnet→Opus) with the two failure transcripts attached. **One** attempt at the escalated tier.
3. On failure at **Opus** (or on the escalated attempt): **halt that branch only.** Write `docs/BLOCKED.md` with: task ID, the three transcripts, the last 200 lines of compiler/test output, the specific acceptance assertion that failed, and the agent's best hypothesis. **Every other parallel branch continues.**
4. **Never weaken an acceptance test to make it pass.** Changing an acceptance criterion requires a Boss commit to PLAN v2, recorded in `docs/HANDOFF.md`. An agent that cannot pass a test halts; it does not renegotiate.
5. **Wall-clock stop-loss per task:** Haiku 30 min, Sonnet 90 min, Opus 180 min *per attempt*. Exceeding it counts as a failed attempt.
6. **Wave gate:** a wave completes when every task in it is PASS or BLOCKED. Boss reviews `docs/BLOCKED.md` between waves and either re-scopes (a new task in the next wave) or accepts the gap into `docs/RESIDUAL.md`. **Boss never pauses for the owner mid-run.**
7. **Halt the whole run** only if: the baseline (`cargo test --features server`) goes red on `main` and cannot be restored in one attempt, or ≥ 3 tasks in one wave are BLOCKED.

### P5.2 Git conventions

- Branch per task: `phase-<N>/<task-id>` (e.g. `phase-1/T1.3`). One task = one branch = one worktree.
- Agents **never** commit to `main` and **never** push to the GitHub remote. **Local branches only.**
- **Boss squash-merges** each branch into `main` after its acceptance output is captured, in wave order, and runs the full baseline after each merge.
- **Boss alone pushes to GitHub**, at wave boundaries. T0.8's CI therefore first runs on the owner's account at Boss's first push — this is expected and is not an agent responsibility.
- Commit message: `<task-id>: <imperative summary>` + a body listing the acceptance assertions that passed.
- No force-push, no history rewriting, no `--no-verify`.

### P5.3 Agent shell preamble (mandatory, every shell, first line)

```powershell
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
$env:RUST_BACKTRACE = "1"
$env:FAMILY_HUB_DATA_DIR = "$env:TEMP\familyhub-test"   # tests only; never the real data dir
```

Rationale: R-04's toolchain finding is closed (BASELINE is green) **except** that `cargo`, `rustc`, `rustup` and `dx` live at `%USERPROFILE%\.cargo\bin` and are not on the system PATH. Every agent shell must prepend it or every `cargo` invocation fails. `dx` 0.6.3 also exists at `~/.cargo/dx06` — **do not use it**; the fallback is deleted (P2b).

### P5.4 Version pins

| Crate / tool | Pin | Trap to avoid |
| --- | --- | --- |
| `dioxus` | **`=0.7.10`** (features `router`, `fullstack`) | Never `0.8.0-alpha.*`; 0.7.2 was breaking within the patch line |
| `dioxus-cli` (`dx`) | **`0.7.10`** — `cargo binstall dioxus-cli@0.7.10` | `@0.7.x` is **invalid binstall syntax**; 0.7.0/0.7.1 have the Windows `--addr 0.0.0.0` bug (#4981) |
| `dioxus-cli-config` | `=0.7.10` | Reads bare `IP`/`PORT`; **removed from the release path** by T0.5 |
| `axum` | `=0.8.9` (via Dioxus `^0.8.4`) | `multipart` already enabled — do not add it again; `DefaultBodyLimit` defaults to **2 MB** |
| `tower-http` | `=0.6.8` | W8: assert no duplicate major in `cargo tree -d` |
| `sqlx` | **`=0.8.6`**, features `sqlite`, `runtime-tokio`, `chrono` | **Drop `tls-native-tls`**; do **not** chase 0.9.0 mid-run; `migrate!` needs `build.rs` `rerun-if-changed=migrations` |
| `rustls` | `=0.23.43`, **`default-features = false`, features `["ring","tls12","logging","std"]`** | Panics if both `ring` and `aws-lc-rs` end up enabled. **Call `CryptoProvider::install_default(rustls::crypto::ring::default_provider())` as the first line of `main()`.** `ring` chosen over `aws-lc-rs` deliberately: aws-lc-rs needs CMake + NASM on Windows, which this box does not have and which would break an autonomous build. |
| `tokio-rustls` | `=0.26.4` + `hyper-util` auto server | **Not `axum-server`** (0.8.0, ~9 months stale, dev-deps still on axum 0.7). **Never `axum-server-dual-protocol`.** |
| `rcgen` | `=0.14.10`, `default-features = false`, features `["ring","pem","x509-parser"]` | Default `not_after` is **year 4096** — set `not_before` **and** `not_after` explicitly. API broke twice since 0.12: `signed_by()` takes `&Issuer`; `CertifiedKey::key_pair` → `signing_key`; `from_ca_cert_pem` moved to `Issuer`. **Training-recalled snippets will not compile.** |
| `instant-acme` | `=0.8.5` (T1.8 only) | Pure Rust, Tokio + rustls, RFC 8555, actively maintained. Default **off**; must make zero network calls in the default path |
| `mdns-sd` | `=0.21.0` | **Exactly one `ServiceDaemon`** (multiple daemons intermittently hang on Windows, #478). No standalone hostname API (#374) — the A record comes from `ServiceInfo::host_name`; use an FQDN **with a trailing dot** and bind the LAN interface explicitly |
| `fast_qr` | `=0.13.1` | Chosen over `qrcode` 0.14.1 (2 years stale, 32 open issues). SVG output |
| `argon2` | **`=0.6.0`** | Two days old at time of writing, after 8 RCs; 0.5.x-era `password-hash` snippets will not compile |
| `rrule` | `=0.14.0` | 16 months without commits; DST behaviour undocumented → **T2.4's DST test is mandatory**. Always `all(limit)`, **never** `all_unchecked()` |
| `icalendar` | **not used in v1** | ICS import cut (R-25). If revived: `features = ["parser"]` — parsing is behind an opt-in feature |
| `chrono` / `chrono-tz` | `0.4` / latest 0.10.x | `.single()` returns `None` on DST-ambiguous times — use `.earliest()` with an explicit fallback |
| `windows-service` | `=0.8.1` | Build the tokio runtime **inside `service_main`**, not `main`; 30 s SCM handshake; stderr goes nowhere |
| `resvg` / `tiny-skia` | latest 0.4x resolved by `cargo add` **on the day, then pinned exactly in `Cargo.toml`** by T0.7 | Used only by `xtask` for icon generation — never in the shipped binary |
| `uuid` | `=1.x` (`v4`) | `ClientId` |
| `image` | `=0.25.x` | Server-side re-encode; do not enable formats you do not allow |
| `tokio-tungstenite` | `=0.28.x` (**dev-dependency**) | The Rust WS test client for T1.2/T2.6 — replaces any headless-browser need (G-9) |
| Tailwind CSS | **standalone `tailwindcss-windows-x64` v3.4.17** (no Node) | Not the npm install currently on this box; CI rebuilds and fails on diff (W9) |
| `adb` | 1.0.41 (scoop) | Declared in `NON_RUST.md`; used only by T0.0 and Appendix A |

### P5.5 The 26 stated defaults — final values

| # | White's proposal | **Purple's final value** |
| --- | --- | --- |
| 1 | Device ID; halt on Vega | **MODIFIED.** T0.0 probes and classifies; **three branches always published; the run never halts.** Vega is a runbook branch (Silk), not a blocker. |
| 2 | PATH prefix | **ADOPT.** P5.3, mandatory first line of every agent shell. |
| 3 | `dx` 0.7.10 vs crate 0.6.3 | **MODIFIED.** Migrate; keep `dx` 0.7.10; **delete the 0.6.3 fallback entirely.** `~/.cargo/dx06` is not used. |
| 4 | `=0.7.10` everywhere | **ADOPT.** |
| 5 | Bind config | **ADOPT + extend.** `FAMILY_HUB_ADDR` default `0.0.0.0:8080`; `FAMILY_HUB_TLS_ADDR` default `0.0.0.0:8443`; `fullstack_address_or_localhost()` removed from the release path. |
| 6 | Ports 8080/8443 | **MODIFIED.** 8080 HTTP **serves the TV in full** (not a blanket redirect); it 308s only `/m*`, `/manifest.webmanifest` and `/sw.js`. 8443 HTTPS serves everything. |
| 7 | PKI: CA 10 y, leaf 397 d, `%ProgramData%\FamilyHub\pki`, SANs, `install_default()` | **ADOPT with two changes.** Leaf **397 days** confirmed (inside Apple's 398 *and* 825 caps — no need to go near 824). **`ring`, not `aws-lc-rs`** (Windows build safety). Re-issue at **30 days remaining** with hot reload, not just on IP change. CA key ACL-restricted and **excluded from backups**. |
| 8 | `tokio-rustls`, not `axum-server` | **ADOPT.** |
| 9 | PIN bootstrap, 4-digit implied | **MODIFIED.** **6 digits** (R-23d — 100× keyspace at zero UX cost). No PIN on first run; setup code written to the log, to `<data>\setup-code.txt`, **and shown on the TV**. Server-side enforcement only. Exponential backoff, **no hard lockout** (a lockout on a wall display is a self-inflicted outage). |
| 10 | Migration baselining + `build.rs` | **ADOPT.** |
| 11 | Replace the CHECK test with an FK test | **ADOPT**, written verbatim into T1.4. |
| 12 | `ProfilesUpdated` | **ADOPT.** In the P2c message list, owned by T1.2. |
| 13 | `rrule 0.14`, store `dtstart`+TZID+RRULE, dedicated DST test | **ADOPT.** Local events only; Google events are stored expanded. |
| 14 | Week starts Sunday; server-local time | **ADOPT.** All surfaces display **server-local** time, never device-local. |
| 15 | One board; retain 5,000 strokes | **MODIFIED.** One board (`board_id=1`) — adopted, and "multiple named boards" is **cut**. Retention **2,000 strokes** since `cleared_at` (one row per **stroke**, not per segment — R-18a — so 2,000 strokes is far more content than 5,000 segments). Hard-delete before `cleared_at` on the midnight tick. |
| 16 | `ClientId` origin for echo, assigned to 1.1 | **ADOPT**, assigned to **T1.2**, with the addition that the **server** mints the id so it cannot be spoofed. |
| 17 | 3 CC0 placeholders + a phone upload route | **ADOPT.** T0.7 + T2.7. |
| 18 | PWA icons via `resvg`/`tiny-skia`, Haiku | **ADOPT.** T0.7, as a Rust `xtask` (no Node, no ImageMagick). |
| 19 | Photo retention 30 days | **ADOPT**, plus an explicit user-initiated delete (R-18b) and deletion of the file with the task. |
| 20 | Screensaver schedule off by default; idle 10 min | **ADOPT.** |
| 21 | `phase-<N>/<task-id>`; agents never push `main`; Boss squash-merges | **ADOPT + decide the open question:** **agents never push to the GitHub remote at all.** Local branches only; **Boss pushes** at wave boundaries. |
| 22 | 2 attempts, escalate a tier, halt branch + `BLOCKED.md` | **ADOPT + extend** — full policy in P5.1, including the wall-clock stop-loss, the wave gate, the whole-run halt condition, and the "never weaken an acceptance test" rule. |
| 23 | Tailwind standalone 3.4.17; CSS committed; CI fails on diff | **ADOPT.** |
| 24 | Assume no Google service account; every Google gate needs an offline fixture | **ADOPT.** T2.4's poll test is fixture-driven. |
| 25 | Drop `tls-native-tls`; consider sqlx 0.9 | **ADOPT the drop; REJECT the bump.** Pin `=0.8.6` for the run; 0.9.0 (new repo org) is a Phase 4 consideration. |
| 26 | Rust WS-client reconnect timing test | **ADOPT**, as P2c assertion 7, using `tokio-tungstenite` as a dev-dependency. |

**Additional defaults Purple adds** (not in White §H):

| # | Default |
| --- | --- |
| 27 | **TV origin is HTTP; phones are HTTPS.** The TV never receives a certificate, a service worker or an install prompt. Written into PLAN v2 §2 as decision D3′. |
| 28 | **WS throttle:** client ≤ 30 msg/s (one flush per animation frame); server token bucket 40/s burst 80; broadcast capacity 1024; per-connection outbound queue 256 drop-oldest. |
| 29 | **Body limits:** global `DefaultBodyLimit` stays at axum's 2 MB; the upload route alone is raised to **25 MiB**. |
| 30 | **Backups:** `<data>\backups\`, nightly at the midnight tick, **14 retained**, `VACUUM INTO` only, uploads tar'd alongside, **PKI excluded**. |
| 31 | **Session:** parent session token, 30-day expiry, `HttpOnly` + `Secure` on the HTTPS origin, `SameSite=Lax`. TV origin holds no session and cannot obtain one. |
| 32 | **Staleness threshold:** 90 s without a server message → disconnected badge on the TV. |
| 33 | **Log level:** `info` in the service, `debug` behind `FAMILY_HUB_LOG`. Rotation at 10 MB × 5 files. |
| 34 | **Kiosk URL of record:** `http://<dhcp-reserved-ip>:8080/tv`. The DHCP reservation is an **owner prerequisite** (Appendix A), and the leaf is re-issued automatically if the IP ever changes anyway. |
| 35 | **Scope line (R-12), stated in the product doc:** *routine completion, profile switching, panel navigation, screensaver dismiss and the QR overlay are fully operable from the TV remote. Drawing, photo capture, calendar editing and all administration are phone-only.* |

---

## Appendix A — Owner Verification Checklist (outside the autonomous run)

Everything here needs the owner, a phone, the TV, or a power cycle. **No task's completion depends on any of it.** Delivered as `docs/OWNER_CHECKLIST.md` by T3.2.

| # | Step | Pass criterion |
| --- | --- | --- |
| A1 | **Read the TV's identity:** Settings → My Fire TV → About. Record model and OS. | `docs/FIRE_TV.md` STATUS line matches what you see; if T0.0 said UNKNOWN, pick your branch now |
| A2 | **DHCP reservation** for the server PC on the router | The PC keeps the same IP after a router reboot |
| A3 | **Reboot the PC.** | The hub is reachable at `http://<ip>:8080/health` with nobody logged in |
| A4 | **Branch A only:** sideload Fully Kiosk ≥ 1.61.2, buy PLUS, grant `SYSTEM_ALERT_WINDOW` + `GET_USAGE_STATS` over adb, `settings put secure sleep_timeout 0`, screensaver → Never, disable HDMI-CEC on the television | **Three consecutive TV reboots** each land on the kiosk with no remote interaction |
| A4′ | **Branch B only:** open Silk, bookmark `http://<ip>:8080/tv`, set it as the start page | "Alexa, open Silk" reaches the kiosk in one step; you accept a manual relaunch after a power cut |
| A5 | **Navigate the whole TV UI with the real remote.** Use `?keys=1` to capture actual key codes and report any that differ from the shipped map | A child can complete a full routine using only the remote |
| A6 | **Install the CA on each phone** from `http://<ip>:8080/ca.crt`. **iOS additionally:** Settings → General → About → Certificate Trust Settings → enable full trust | `https://<ip>:8443/m` shows a padlock; Add to Home Screen offers install |
| A7 | **Install the PWA** on one Android and one iOS phone | It opens standalone, no browser chrome |
| A8 | **Airplane-mode test:** open the PWA offline, toggle a routine item, re-enable network | The toggle appears on the TV within 10 s of reconnect, **dated to when you made it** |
| A9 | **Upload a real 12 MP photo** from a phone camera | Under 3 s on 5 GHz; stored file ≤ 400 KB; appears on the TV |
| A10 | **Drop real screensaver photos** into `<data>\screensaver\` (or upload from the phone) | They appear in rotation after 10 min idle |
| A11 | **Pull the network cable for 5 minutes**, then restore | The TV shows the disconnected badge, then recovers within 30 s with no reload |
| A12 | *(optional)* **Enable DNS-01:** buy a domain, add a DNS API token, set `certs.mode = "acme_dns01"` | Phones trust the cert with **no CA installed**; you may then remove the CA from every phone |

---

## P6. Residual risks

| # | Risk | Why it survives every change above | Trigger that would surface it |
| --- | --- | --- | --- |
| **RR-1** | **Nobody has run the app.** BASELINE covers `cargo test`/`clippy`; `dx serve` has never been executed and no page has ever been rendered. T0.3's harness reduces this to "SSR + WS work"; it does not prove the wasm bundle hydrates. | An acceptance test can assert an HTTP body and a WS frame; it cannot assert that a browser hydrated the app. `dx build --platform web --release` proves compilation, not runtime. | The first time the owner opens `/tv` in a real browser (A5). Mitigation: T3.3's transcript makes it obvious this class was never covered. |
| **RR-2** | **Fully Kiosk's boot-launch on Fire OS is unreliable, and an Amazon OTA can break the launcher override at any time.** | The vendor says so; nothing in a Rust codebase can fix it. | Any Fire OS update, or the first power cut (A4). Mitigation: Branch B′ (a £35 Google TV box) is priced and documented; the URL is unchanged, so switching costs a bookmark. |
| **RR-3** | **Vega OS Silk has no launch-on-boot.** If the device turns out to be Vega and the owner declines B′, the TV needs a manual relaunch after every power cut. | Amazon removed sideloading; no software fix exists. | A power cut. Mitigation: `docs/RECOVERY.md` step 1 is "Alexa, open Silk". |
| **RR-4** | **iOS may still refuse the private root** in some configuration (MDM profile, Lockdown Mode, a future iOS tightening), leaving iPhone users without the installable PWA. | Apple's trust behaviour is documented but has tightened repeatedly; we cannot test it here. | Step A6 on an iPhone. Mitigation: T1.8 exists precisely for this — flip one config key and the problem disappears. |
| **RR-5** | **`rrule 0.14` has been dormant for ~16 months and documents no DST guarantee.** T2.4 tests two named boundaries; a third timezone or an exotic rule could still be wrong. | We are testing the cases we can name, against a crate with 33 open issues and no maintainer activity. | A recurring event landing on an untested DST edge, most likely a European autumn transition. Mitigation: recurrence is used only for **local** events, which are few and owner-authored. |
| **RR-6** | **The offline mutation queue is best-effort on iOS.** Safari has no Background Sync, so a queued toggle replays only when someone next opens the app. | Platform limitation. | A parent toggling offline on an iPhone and not reopening the app for a day. Mitigation: stated explicitly in `docs/PWA.md`; the mutation carries its intended date, so a late replay is still *correct*, just late. |
| **RR-7** | **`mdns-sd` on Windows has open issues** (#374 no hostname API, #459 multi-interface, #478 multi-daemon hang). We use exactly one daemon and treat `.local` as a convenience, but a multi-homed host (VPN adapter, Hyper-V switch, Docker bridge) may advertise the wrong interface. | The crate has no stable answer; the workaround is explicit interface binding, which itself requires guessing the LAN interface. | The QR resolving to a Hyper-V or VPN address. Mitigation: the QR encodes the **raw IP**, chosen from the interface with the default route, and `/health` reports which interface was chosen. |
| **RR-8** | **No load has ever been generated by real clients.** T1.2's load test uses Rust WS clients with synthetic strokes; real touch input from a child's finger has different burst characteristics. | Synthetic load approximates but does not equal a real pointer stream. | A long scribbling session with several children (A5/A9 era). Mitigation: the design now degrades by *resyncing*, not by closing — the worst case is a visible flicker, not a dead kiosk. |
| **RR-9** | **The Windows PC is a single point of failure with no monitoring beyond `/health`.** Nothing alerts anyone; the TV badge only tells whoever is looking at the TV. | Alerting requires a channel (email, push) that is either cloud (violates local-first) or another daemon. | A disk filling, a Windows Update reboot loop, or a failed backup. Mitigation: `/health` exposes disk free and last-backup time; the TV badge covers the observable case; backups are verified by a restore drill in CI. |
| **RR-10** | **The `sw.js` exception is small but real, and Tailwind still needs a downloaded binary.** The project is Rust "whole stack" with four declared exceptions plus a browser. | Genuine platform constraints; no Rust equivalent exists for either. | Any future audit of the Rust-purity claim. Mitigation: `docs/NON_RUST.md` is a pre-filled ledger, not a tripwire — the exceptions are now counted, priced and justified rather than discovered. |
| **RR-11** | **T0.4 (the Dioxus migration) is the single serial choke point.** If it BLOCKs, the entire run stops — every Phase 1 and 2 task depends on it transitively. | The migration cannot be parallelised and cannot be skipped without forfeiting multipart, and a version fork would double every downstream task. | Break #7 (silent form-submit inversion) or an undocumented break not in the list of 11. Mitigation: T0.3's harness exists solely to catch this; the acceptance test names each break explicitly so a failure identifies *which* one. |
| **RR-12** | **Design (G19) is unresolved and stays unresolved.** T3.4 polishes the existing palette to objective criteria; it cannot produce the look the owner has in mind. | The inspiration images do not exist. Waiting for them would be a human checkpoint. | The owner seeing the finished hub. Mitigation: this is a deliberate, stated trade — Phase 4 exists for it, and T3.4 leaves a clean, accessible base rather than a half-designed one. |

---

*Purple Team resolution complete. Every Accept in P1 maps to a task ID in P3, a default in P5.5, or a pin in P5.4. Fable: apply P2 to §2 (D2/D3/D7 rewritten as D2′/D3′/D7′), P3 to §3, P4 to §4, P5 to §4 and a new §8, and P6 to §7 of PLAN v2. Appendix A becomes `docs/OWNER_CHECKLIST.md`, delivered by T3.2 and executed by the owner after the run.*

**Sources consulted for facts neither review settled:** [instant-acme on crates.io](https://crates.io/crates/instant-acme) · [instant-acme docs](https://docs.rs/instant-acme) · [Amazon launches Vega OS for Fire TV — AFTVnews](https://www.aftvnews.com/amazon-launches-vega-os-for-fire-tv-heres-how-it-affects-new-old-fire-tvs-apps-and-sideloading/) · [Silk Web Browser on Fire TV — Amazon](https://www.amazon.com/gp/help/customer/display.html?nodeId=TBhICALzPJBwmVvfvk) · [What is Amazon Silk — AWS](https://docs.aws.amazon.com/silk/latest/developerguide/what-is-silk.html) · [Fire TV / Android CA trust behaviour — HTTP Toolkit](https://httptoolkit.com/blog/android-11-trust-ca-certificates/) · [Network Proxy on Fire TV — Amazon Developer](https://developer.amazon.com/docs/fire-tv/network-proxy.html)
