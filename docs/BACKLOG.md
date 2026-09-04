# Backlog — owner-queued items outside the current run

Items the owner asked to queue. Each becomes a planned task (plan → review → approval) when picked
up; nothing here is executed by the autonomous run in progress.

## B-1 — The kiosk renders too large on the Insignia Fire TV: rail, routine text and footer clipped (owner, 2026-09-02)

**Evidence:** `docs/design/current-state/tv-routine-clipped-insignia-2026-09-02.jpg` — photo of the
television. Only Isaiah's rail card is visible (the second card is cut off by the footer pills), the
first routine row's text ("Wake up and thank…") overflows the poster card's right edge and is clipped
at the bottom, the progress bar and `0 / 8` chip sit on the card's edge. Compare
`tv-after-d4.3.jpg`, the same panel at a true 1920×1080 CSS viewport, where all four boys, eight rows
and the footer fit.

**Hypothesis:** the layout is designed and tested at `TV_RENDER_WIDTH_PX × TV_RENDER_HEIGHT_PX`
(1920×1080 CSS px, `src/client/components/tv/style.rs`), but the Fire OS WebView inside Fully Kiosk
reports a smaller CSS viewport (typically 960×540 at device-pixel-ratio 2, or Fully Kiosk's
"web content scale" is not 100 %), so every `px`-sized element paints at twice the intended size and
the card overflows. `docs/FIRE_TV.md` says nothing about viewport scale today.

**VERIFIED 2026-09-03 (Boss, live over adb — the hypothesis above is confirmed, and its
Fully Kiosk clause is wrong).** No JavaScript console was needed; the device settled it:

* `adb shell wm size` → physical `3840x2160`, **override `1920x1080`**; `adb shell wm density` → **320**
  (= 2.0 dppx). So the WebView lays out at ~**960 CSS px** wide against a layout built for
  `TV_RENDER_WIDTH_PX` = 1920 CSS px — every `px` element paints at 2×.
* `src/client/app.rs:80-81` sets **one** viewport meta for *every* route, `/tv` included:
  `width=device-width, initial-scale=1, viewport-fit=cover, user-scalable=no`. On this panel
  `device-width` resolves to ~960 px. That single line is the whole defect.
* Reproduced live: `docs/design/current-state/tv-clipped-fullykiosk-adb-2026-09-03.png` — the hub on
  the television, clipped exactly as the owner's photo (one rail card, "Wake up and thank…" cut at the
  right edge, the `0 /` chip severed).
* **Fully Kiosk is not the cause and never was.** It was not installed when the owner took the B-1
  photo (only Silk and the Chromium WebView were on the device), so the "web content scale is not
  100 %" clause is dead. Independent proof: `tv-fullykiosk-welcome-clipped-2026-09-03.png` — Fully's
  **own** welcome page is clipped the same way in the same WebView.

So fix candidate 1 below is the right one, and it needs no device configuration.

**Fix candidates (pick one, prefer the first that works without device configuration):**
1. App-side, one line: serve `/tv` with `<meta name="viewport" content="width=1920">` so the WebView
   lays out at 1920 CSS px and scales to the panel — the standard TV-WebView fix; the phone routes
   keep `width=device-width`.
2. App-side, adaptive: on mount read `innerWidth`, set `zoom: innerWidth / 1920` (or a `transform:
   scale()` on the root with `transform-origin: top left`) and re-run on `resize`.
3. Device-side: Fully Kiosk **Web Content Settings → Initial scale / Desktop mode**, documented in
   `docs/FIRE_TV.md` Branch A as a numbered step.

**Acceptance (agent-executable):** a headless Chrome run at a 960×540 viewport with DPR 2 (browser
automation, or a `#[test]` computing the layout budget for that viewport the way `tv_rail_budget_px`
does) shows all four rail cards, the full first routine row, and the footer pills inside the poster
card; the 1920×1080 golden files stay unchanged; `docs/FIRE_TV.md` records the measured viewport.

**Also seen in the photo (not defects):** the wordmark, sun glyphs, corner balls and focus ring render
as designed.

**DONE (TV1–TV3, 2026-09-03).** `docs/design/PLAN_TV_VIEWPORT.md` is the approved plan; fix candidate
1 above is what shipped, exactly as the VERIFIED block predicted.

* **Mechanism:** `/tv` now emits its own `document::Meta { name: "viewport", content: TV_VIEWPORT_META
  }` (`TV_VIEWPORT_META = "width=1920, user-scalable=no"`, `src/client/components/tv/style.rs`) as the
  first node of `KioskDashboard`, and the old single global viewport meta on `App` is deleted. The
  phone routes (`/m`, `/mobile`) render their own meta from `Mobile`, byte-identical to the string the
  whole app used to share (`width=device-width, initial-scale=1, viewport-fit=cover,
  user-scalable=no`), so the phone surface is untouched. With no `initial-scale`, Chromium computes
  the scale that fits 1920 CSS px into whatever panel it is given — 0.5 on this TV's 960-wide WebView,
  1 on a true 1920 panel — so the layout viewport is 1920 × 1080 CSS px everywhere, which is the
  precondition the rail budget and every golden file already assumed.
