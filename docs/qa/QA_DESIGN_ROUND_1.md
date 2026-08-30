VERDICT: FAIL

# QA design round 1 — Phase 4 (P4.1), commits `7d6644d..e36cf66`

**Auditor:** Fable 5, independent design + code QA (no prior context) · **Date:** 2026-08-30 · **Branch:** `main` @ `e36cf66`
**Brief:** `docs/design/inspiration/sheffield-morning-routine-poster.jpg` + `docs/design/DESIGN_DIRECTION.md` (§2 language, §3 per-surface, §4 acceptance, §5 fonts) · `docs/PLAN.md` §0 + D8 · `docs/NON_RUST.md`
**Counts:** 0 Critical · 0 High · **2 Med** · 9 Low. PASS requires zero Critical/High/Med → **FAIL** on QD-01 and QD-02.

Both Med findings are cheap, well-bounded fixes; nothing in this round questions the direction or the bulk of the implementation, which is good work (see §3).

---

## 1. Gates (run on this PC, foreground)

| Gate | Result |
| --- | --- |
| `cargo fmt --check` | clean (exit 0) |
| `cargo clippy --features server --all-targets -- -D warnings` | clean, 0 warnings |
| `cargo clippy --features web --target wasm32-unknown-unknown -- -D warnings` | clean, 0 warnings |
| `cargo test --features server` | **429 passed, 0 failed**, 2 ignored (1 pre-existing in `storage_tests`, 1 doc-test) across 27 binaries; new suites `font_tests` 7/7, `glyph_tests` 6/6, `palette_tests` 6/6, `tv_tests` 27/27 |
| `assets/tailwind.css` vs fresh rebuild (`tailwindcss.exe -c ./tailwind.config.js -i ./input.css -o <tmp> --minify`) | SHA-256 identical (`6DB4ECFE…0243`, 20 068 bytes both) |
| `tests/golden/tv_focus_order.txt` / `tv_type_scale.txt` | blob ids identical at `7d6644d` and `e36cf66` (`1a3af5d…`, `4f4a4fc…`) — byte-identical |
| `cargo build --features server` + `dx build --platform web --release` | both green; the dx bundle on disk predated the phase (built 04:01, phase landed 15:44+) so it was rebuilt before looking |
| Server run (`family-hub.exe run`, `127.0.0.1:18097` / TLS `:18460`, fresh data dir) | boots clean; `/health` 200; no ERROR/WARN in the log |

---

## 2. §4 acceptance — each clause actually exercised

