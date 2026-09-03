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

**Verify first:** on the TV open `/tv?keys=1` (or Fully Kiosk's remote admin → JavaScript console) and
read `window.innerWidth`, `window.innerHeight`, `devicePixelRatio`; record them in `docs/FIRE_TV.md`.

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

## B-3 — Agents' tests and tools must never open the real data directory (Boss, 2026-09-03)

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
