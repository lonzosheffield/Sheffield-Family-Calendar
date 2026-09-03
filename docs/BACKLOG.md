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