| Task | Tier | Clause | Result | Evidence |
| --- | --- | --- | --- | --- |
| **D4.1** Bundle the poster faces | S | (a) `GET /fonts/{nunito-600,nunito-800,baloo2-800}-latin.woff2` → 200, `font/woff2`, body `wOF2` | PASS | `font_tests::fonts_route_serves_each_woff2…` green; re-verified live against the running server: 200, `font/woff2`, first 4 bytes `wOF2`, 16 556 / 16 520 / 18 632 bytes |
| | | (b) `input.css` exactly 3 `@font-face`, no `http` | PASS | test green; manual grep: 3 blocks, absolute `/fonts/…` URLs |
| | | (c) compiled css has `Baloo 2` in `.font-poster` | PASS | `.font-poster{font-family:Baloo\ 2,Nunito,…}` present (cssnano escapes the space; test normalises) |
| | | (d) each woff2 ≤ 120 KB, OFL texts present | PASS | all < 19 KB; `OFL-nunito.txt` / `OFL-baloo2.txt` are the real SIL OFL 1.1 texts with the project copyright lines |
| | | Boss addition: `sw.js` precaches the three fonts, ≤ 6 KB | PASS | 3 strings added; `service_worker_source_stays_within_the_six_kilobyte_budget` green |
| | | **Overall** | **PASS with QD-01 (Med)** | The acceptance is met on this PC, but the route serves from a compile-time source-tree path rather than embedding the bytes — see QD-01 |
| **D4.2** Glyph module + phone/screensaver | S | (a) all 8 seeded `icon_name`s (from `db::SHEFFIELD_MORNING_ROUTINE`) → non-ASCII glyph; unknown → `✅` | PASS | `d4_2_a_*` ×2 green + 3 unit tests in `glyphs.rs` |
| | | (b) SSR mobile routine row contains `☀️`, not `graduation-cap` | PASS | `d4_2_b_*` ×2 green; live `/m` SSR (fetched over HTTPS) contains 0 occurrences of `graduation-cap` |
| | | (c) SSR `Screensaver` has `Sheffield Family Hub` inside a `bg-slate-800` element | PASS | `d4_2_c_*` green — but note QD-03 (Low): the test only passes because the no-images gate was removed, a behaviour change |
| | | (d) palette suite still green | PASS | 6/6; no new colour classes |
| | | **Overall** | **PASS** | |
| **D4.3** The kiosk becomes the poster | O | (a) `☀` before the h1 text; `SHEFFIELD` with `tracking-[0.35em]` | PASS | `d4_3_a` green; live DOM: h1 = `☀️ Morning Routine ☀️`, eyebrow 30 px bold tracked |
| | | (b) every routine row contains its `icon_glyph` | PASS | `d4_3_b` green; live: ☀️ 🛏️ 🪥 🥤 🍳 🏃🏾 📖 📚 all render in Chrome (Fire OS tofu check is still an owner item — QD-04) |
| | | (c) root has `bg-sheffield-light` + `p-[5%]`; exactly one `border-slate-800`+`bg-white` element | PASS | `d4_3_c` green across all golden models |
| | | (d) golden focus order byte-identical | PASS | blob ids identical (table above); `d4_3_d_e_f` re-pins 29 ids |
| | | (e) type-scale golden unchanged, `t3_4_b` green | PASS | four sizes 30/36/48/60; live min font on `/tv` = 30 px, zero elements < 28 px |
| | | (f) hover-grep `t3_4_d` green | PASS | also re-checked the 8/8 render; compiled css has 4 `hover:` rules, all from phone components |
| | | (g) 8/8 → `bg-sheffield-sun`, 1/8 → `bg-sheffield-accent` | PASS | `d4_3_g` green; live: chip class `bg-sheffield-sun`, text `8 / 8 ☀️`, both wordmark suns carry `motion-safe:animate-spin` |
| | | **Overall** | **FAIL — QD-02 (Med)** | All seven lettered clauses pass, but the shipped layout does not fit the kiosk's own 1920×1080 target: the fourth boy's rail button is clipped and *Add a phone* is pushed fully out of view (§4 could not see this — it is SSR-only) |
| **D4.4** Palette contract amendment | S | `cargo test --test palette_tests` green | PASS | 6/6 |
| | | printed table shows the new pair at 3.16:1 flagged Large | PASS (partial) | prints `text-sheffield-accent on bg-white 3.17:1` (3.165 rounds up); the **Large flag is not printed** — Low QD-09 |
| | | negative test: a Body pair under 4.5 still fails | PASS | `the_checker_rejects_a_deliberately_sub_aa_body_pair` (light-on-white 2.0:1, declared Body) fails the checker as required |
| | | **Overall** | **PASS** | |

---

## 3. Faithfulness — does it read as the poster? (what I saw in Chrome, 1920×1080 window)

Screenshots: `docs/design/qa/*.jpg`. Note on scale: this PC's display is 125 % DPR, so Chrome's CSS viewport at a 1920×1080 window is 1536×826; the kiosk target (`docs/device.toml`: 1920×1080 render) has 25 % more room in both axes. Everything below that depends on vertical room has been re-computed for the real 1080-line target from measured element heights, not eyeballed from the smaller viewport.

**What reads right — and reads *as the poster*:**

