VERDICT: PASS

# QA design round 2 — Phase 4 (P4.1) fixes, commits `54bc715..41600f7`

**Auditor:** Fable 5, independent design + code QA (fresh context, no memory of round 1 beyond its report) · **Date:** 2026-08-30 · **Branch:** `main` @ `41600f7`
**Brief:** `docs/design/inspiration/sheffield-morning-routine-poster.jpg` · `docs/design/DESIGN_DIRECTION.md` §2–§5 · `docs/qa/QA_DESIGN_ROUND_1.md` (both Med findings re-verified in code *and* in a browser, not taken as claimed) · `docs/design/qa/*.jpg` · full read of `git diff 54bc715..41600f7`; spot-read of `7d6644d..41600f7` in `tv/surface.rs`, `tv/shell.rs`, `screensaver.rs`, `router.rs`
**Counts:** 0 Critical · 0 High · **0 Med** · 2 new Low (§6) + 7 round-1 Lows still open (§4). PASS requires zero Critical/High/Med → **PASS**.

Both round-1 Meds are genuinely fixed: QD-01 by embedding the faces in the binary (proved by a request from a process whose CWD is a scratch directory with no `assets/` under it), QD-02 by reclaiming 32 px of card spacing and 64 px of rail spacing so that four boys plus *Add a phone* cost 600 px of the 612 px the rail gets at 1920×1080 — measured live at a true 1920×1080 CSS viewport, not computed.

---

## 1. Gates (this PC, foreground, one cargo process at a time)

| Gate | Result |
| --- | --- |
| `cargo fmt --check` | clean (exit 0) |
| `cargo clippy --features server --all-targets -- -D warnings` | clean, 0 warnings |
| `cargo clippy --features web --target wasm32-unknown-unknown -- -D warnings` | clean, 0 warnings |
| `cargo test --features server` | **436 passed, 0 failed, 2 ignored** (1 pre-existing in `storage_tests`, 1 doc-test) across 28 binaries; `font_tests` 9/9 (was 7), `tv_tests` 30/30 (was 27), `tv::style` unit tests +2, `glyph_tests` 6/6, `palette_tests` 6/6 |
| `assets/tailwind.css` vs fresh `tailwindcss.exe -c ./tailwind.config.js -i ./input.css -o <tmp> --minify` | SHA-256 identical `5F3F5C14…B65D8B`, 20 051 bytes both |
| `git diff 7d6644d..HEAD --stat -- tests/golden/` | empty; blob ids `1a3af5d…` (`tv_focus_order.txt`) and `4f4a4fc…` (`tv_type_scale.txt`) identical at `7d6644d` and `41600f7` |
| Zero external requests | `assets/tailwind.css`: the only `https://` is the Tailwind licence comment; `src/client/components/mobile/sw.js`: no `http`; `src/client/components/**`: only the join-URL format/fixtures (`https://{host}:{port}/m`); no `googleapis`/`gstatic` anywhere. Live: `performance.getEntriesByType('resource')` inside `/tv` lists exactly one origin, `http://127.0.0.1:18099`; the three woff2 requests are `/fonts/*` on it |
| `/fonts` from a foreign CWD | Server started with `-WorkingDirectory <scratchpad>\srv-cwd` (no `assets/` beneath it), `DIOXUS_PUBLIC_PATH` absolute. All three: `200`, `content-type: font/woff2`, `cache-control: public, max-age=31536000, immutable`, bodies begin `wOF2`, 16 556 / 16 520 / 18 632 bytes; `/fonts/nope.woff2` → 404 |
| `cargo build --features server` · `dx build --platform web --release` | both green (binary 21:15, dx bundle 21:16 — fresh, not the stale 04:01 bundle) |
| Server run (`127.0.0.1:18099`, TLS `:18462`, fresh data dir) | `/health` 200; `logs/familyhub.log` 0 WARN/ERROR lines; stdout/stderr empty |
| `/tailwind.css` actually served | 20 051 bytes = the committed file (`include_str!`), not the older `tailwind-dxh…css` copy dx leaves in `public/assets` (18 998 B, 04:01) — that copy is unreferenced |

---

## 2. Round-1 Med findings — status with measured evidence