* **Files:** `src/client/app.rs`, `src/client/components/tv/style.rs` (TV1); `src/client/components/tv/{model,surface,shell,fixture}.rs` (TV2, adds the `?keys=1` viewport readout); `docs/FIRE_TV.md`,
  `docs/BACKLOG.md`, `docs/design/DESIGN_DIRECTION.md` (TV3, this entry).
* **Tests that pin it:** `tests/router_tests.rs` asserts `GET /tv` carries exactly one viewport meta
  equal to `TV_VIEWPORT_META` and neither `width=device-width` nor `initial-scale`, while `GET /m` and
  `GET /mobile` keep `width=device-width, initial-scale=1, viewport-fit=cover` and never see
  `width=1920`; a unit test on `TV_VIEWPORT_META` itself (`style.rs`) pins the constant against
  `TV_RENDER_WIDTH_PX`; `tests/tv_tests.rs` pins the `?keys=1` readout's text against a measured
  viewport and against `None`; every pre-existing `/tv` acceptance test (`t2_1_*`, `d4_3_*`, `qd_02_*`,
  `qd_08_*`, `hs6_*`) and the golden files stay green and byte-identical; `cargo test --features server
  --test docs_tests` link-checks this file and `docs/FIRE_TV.md`.
* **The 960×540 acceptance sentence, honestly.** B-1's original acceptance line above asks for "a
  headless Chrome run at 960×540 viewport with DPR 2 … or a `#[test]` computing the layout budget for
  that viewport." With the meta in place there is no 960-px layout left to budget for — the layout
  viewport is 1920 on every device the meta reaches — so that sentence is satisfied differently than
  written: (a) the SSR assertion that `/tv` pins `width=1920`, and (b) the on-device readout below,
  not a headless run at 960×540. `docs/design/PLAN_TV_VIEWPORT.md` §1.5's last paragraph records this
  same substitution and is the source for this note.
* On-device verification per `docs/design/PLAN_TV_VIEWPORT.md` §4, recorded by the Boss at merge.

## B-2 — Heads-up before a parent phone's sign-in lapses (owner, 2026-09-03)

**Ask:** the parent session cookie lasts 30 days (T1.4). When it lapses the phone simply shows the
PIN box again on the next parent-only action, which is fine, but the owner would like a heads-up a
few days before rather than a surprise at 7am.

**Shape:** `/api/session` already answers the probe; extend its response with `expires_at`
(RFC3339). The phone shell, on each probe, renders a dismissible chip in Settings and a one-line
banner on the Routine/School header when `expires_at − now ≤ 3 days`: "Your parent sign-in ends on
Sat — tap to renew". Tapping renew re-prompts the PIN and mints a fresh 30-day cookie. No push
notifications (the PWA has none today, and none are wanted for this).

**Acceptance:** a probe response with `expires_at` 2 days out renders the chip; 10 days out does not;
renewing replaces the cookie (new `expires_at` ≥ 29 days out); `docs/PWA.md` states the behaviour.

## B-3 — Agents' tests and tools must never open the real data directory (Boss, 2026-09-03) — **DONE (HS9, 2026-09-03)**

**What happened:** during the homeschool run an agent process applied migration 0005 to the
production database under `%ProgramData%\FamilyHub`, seeded the synthetic fixture curriculum and a
junk enrollment there, set a throwaway parent PIN and blanked the setup code — all at 22:17 on
2026-09-02. Root cause: a command ran with `FAMILY_HUB_DATA_DIR` unset, and the config falls back to
the real service directory. Recovery took a database snapshot, manual row deletes, and the
RECOVERY.md failure-mode-7 PIN reset.

**Fix:** (1) every test harness (`init_test_env` in each suite, `tests/homeschool_*`,
`tests/health_*`) sets `FAMILY_HUB_DATA_DIR` itself to a pid-keyed temp dir before anything reads
config; (2) `FamilyHubConfig` refuses to resolve to `%ProgramData%\FamilyHub` when compiled with
`cfg(test)` or when an env `FAMILY_HUB_REFUSE_SYSTEM_DIR=1` is set, which the workflow preamble
exports; (3) `import-curriculum` prints the resolved data dir and requires `--yes` when it resolves to
the system directory; (4) `docs/DEV_WINDOWS.md` and the workflow preamble say so.

**Acceptance:** a test binary run with the env var unset still writes nothing outside `%TEMP%`
(assert via a canary file in the real dir before/after); `import-curriculum` without `--yes` against
the system dir exits non-zero.