- **The frame and the card.** Sky-blue overscan band, white page, thin near-black rounded border: the first-glance identity of the poster is there, flat colour, no gradients, no paper gimmicks. Turning the safety margin into the design is the best single move in the phase. The four balls in the corners of the band, clear of all text, are exactly §2.8.
- **The wordmark.** `SHEFFIELD` tracked eyebrow over `☀️ Morning Routine ☀️` in Baloo 2 800, with *Morning* in the display red and a genuine dark outline (`text-shadow` 4-way, 2 px; computed 60 px / 800 / `rgb(232,106,88)`). It is unmistakably the poster's headline. The choice to keep *Routine* quiet and let only *Morning* shout is right, and the suns flank the line rather than sit under text. Nunito actually loads now (`document.fonts`: Nunito 400-600, Nunito 700-900, Baloo 2 700-900 all `loaded`), which alone lifts every panel out of the Segoe-dashboard look of the before shots.
- **Row anatomy.** Emoji → empty rounded square → instruction *with the why in parentheses on the same line* is the poster's row, and the toothbrush-not-toilet choice is honoured. The **stamp** is genuinely satisfying: the blue tick lands rotated −4° and 6 % larger, and the row ground flips to the light tint — it looks like a kid checked it.
- **8/8.** Count chip flips to sun-yellow with a sun of its own, and the two wordmark suns turn slowly. Exuberant, not noisy — exactly the brief.
- **The rail.** ⚽ 🏈 ⚾ 🏀 after each boy's name is a small, charming lift straight off the poster's corners.
- **Other panels.** *Today* and *Whiteboard* wear the face and the blue heading and stay quieter than the routine, as §2.6 asks.

**What does not read right:**

- **The card is crowding the content (QD-02).** The poster's white page is generous; ours is cramped: at the real 1920×1080 the rail has 580 px for four 144 px boys (636 px needed) plus *Add a phone* (816 px needed). Boy 4's button loses its bottom 38 % and *Add a phone* is entirely off-screen behind `overflow-hidden`. Before the phase the four boys fit (rail ≈ 712 px). A poster "for four boys" that cuts the fourth boy off is the one thing the owner will notice first. The same lost ~130 px shrinks the whiteboard mirror to a wide strip.
- **Density of the routine list.** Only ~3 rows are visible at once at 1080p (the direction accepts scrolling, so this is not a defect), but combined with the pre-existing absence of scroll-into-view for the virtual focus (QD-08) a child stepping down the list will lose sight of the focused row after the third item.
- **Flat frame vs the poster's two blues.** The poster's frame is lighter above the midline and deeper below. The direction deliberately chose flat `sheffield-light` (gradients banned) — accepted, noted only as a conscious divergence.
- **The QR overlay** is a smaller poster on the poster, as intended, but the heading/sentence/URL sizes moved (see QD-10) and at viewports under ~1000 px tall the centred stack overflows the card top and bottom (QD-05) — fine at exactly 1080p (708 px stack in an 800 px interior).

Net: the kiosk now *is* the Sheffield Morning Routine poster on the wall — frame, headline, rows, stamp, balls — and the remaining work is fitting the poster's own generosity into 1080 lines.

---

## 4. Standards checks