Method for QD-02: this PC's display is 125 % DPR, so a 1920×1080 window is a 1536×826 CSS viewport. To look at the kiosk's real target I replaced the page with a same-origin harness (`<iframe src="/tv">` at 1920×1080 CSS px, `transform: scale(0.76)`), confirmed `innerWidth/innerHeight` inside the frame = 1920×1080, and took every number below from `getBoundingClientRect()` inside that frame (unscaled, true CSS px). The screenshots in §7 are that harness.

| id | Status | Evidence |
| --- | --- | --- |
| **QD-01** fonts served from the build machine's source tree | **FIXED** (`7befe1e`) | `src/server/router.rs`: `fonts_router()` is three `get` routes returning `include_bytes!("../../assets/fonts/…")` with `CONTENT_TYPE: font/woff2` + `CACHE_CONTROL: public, max-age=31536000, immutable`; `ServeDir`/`FONTS_DIR` gone. Tests: `fonts_route_is_served_even_when_the_process_cwd_is_not_the_repo` really `set_current_dir`s to a temp dir (with a `Drop` guard) before booting the router; `fonts_route_no_longer_serves_from_a_build_machine_path` greps the function body for `ServeDir`; the existing route test now asserts the `Cache-Control` value. Live proof in §1 (server CWD = scratch dir). `document.fonts` in Chrome: `Nunito 400 600 loaded`, `Nunito 700 900 loaded`, `Baloo 2 700 900 loaded`. QD-06 (no Cache-Control) closed by the same change. |
| **QD-02** rail clipped Boy 4 and hid *Add a phone* at 1080p | **FIXED** (`41600f7`) | Live at 1920×1080: card 96→984; header 132→235.2 (103.2 px — the budget's 104 is correctly pessimistic); **rail 259.2→872 = 612.8 px** (budget says 612); profile buttons 112 px each at 259.2/391.2/523.2/655.2, **Boy 4 bottom 767.2**; **Add a phone 800→872, one line, `📱 Add a phone`**, fully inside; rail `scrollHeight 613 == clientHeight 613` (nothing to scroll, no scrollbar at 1080p). Sum: 4×112 + 4×20 + 72 = 600 ≤ 612, exactly `tv_rail_needed_px(4)`/`tv_rail_budget_px()`. `TV_POSTER_CARD_CLASS` is `p-8 gap-6`, rail `overflow-y-auto`, buttons `py-4`, disc `h-20 w-20`, pill `py-4`; the class↔arithmetic pinning tests (`the_budget_is_made_of_the_classes_it_is_made_of`, `qd_02_the_poster_card_and_the_rail_wear_the_measured_spacing`) walk the rendered markup for every token. Routine rows still `px-8 py-6` (the direction's rule, and the type scale golden is untouched). Whiteboard mirror recovered too: canvas 1200×532 CSS px at 1080p. |
| **QD-08** (Low, closed by QD-02's fix) virtual focus never scrolled into view | **FIXED** (`41600f7`) | `nav::scroll_target` (pure) + `shell.rs` `scroll_to` signal → `use_effect` → `platform::scroll_into_view` with `block: nearest, inline: nearest` after the frame renders. Live walk, Enter then ArrowDown ×7 at 1080p, list 339.2→872: row 1 [339.2, 467.2], row 2 [487.2, 615.2], row 3 [635.2, 799.2], rows 4–8 land at [744, 872] (row 7 [708, 872]) with `scrollTop` 75.2 → 223.2 → 371.2 → 555.2 → 703.2 — **every focused row fully inside the list on every press**. ArrowUp back to row 1 also scrolls (row 1 in view at 8/8). Rail walk Down ×4 lands `tv-join-qr` at [800, 872] inside the rail. `qd_08_every_focus_move_asks_to_be_scrolled_into_view` covers the decision side; a no-op press (Backspace on the rail) yields `None`, so no twitch. |

---

## 3. Other states looked at (1080p harness)

| State | Result |
| --- | --- |
| Stamp on a checked row | Enter on row 8: box classes `bg-sheffield-dark text-white stamp-check`, glyph `✓`, computed transform `matrix(1.0574, −0.0739, 0.0739, 1.0574, 0, 0)` = rotate(−4°) scale(1.06), `transition-duration 0.15s` under `prefers-reduced-motion: no-preference` (the transform itself is unconditional — correct per the amended comment in `input.css`); row ground `bg-sheffield-light/25`; chip `1 / 8` on `bg-sheffield-accent`. `round2-tv-routine-stamp-row8-detail.jpg` |
| 8/8 | All eight checked via the remote: chip `8 / 8 ☀️` on `bg-sheffield-sun`, both wordmark suns `animation-name: spin`, `animation-duration: 8s`; progress bar full (939 px). `round2-tv-routine-8of8-celebration.jpg` |
| QR overlay inside the card | Play/Pause-equivalent path (Enter on *Add a phone*): overlay stack h1 186→246, sentence 270→310, QR card 334→718 (svg 320 px at 800–1120), URL 742→782, Back 806→894 — all within the card interior 132→948; `document.scrollHeight` 1080 (no page scrollbar). Backspace closes it and focus returns to `tv-join-qr`. `round2-tv-join-qr-overlay-1080p.jpg` |
| Today / Whiteboard | Plain `font-poster text-sheffield-dark` h1s; rail 216→872 on both (shorter header); min font size on `/tv` across all three panels = 30 px (D8 ≥ 28). `round2-tv-today-1080p.jpg`, `round2-tv-whiteboard-1080p.jpg` |
| Console | No wasm `RuntimeError`/panic across load, list walk, 8 checks, overlay open/close, Routine→Today→Whiteboard. The only entries are six Chrome-extension `message channel closed` exceptions timestamped exactly at two `Page.captureScreenshot` timeouts of the QA tooling (21:23:21, 21:25:38) — not the app. QD-11 was not reproduced. |

---

## 4. Round-1 Low observations — status

| id | Status | Note |
| --- | --- | --- |
| QD-03 screensaver overlay with zero photos | OPEN | `screensaver.rs` unchanged since `753a7ec`; still not recorded as a decision. Low. |
| QD-04 owner-checklist row for 🪥/🥤 on Fire OS | OPEN | No such row in `docs/OWNER_CHECKLIST.md`. Low. |
| QD-05 QR overlay overflows under ~1000 px tall | OPEN (harmless at target) | `join_overlay` unchanged; verified it fits with 54 px spare at 1080p (§3). Low. |
| QD-06 no `Cache-Control` on `/fonts` | **FIXED** | folded into QD-01 — `public, max-age=31536000, immutable`, asserted in `font_tests`. |
| QD-07 phone header h1 is the brand, tab label 14 px | OPEN | `mobile/mod.rs:157-164` unchanged. Low. |
| QD-08 no scroll-into-view for the virtual focus | **FIXED** | see §2. The second half of the note (the ring's edges clipped by the scroll container) remains — now QD2-L1 below. |
| QD-09 palette table does not print `size` | OPEN | `tests/palette_tests.rs:317` unchanged. Low. |
| QD-10 §3.4 "URL line and Back unchanged" vs code | OPEN | comment still explains only the QR size. Low. |
| QD-11 whiteboard `ResizeObserver` wasm panic | OPEN / not reproduced | `whiteboard.rs` not in the fix diff; no panic seen this round. Backlog. |

---

## 5. Design critique after the fixes — does it read as the poster?

Yes, and now on the television it was drawn for. At 1920×1080 the white page finally has the poster's generosity back: four boys stacked with air between them, a quiet one-line *Add a phone* pinned at the bottom of the rail, and three full routine rows plus the top of a fourth in the list — the eye lands on `SHEFFIELD / ☀️ Morning Routine ☀️` first, then walks the emoji → empty square → instruction rows exactly the way it walks the laminated sheet. Dropping the *Play/Pause shows the code* second line was the right call: the poster has no footnotes, and the hint row already carries the key. The smaller 80 px discs read *better*, not worse — closer to the poster's small icons, less like app avatars.

The scroll-into-view fix changes the feel of the routine more than any pixel change: a child pressing Down now sees the yellow ring ride down the list and the list move under it, which is what a finger on a paper checklist does. Together with the crooked blue stamp and the sun-yellow `8 / 8 ☀️`, the interaction now carries the poster's sincerity rather than only its colours.

What still separates it from the wall: the ring's bottom edge is under the scroll edge whenever the focused row is the last visible one (QD2-L1) — a 12 px thing, but it is the one place the frame looks clipped instead of drawn; and the QR overlay's Back button is a full-width blue bar with the word at its left edge (QD2-L2), which is the only element on the kiosk that does not sit centred on its page. Both are Low and neither touches the poster identity. The flat single-blue frame (the poster's two blues were consciously not copied) remains a stated divergence, and a good one on a backlit panel.

---

## 6. New observations (Low — do not affect the verdict)

| id | task | tier | file:line | sev | description | solution |
| --- | --- | --- | --- | --- | --- | --- |
| QD2-L1 | D4.3 | O | `src/client/components/tv/surface.rs:341` (routine `ul … overflow-auto`), `tv/style.rs:173` (`TV_PROFILE_RAIL_CLASS`) | Low | `block: nearest` lands the focused row/pill *flush* with the scroll container's bottom edge (rows 4–8 at [744, 872] in a list ending at 872; *Add a phone* at [800, 872] in a rail ending at 872), so the ring's `ring-offset-4 + ring-8` = 12 px bottom edge is under `overflow` and the focus ring shows on three sides only (visible in `round2-tv-routine-row8-focus-scrolled.jpg`, `round2-tv-rail-add-a-phone-focus.jpg`). Pre-existing in kind (round 1 noted ring clipping), but the new scroll behaviour makes "last visible row" the *common* case. | Give both scroll containers `scroll-py-3 py-3 -my-3 px-3 -mx-3` (12 px scroll-padding so `nearest` stops 12 px short of the edge, and matching negative margins so layout is unchanged) — or simply `pb-3` on the rail (600 + 12 = 612, still ≤ budget) and `scroll-pb-3` on the list. Update `tv_rail_needed_px` if padding is added so the budget test stays honest. |
| QD2-L2 | D4.3 (pre-existing at `e36cf66`) | O | `src/client/components/tv/surface.rs:614-618` (`join_overlay` Back button `class: "{ring} w-auto …"`) | Low | `focus_class()` prepends `TV_FOCUSABLE_CLASS`, which contains `block w-full text-left`; Tailwind orders `w-full` after `w-auto`, so `w-auto` is dead and the Back button is a 1656 px wide left-aligned bar under a centred stack (`round2-tv-join-qr-overlay-1080p.jpg`, also in round 1's `tv-join-qr-overlay-826px.jpg`). | Use `!w-auto text-center` (Tailwind important modifier) or `self-center` with `w-fit`; no test needs to change (golden focus order is by id). |

---

## 7. Screenshots (`docs/design/qa/round2-*.jpg`, all ≤ 110 KB, from the 1920×1080 same-origin harness scaled 0.76 into the 1536×826 viewport; the capture tool varied its crop between shots, which is why some frames show more dark border than others)

| File | State |
| --- | --- |
| `round2-tv-dashboard-1080p.jpg` | `/tv` Routine, Boy 1 focused, 0/8 — four boys and *Add a phone* inside the rail, no rail scrollbar |
| `round2-tv-routine-row8-focus-scrolled.jpg` | Enter, ArrowDown ×7 — row 8 focused and in view, list scrolled (QD-08 fixed; ring bottom under the edge = QD2-L1) |
| `round2-tv-routine-stamp-row8-detail.jpg` | Enter on row 8 — the stamp (−4°, 1.06×), row tint, 1/8 |
| `round2-tv-routine-8of8-celebration.jpg` | all eight checked — chip on `bg-sheffield-sun`, suns turning, focus back on row 1 |
| `round2-tv-rail-add-a-phone-focus.jpg` | Backspace, Down ×4 — *Add a phone* focused inside the rail, Boy 4 active |
| `round2-tv-join-qr-overlay-1080p.jpg` | Enter on *Add a phone* — overlay entirely inside the poster card (QD2-L2 visible) |
| `round2-tv-today-1080p.jpg` | Today panel |
| `round2-tv-whiteboard-1080p.jpg` | Whiteboard panel — mirror back to a proper board (1200×532) |

`/m` was not re-audited this round: neither fix commit touches the phone surfaces.