**Delivered (HS9, branch `hs/HS9`, 2026-09-03):** all four parts.
(1) Every `init_test_env` harness in `tests/` now sets `FAMILY_HUB_DATA_DIR` to a pid-keyed
`%TEMP%` directory itself — `font_tests`, `health_pool_closed_tests`, `homeschool_db_tests`,
`http_tests`, `pwa_tests`, `router_tests`, `routine_tests`, `tls_tests`, `service_tests` and
`config_tests` gained one; the other suites already had it. `whiteboard_tests`, `realtime_tests`,
`loop_tests` and `homeschool_loop_tests` had one but called it too late — every test there runs
`realtime::reset_board()`, which opens the process-wide pool, *before* `spawn_hub()` did the
pinning, so those suites had genuinely been drawing on and compacting the family's real whiteboard
(`docs/HANDOFF.md` H-HS9-1); the call moved into `hub_lock()`, the first line of every test. The unit test
`every_integration_test_suite_sets_the_data_dir_itself` in `src/server/config.rs` re-audits the
whole directory so a new suite cannot drop the line.
(2) `FamilyHubConfig::from_sources` returns `Result<Self, ConfigError>`: resolving to
`%ProgramData%\FamilyHub` is `ConfigError::SystemDataDirRefused` under `cfg(test)` or
`FAMILY_HUB_REFUSE_SYSTEM_DIR=1`. `load()` panics with that message, `try_load()` hands it back;
`family-hub.exe run` and `import-curriculum` use `try_load` and exit 1 with one line.
(3) `import-curriculum` prints `data directory: <path>` on every run and the loader refuses the
system directory without `--yes`, before it reads the file or opens a pool.
(4) `docs/DEV_WINDOWS.md` "Never develop against the live data directory" and `docs/PLAN.md` §5.7.
Verified: `cargo test --features server` with `FAMILY_HUB_DATA_DIR` unset left a canary file in
`C:\ProgramData\FamilyHub` byte-identical and the directory listing unchanged.

## B-4 — The phone's Remote tab can show a panel but cannot navigate inside it (owner, 2026-09-03)

**What the owner hit:** with the kiosk on the Routine panel, driven from the phone's **Remote**
tab, there is no way to scroll the routine list from the phone. Their words: *"I like the way this
looks but I don't have the ability to scroll down on the Routine."* Reaching the rows below the
fold means walking to the television and using the Fire TV remote.

**What ships today:** `src/client/components/mobile/remote.rs` offers exactly two controls —
*Show on the TV* (`SetView`: Dashboard · Routine · Calendar · Whiteboard · School) and
*Whose routine* (`SetActiveProfile`). Both are one-shot state changes. There is **no directional
input of any kind**, and the protocol has nowhere to put one: `ClientMessage`
(`src/shared/types.rs:218`) is `Hello`, `Ping`, `Draw`, `ClearBoard`, `SetView`,
`SetActiveProfile`, `RequestSnapshot` — no navigation variant exists. So this is a missing
capability, not a defect: the Remote tab is a *channel changer*, and the owner expected a
*remote control*.

**Why it matters:** D1 makes the phone the parent's way to drive the television without crossing
the room, and the TV is D-pad-only by design (no touch, no pointer). A panel the phone can open
but not operate is half a remote — and the routine list is precisely the panel with more rows
than fit.

**Not to be confused with:** the boy-switching complaint in the same report, which is **working as
designed** — `SetActiveProfile` is ignored by the hub unless the phone holds a parent session (or
asks for its own profile, R-23b). The tab already says so: *"Sign in with the parent PIN under
Settings to control the TV."* Worth checking the owner is signed in before treating that half as a
bug.

**Shape of the work (not yet planned):**
1. A new `ClientMessage` variant — a directional/activate message. This is a **normative protocol
   change**: `docs/PROTOCOL.md` §3 is the contract and `tests/realtime_tests.rs` holds a
   compile-time exhaustive match that fails until every variant is documented there.
2. Server: authorise it exactly as `SetView` / `SetActiveProfile` are authorised (§P2c — parent
   session), then mint the fan-out `ServerMessage`; the client's bytes are never forwarded.
3. Kiosk: apply it to the existing focus/scroll model rather than inventing a second one, so
   phone-driven and remote-driven navigation cannot diverge.
4. Phone: a D-pad on the Remote tab, thumb-sized per the 44 px minimum.

**Open design questions for the plan:** does the phone move *focus* (a true remote) or *scroll*
(a simpler scrollbar)? What happens when two phones press at once? The `/ws` token bucket is
40 msg/s with burst 80 (`docs/PROTOCOL.md`) — a held key must not trip it, and the whiteboard's
`pointermove` flood (G20) is the cautionary tale. Should the boys' phones be able to do this at
all, or parents only?