| Check | Result |
| --- | --- |
| **WCAG AA** — Body ≥ 4.5, exactly one Large pair | PASS. `t3_4_a_every_declared…` and `…actually_paints…` hold every painted pair to its own floor; the only Large pair painted anywhere is `text-sheffield-accent` on `bg-white` (asserted `assert_eq!(large, vec![(accent, white)])`). Live: that pair is used once, at 60 px / 800. All 10 painted pairs AA. |
| **Tests not weakened** | PASS. Before: blanket `ratio >= 4.5` for every pair (palette.rs unit test + two places in `palette_tests.rs`). After: Body still `>= 4.5`, Large `>= 3.0` (the agreed amendment) **plus** a new "only the wordmark may walk through the Large door" assertion, a pinned [3.1, 3.3] ratio, and a negative Body test. `tv_tests.rs` assertions unchanged; `NON_TEXT_PAIRS` unchanged; overscan walker and hover grep unchanged. Nothing laxer. |
| **10-foot** — ≥ 28 px on `/tv` | PASS. Live scan of every leaf text node in `#tv-root`: min 30 px, none < 28. Type-scale golden unchanged. |
| **10-foot** — 5 % overscan | PASS. Root `p-[5%]` (76.8 px at 1536, 96 px at 1920); card sits inside it; corner balls at 1.6 % are inside the band by design (§3.1.6). |
| **10-foot** — focus ring on every focusable | PASS. 13 `[data-tv-focus]` elements on the dashboard, every one carries `ring-8 ring-offset-4 ring-offset-sheffield-dark`; the active one adds `ring-sheffield-sun`. (Visibility of the ring is a separate pre-existing issue, QD-08.) |
| **10-foot** — no hover-only | PASS. `t3_4_d` green; no `hover:` in `tv/` source or rendered markup (incl. 8/8 render). |
| **Local-first** — zero external requests | PASS. Compiled `assets/tailwind.css`: the only `https://` is the Tailwind licence comment; `sw.js`: none; `src/client/components/**`: only the join-URL formatting/fixtures (`https://{host}:{port}/m`). No `googleapis`/`gstatic` anywhere. Live network log for `/tv`: every request is `127.0.0.1:18097` (fonts, wasm, js, css, server fns) — the only off-origin entries are Chrome extension scripts and the phone-origin `manifest.webmanifest` probe (pre-existing D3′ behaviour). |
| **Fonts genuinely same-origin** | PASS on this PC (see D4.1 row); **QD-01** on deployment robustness. |
| **OFL licences** | PASS. Both texts committed beside the woff2s. |
| **Rust-only** | PASS. No new non-Rust code; `sw.js` grew by 3 string literals (declared component, budget asserted). Font files are data assets (`NON_RUST.md` §4 says no row needed; optional row not added — fine). |
| **Console** | One `RuntimeError: unreachable` (wasm panic) seen twice, from `ResizeObserver` → whiteboard canvas repaint; not reproducible on a clean reload + panel round-trip; the code path (`whiteboard.rs::install_resize_observer`) is untouched by this phase — logged as QD-11 (pre-existing). |
| **`/m` at 390×844** | Phone origin redirects `http` → `https` (308, by design). The Chrome tools cannot attach to the self-signed interstitial, so the phone was audited from the live `/m` SSR HTML (HTTPS, `curl -k`) plus `mobile/mod.rs` + `routine.rs`: header is `☀️ Sheffield Hub` in `font-poster` on `bg-sheffield-dark` with the tab label beside it; tab glyphs are `☀️ 📅 🖍️ 📺 ⚙️`; the row leads with the glyph and the raw `icon_name` span is gone. Not visually verified — owner checklist. |

---

## 5. Findings (Med and above — these set the verdict)

| id | task | tier | file:line | sev | description | solution |
| --- | --- | --- | --- | --- | --- | --- |
| **QD-01** | D4.1 | S | `src/server/router.rs:224-227` (`FONTS_DIR`, `fonts_router`) | **Med** | The three woff2 files are served with `ServeDir::new(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/fonts"))` — an absolute path into the **build machine's source tree**, baked at compile time. Every sibling static asset (`tailwind.css` via `include_str!` at `router.rs:152`, `sw.js`, the four PWA icons via `include_bytes!` in `mobile/pwa.rs`) is embedded in the binary. Under the documented install (`docs/OWNER_CHECKLIST.md` §3: copy `family-hub.exe` + `public\` to `C:\Program Files\FamilyHub\`) the fonts keep working only while `C:\Family Calendar\assets\fonts` exists; move/delete the checkout, or move to the Pi (P4.4), and `/fonts/*` silently 404s and, because of `font-display: swap`, the entire kiosk quietly reverts to Segoe — the exact pre-phase state — with no error anywhere. `font_tests` cannot catch this (they run from the checkout). | Embed the bytes: `const NUNITO_600: &[u8] = include_bytes!("../../assets/fonts/nunito-600-latin.woff2");` (×3) and replace `fonts_router` with three `get` routes returning `([(CONTENT_TYPE, "font/woff2"), (CACHE_CONTROL, "public, max-age=31536000, immutable")], bytes)` — ~52 KB in the binary, the same pattern as `ICONS`. Existing `font_tests` (200 / `font/woff2` / `wOF2`) pass unchanged; add one assertion that the response carries `Cache-Control` (the §5 "long Cache-Control" note is currently unimplemented too). |
| **QD-02** | D4.3 | O | `src/client/components/tv/style.rs:98` (`TV_POSTER_CARD_CLASS` `p-10`), `surface.rs:92` (card `gap-8`), `surface.rs:210` (rail `overflow-hidden`), `surface.rs:252` (profile `py-6`), `surface.rs:274` (join button) | **Med** | At the kiosk's declared render target (1920×1080, `docs/device.toml`) the vertical budget no longer fits the rail. Measured live: root padding 96×2, card border 4×2 + padding 40×2, header 103.2 (eyebrow + 60 px lockup; was ~60), gaps 32×2, footer 52 → **rail = 580.8 px**. Four profile buttons need 4×144 + 3×20 = **636 px**; with *Add a phone* (160 px) **816 px**. Result: Boy 4's button is clipped by ~55 px (its lower 38 %, including the bottom of its focus ring) and *Add a phone* is completely invisible behind `overflow-hidden` while still being a focus stop (Down from Boy 4 lands on an element the viewer cannot see). Pre-phase the rail had ≈ 712 px: all four boys fit and the top of *Add a phone* was visible. The same ~130 px loss squeezes the whiteboard mirror. Screenshot `tv-profile-boy4-rail-clipped.jpg` shows the symptom at 826 px (Boy 4 active in the header, absent from the rail); the 1080p numbers are computed from the measured element heights. No SSR test can see this, which is why §4 passed. | Reclaim the room, minimum change set: (1) `TV_POSTER_CARD_CLASS` `p-10` → `p-8` (+16), (2) card `gap-8` → `gap-6` (+16), (3) profile buttons `py-6` → `py-4` (144 → 128 each, +64) → rail ≈ 629 px ≥ 572 px for four boys with ~57 px spare at 1080p. Then make the hidden focus stop visible: either give the rail `overflow-y-auto` and scroll the virtual focus into view (a `use_effect` on `current_focus` in `tv/shell.rs` calling `web_sys::Element::scroll_into_view_with_bool(false)` on the focused `dom_id()` — pure Rust via the existing `web-sys` row, and it fixes the routine list too, see QD-08), or fold *Add a phone* into a compact one-line pill (drop the "Play/Pause shows the code" second line, `py-4`) so the whole rail is ≤ 629 px. Keep the golden focus-order file untouched (no new focusables). Re-shoot `tv-after-d4.3.jpg` at a true 1920×1080 CSS viewport (DPR 1) for `current-state/`. |

---

## 6. Low observations (do not affect the verdict)

| id | task | file:line | note |
| --- | --- | --- | --- |
| QD-03 | D4.2 | `src/client/components/screensaver.rs:142-155` | Behaviour change outside §3.3's "keep the crossfade exactly as is": the `images.is_empty()` early-return was removed so the caption test could pass under SSR (no images resolve). Consequence: a kiosk with **zero** screensaver photos (fresh install) now goes to a full black overlay + caption after 10 min idle, where it previously stayed on the routine. Defensible (the schedule-forced overlay was invisible with no photos before), but it should be a stated decision; if unwanted, keep the caption but gate the overlay on `!images.is_empty() \|\| scheduled_on`. |
| QD-04 | D4.2 | `docs/OWNER_CHECKLIST.md` | §2.5 asks for an owner-checklist row to verify 🪥 and 🥤 render on the real Fire OS WebView (Chromium 138 per `device.toml`, so likely fine) with the ✨/💧 fallback noted. No row was added. |
| QD-05 | D4.3 | `surface.rs:587` (`join_overlay` card `items-center justify-center`) | The centred stack (h1 60 + p 40 + QR card 384 + url 40 + Back 88 + 4×24 gaps = 708 px) fits the 800 px card interior at 1080p, but at any viewport under ~1000 px tall it overflows *both* ends of the card (heading clipped above the frame, Back button on the frame) and the page grows a scrollbar — `tv-join-qr-overlay-826px.jpg`. Add `overflow-y-auto` on the overlay card or `justify-start` + `mt-auto`; harmless at exactly 1080p. |
| QD-06 | D4.1 | `router.rs:227` | No `Cache-Control` on `/fonts/*` (§5: "long Cache-Control fine"). Folded into QD-01's solution. |
| QD-07 | D4.2 | `mobile/mod.rs:157-164` | The phone page `<h1>` is now the brand (`Sheffield Hub`) and the tab name dropped from `text-lg` h1 to a `text-sm` (14 px) span. Semantically the page heading is no longer the current tab; visually the tab label is small for a header. Consider `text-base font-bold` for the tab label, or keep the tab name as the h1 and the brand as a span. |
| QD-08 | pre-existing (T2.1) | `tv/shell.rs`, `surface.rs:210,335` | No scroll-into-view for the virtual focus: the routine `ul` is `overflow-auto` and the rail `overflow-hidden`, and `document.activeElement` stays on `#tv-keyboard-host`. Pressing Down seven times moved focus to row 8 while the list stayed scrolled to the top (no focus ring visible on screen — `tv-routine-8of8-celebration.jpg`). Also the ring's top/side edges are clipped by the scroll container even for visible rows (visible in the pre-phase `tv-routine-checked-focused.jpg` too). Not introduced by this phase, but the phase's tighter layout makes it bite sooner; QD-02's scroll-into-view fix resolves both. |
| QD-09 | D4.4 | `tests/palette_tests.rs:317` | The printed table (`println!("{:<26} on {:<28} {ratio:>6.2}:1", …)`) does not print the `size` column, so "flagged Large" in the D4.4 acceptance is not literally met. Add `{:?}` of `pair.size` to the line. |
| QD-10 | D4.3 | `surface.rs:588-613` | §3.4 says "the URL line and Back button unchanged", but the sentence and the URL moved from `TV_HEADING` (48 px) to `TV_BODY_LARGE` (36 px) and the QR from 520 → 320 px. All still ≥ 28 px and AA; note it as a deliberate deviation in the code comment (the comment explains the QR, not the text). |
| QD-11 | pre-existing (T2.3) | `src/client/components/whiteboard.rs:36-43, 284-300` | Console: `RuntimeError: unreachable` twice (19:22:34, 19:23:44) with the stack `ResizeObserver.r → wasm`. The observer's closure reads the `stroke_log` signal outside the component; if the whiteboard component has been dropped when the observer fires its final callback (panel switch / page teardown), the signal read panics. Not reproduced on a clean reload + Routine→Whiteboard→Routine round-trip; not in the phase diff. Suggest guarding the callback with a `try`-style read (`stroke_log.try_read()` / check the closure's `Rc` liveness) — for the Boss's backlog. |

---

## 7. Screenshots (`docs/design/qa/`, all ≤ 91 KB, Chrome at a 1920×1080 window = 1536×826 CSS px on this 125 % display)

| File | State |
| --- | --- |
| `tv-dashboard.jpg` | `/tv` Routine, Boy 1 focused, 0/8 — frame, card, wordmark, glyph rows, corner balls |
| `tv-routine-maximized-focus.jpg` | Enter on Boy 1 → focus into the list (row 1 ring, clipped by the scroll container) |
| `tv-routine-stamp-1of8.jpg` | Enter on row 1 → the stamp (−4°, 1.06×), row tint, chip 1/8 |
| `tv-routine-8of8-celebration.jpg` | all eight checked → chip on `bg-sheffield-sun` with ☀️, wordmark suns rotating; focus on row 8 is off-screen (QD-08) |
| `tv-profile-boy4-rail-clipped.jpg` | Backspace, Down ×4 → Boy 4 active in the header but not visible in the rail (QD-02) |
| `tv-routine-detail-zoom.jpg` | 0.8 page-zoom crop: rows with 🪥 glyph, Boy 3 ⚾, three boys visible |
| `tv-today.jpg` | Today panel — plain `font-poster` blue heading, rail balls |
| `tv-whiteboard.jpg` | Whiteboard panel — board on the light-blue ring; canvas reduced to a strip at this height |
| `tv-join-qr-overlay-826px.jpg` | Play/Pause / Add a phone → overlay; overflow at 826 px viewport (QD-05); fits at 1080p |

`/m` could not be screenshotted (self-signed interstitial is not attachable by the browser tools); see §4 for how it was audited.
