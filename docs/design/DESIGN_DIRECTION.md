# Phase 4 (P4.1) Design Direction — the poster, alive on the TV

**Author:** Fable 5 (design consultant, fresh context) · **Date:** 2026-08-30
**Inspiration:** `docs/design/inspiration/sheffield-morning-routine-poster.jpg` — the family's laminated wall poster; the owner's words: *"I would like to use the Sheffield Morning Routine as inspiration."*
**Current state (screenshots, 2026-08-30, `main` @ `7d6644d`):** `docs/design/current-state/*.jpg`

**Thesis.** The hub should feel like the Sheffield Morning Routine poster came off the wall and onto the TV — not a dashboard that happens to use the poster's colors. Three moves get us there: (1) the poster's **sky-blue frame around a white card** becomes the kiosk's page structure (the 5% overscan zone *is* the frame), (2) the poster's **stacked headline with the red MORNING and the two suns** becomes the routine panel's wordmark, and (3) every routine row gets back its **emoji icon + empty-square checkbox** anatomy, with a satisfying stamp when checked. Everything else — palette, focus system, key map, type scale mechanics — is already right and stays.

---

## 1. Reading of the inspiration

What gives the poster its character (verified against the photo):

1. **The stacked headline.** Three lines: SHEFFIELD (black caps, wide letterspacing), **MORNING** (huge red display caps with a dark outline — the single loudest thing on the page), ROUTINE (black caps, wide letterspacing). Two radiant yellow suns flank MORNING. The middle word carries all the energy; the outer two lines are quiet and tracked.
2. **The frame.** A sky-blue border (lighter above, a deeper blue below the midline) wraps a white card that itself has a thin near-black inner border. The content lives on white; the blue is pure frame — nothing ever sits on it except the corner art.
3. **Row anatomy.** Every routine item is: full-color emoji-style icon → empty rounded square checkbox → friendly rounded-sans text, with the *why* in parentheses after the instruction ("(Quench your thirst.)", "(Take care of your temple.)"). The parenthetical "why" is already the seeded `description` column — the app preserved the poster's voice; the design must preserve its anatomy.
4. **The sports-ball corners.** Soccer ball, football, baseball, basketball anchor the bottom of the card — four balls for four boys. They are decoration with meaning.
5. **The material feel.** It is a laminated home-made poster, hand-hung with push-pins: warm, rounded, sincere, a little exuberant. Not an app, not a brand system.
6. **The face.** A soft rounded geometric sans throughout — exactly the register Nunito was picked for in `tailwind.config.js` (and never actually loaded; every surface today renders in Segoe UI).

What we deliberately do **not** copy:

- **The photograph's artifacts** — glare, lamination shine, wall texture, push-pins. No skeuomorphic paper shadows or "tape" gimmicks; the TV version is the poster's *idea*, not a picture of it.
- **The cramped density.** The poster squeezes 8 rows onto A4; the TV has a scrolling list and 30px+ type. Never shrink text to fit all 8 rows on screen at once.
- **Letterspaced ALL-CAPS body text.** Caps + tracking is reserved for the one-word eyebrow; instructions stay sentence-case (they already are).
- **The toilet.** The bathroom row's icon becomes the toothbrush, not the poster's 🚽.
- **Red as body ink.** The poster's red is display-only; on our surfaces `sheffield-accent` is ~3.16:1 on white — legal only for ≥44px bold display and non-text, never body copy (rules in §2.2).

---

## 2. Design language spec

### 2.1 Typography

Two faces, both OFL, both bundled locally (acquisition in §5). **No external font requests — ever.**

| Role | Face / weight | File | Used for |
| --- | --- | --- | --- |
| Poster display | **Baloo 2** ExtraBold 800 | `assets/fonts/baloo2-800-latin.woff2` | The wordmark word "Morning", panel headings (`TV_HEADING_LARGE`/`TV_HEADING`), the phone header title, the `n / 8` count |
| Body / everything | **Nunito** SemiBold 600 | `assets/fonts/nunito-600-latin.woff2` | All body text, both surfaces (600 because 400 goes thin at 10 feet) |
| Body emphasis | **Nunito** ExtraBold 800 | `assets/fonts/nunito-800-latin.woff2` | Row titles, buttons, tabs, the tracked eyebrow |

Tailwind families (in `tailwind.config.js`):
- `font-display`: `["Nunito", "ui-sans-serif", "system-ui", "sans-serif"]` (unchanged stack — it finally *loads*).
- **New** `font-poster`: `["'Baloo 2'", "Nunito", "ui-sans-serif", "system-ui", "sans-serif"]`.

`font-weight` mapping: body text uses 600 as its `font-semibold`/default; `font-bold`+ resolves to the 800 file (declare the 600 face with `font-weight: 400 600` and the 800 face with `font-weight: 700 900` in `@font-face` so every existing `font-bold`/`font-black` class lands on a real file, no faux-bold).

**TV type scale is unchanged** — the existing four sizes (`text-3xl` 30 / `text-4xl` 36 / `text-5xl` 48 / `text-6xl` 60, golden file `tests/golden/tv_type_scale.txt`) already satisfy D8 and fit the new lockup. The wordmark achieves poster-scale contrast through *face and color*, not new sizes. Phone scale also unchanged.

The one new text treatment: `.poster-outline` utility in `input.css` — a four-direction `text-shadow` in `#1E293B` (2px offsets at 1920×1080) replicating MORNING's dark outline. (Text-shadow, not `-webkit-text-stroke`: stroke centers on the glyph edge and eats thin strokes on old Fire OS WebViews.)

### 2.2 Color usage rules (all five hues stay; per-token law)

Contrast figures are WCAG 2.x ratios computed from the hex values (the same math as `src/client/components/palette.rs`).

| Token | Role after Phase 4 | May carry text? | Key pairings |
| --- | --- | --- | --- |
| `sheffield-light` #8BB5DA | **The frame.** Kiosk page ground (the 5% overscan band + gutters); 25% tint stays the completed-row ground; unchecked checkbox ring | **Never ink, never text on it.** The frame carries only the corner balls (emoji) | decorative only |
| `sheffield-dark` #2672B3 | Primary: headings, filled controls, checked checkbox fill, active pills, phone header, progress fill | Ink on white/paper (5.14:1 ✓ body); ground under white ink (✓) | unchanged |
| `sheffield-accent` #E86A58 | **Display red**: the wordmark word "Morning" (+ `.poster-outline`); ground-under-dark-ink chips (Disconnected, count) — as today | As **ink**: only ≥44px bold display (white 3.16:1, paper 3.11:1 — AA **Large** only). Never body ink. As ground: dark ink only (existing pair) | *new pair, Large* |
| `sheffield-sun` #F4D03F | Focus ring (unchanged); the suns' native emoji color; celebration ground: `n / 8` chip flips accent→sun when 8/8 | Never ink. Ground under `slate-800` only (existing pair, 10.4:1) | unchanged + reuse |
| `sheffield-paper` #FDFDFD | Phone page ground (unchanged). **Retired as the kiosk page ground** (the frame replaces it); kept as the QR-overlay card tint | as today | unchanged |

Neutrals (`slate-800/600/200/100/50`, white, black) and the error ramp: unchanged, same rules as T3.4. **New neutral:** none needed — the poster's "black" border maps to existing `slate-800`.

**Test consequence (the only deliberate amendment):** `tests/palette_tests.rs::t3_4_a_every_declared_palette_pair_meets_wcag_aa` currently holds *every* pair to a blanket 4.5:1 (lines 295–302, "no pair leans on the 3:1 large-text allowance"). Amend that blanket assertion to apply to `TextSize::Body` pairs only; `TextSize::Large` pairs are held to ≥3.0 (their AA floor). Exactly one Large pair will exist: `text-sheffield-accent` on `bg-white` (3.16:1), `used_by: "the wordmark word Morning — >=44px, weight 800, .poster-outline"`. Nothing drops below AA; the `size` column finally earns its keep.

### 2.3 Shape language

- **The poster card:** on the kiosk, one full-height white card `rounded-[2.5rem] border-4 border-slate-800` sits inside the light-blue frame — the poster's white-page-with-thin-black-border. This is the *only* element that wears a visible dark border; rows inside stay border-free with `shadow-lg` (current).
- Radii stay in the current family: cards/rows `rounded-3xl`, checkboxes `rounded-2xl`, chips/pills `rounded-full`. Phone: `rounded-2xl` (current).
- Focus ring: **unchanged** — `ring-8 ring-offset-4 ring-offset-sheffield-dark ring-sheffield-sun` (D8; `NON_TEXT_PAIRS` 3.4:1 / 5.1:1 already asserted).

### 2.4 Checkbox and checked state

- **Unchecked** (poster-faithful, already close): white square, `rounded-2xl ring-4 ring-sheffield-light`, empty. Keep.
- **Checked** — the satisfying stamp: fill `bg-sheffield-dark`, white ✓, plus new `.stamp-check` utility in `input.css`: `transform: rotate(-4deg) scale(1.06)` with a 150ms ease-out transition — the check lands like a rubber stamp, slightly crooked, the way a kid actually checks a box. Row ground flips to `bg-sheffield-light/25` (existing pair). Wrap the transition in `@media (prefers-reduced-motion: no-preference)`.
- **All 8 done:** the count chip swaps `bg-sheffield-accent` → `bg-sheffield-sun` (both grounds under `slate-800`, both pairs already in the table) and reads `8 / 8 ☀️`; the two wordmark suns get a slow 8s rotate animation (same reduced-motion guard). No confetti, no sound — the poster is exuberant, not noisy.

### 2.5 Iconography — emoji glyphs, no icon fonts, no downloads

One shared mapping, `icon_glyph(icon_name) -> &'static str`, in a new `src/client/components/glyphs.rs`. The 8 seeded `icon_name`s (`src/server/db.rs::SHEFFIELD_MORNING_ROUTINE`):

| `icon_name` | Glyph | Poster art it stands in for |
| --- | --- | --- |
| `sun` | ☀️ | boy waking / the suns |
| `bed` | 🛏️ | the made bed |
| `sparkles` | 🪥 | toilet + toothbrush (we keep the toothbrush) |
| `droplet` | 🥤 | the cup of water |
| `utensils` | 🍳 | the breakfast plate |
| `activity` | 🏃🏾 | the running boy (keep the skin-tone modifier — it is the family's poster) |
| `book-open` | 📖 | Bible + praying hands |
| `graduation-cap` | 📚 | the stack of school books |

Unknown names fall back to `✅`. Panels/tabs: Routine ☀️ · Calendar/Today 📅 · Whiteboard/Board 🖍️ · TV Remote 📺 · Settings ⚙️ · Add a phone 📱. Profile balls, straight off the poster's corners, keyed by rail position: 1 ⚽ · 2 🏈 · 3 ⚾ · 4 🏀 (`ball_glyph(index)`; 5th+ profile cycles).

Rules: glyphs are always `aria-hidden="true"` spans beside real text, never the only label; they carry **no `text-*` color class** (emoji bring their own color — this also keeps them invisible to the palette scanner); on `/tv` they are sized by the existing scale classes (48px in rows). 🪥 and 🥤 need an owner-checklist row: verify they render on the real Fire OS WebView via `?keys=1` visit (fallback if tofu: swap to ✨ and 💧 — one-line change in `glyphs.rs`). If richer art is ever wanted, the existing Rust `xtask` (resvg) can rasterize SVGs — but emoji is the default and needs no new `docs/NON_RUST.md` row.

### 2.6 The wordmark / headline treatment

Routine panel header (kiosk), replacing the plain `Morning Routine` h1:

```
SHEFFIELD                      ← eyebrow: TV_BODY_TEXT, font-bold, tracking-[0.35em], text-slate-800, uppercase
☀️ Morning Routine ☀️          ← TV_HEADING_LARGE font-poster; "Morning" = text-sheffield-accent .poster-outline;
                                  "Routine" (and any other words) = text-slate-800; suns are emoji, aria-hidden
```

Other panels keep their plain `font-poster text-sheffield-dark` h1 (Today, Whiteboard) — the full lockup belongs to the routine, the heart. The phone header gains the small form: `☀️ Sheffield Hub` in `font-poster`. The QR overlay h1 becomes `📱 Add a phone` in `font-poster`.

### 2.7 Spacing rhythm

Unchanged — the current rhythm (rows `gap-5`, row padding `px-8 py-6`, panel `gap-6`, rail `w-[26rem]`) is sound. The frame adds: overscan band `p-[5%]` (unchanged class) + poster-card inner padding `p-10`. Nothing new to memorize.

**Amended by QA design round 1, QD-02** (`docs/qa/QA_DESIGN_ROUND_1.md`): at the kiosk's declared 1920×1080 render target that rhythm left the profile rail 580 px for four boys who needed 636 px, so the fourth was clipped and *Add a phone* was invisible. The card is now `p-8` with its own `gap-6`, and the **rail alone** (never the routine rows, which keep `px-8 py-6`) is on `py-4` with an `h-20` disc and a one-line *Add a phone* pill. The rail then costs 600 px of the 612 px it gets. The arithmetic lives in `tv::style::tv_rail_budget_px()` and is asserted against the rendered classes in `tests/tv_tests.rs`, so the next spacing edit fails a test rather than a television.

### 2.8 Decorative elements — where allowed, where banned

- **Suns:** exactly two, flanking the wordmark; one small in the phone header; optional one in the screensaver caption chip. Never elsewhere, never behind text.
- **Balls:** in the two bottom corners of the kiosk frame band (absolutely positioned inside the overscan zone, on the blue frame, `aria-hidden`, ~44px) and one per profile row in the rail. Never behind text, never inside the routine list.
- **Banned:** decorative elements behind or under any text; background patterns; gradients (the poster is flat color); any decoration on the phone beyond the header sun and tab glyphs; anything animated except the stamp transition and the 8/8 sun spin.

---

## 3. Per-surface specs

### 3.1 `/tv` dashboard (`src/client/components/tv/surface.rs`, `style.rs`)

Current state: `docs/design/current-state/tv-routine.jpg` (generic Segoe, white-on-paper, no icons, no frame).

1. **`screen_class()`** (surface.rs:38): page ground `bg-sheffield-paper` → `bg-sheffield-light`; immediately inside it (inside the `p-[5%]` overscan, which stays on the same root element — `t3_4_c` untouched) wrap everything in the **poster card**: `flex h-full min-h-0 flex-1 flex-col gap-8 rounded-[2.5rem] border-4 border-slate-800 bg-white p-10`. All existing text now sits on `bg-white`; every pair already in `PALETTE_PAIRS`. The two `bg-sheffield-paper` pairs stay declared (phone still uses paper; QR card below).
2. **Header** (surface.rs `header()`): wordmark per §2.6 on the Routine panel; `font-poster` h1 on the others; `updated HH:MM` and the Disconnected badge unchanged.
3. **Routine rows** (`routine_row()`): reorder to poster anatomy — glyph `icon_glyph(item.icon_name)` (48px, aria-hidden) → checkbox → title + description. Checked treatment per §2.4 (`.stamp-check` on the box span).
4. **Custom-task rows:** photo thumb (if any) plays the glyph's role; otherwise glyph `✅`.
5. **Profile rail** (`profile_button()`): keep disc + `best_ink_on`; append `ball_glyph(rail_index)` after the name (36px, aria-hidden). "Add a phone" button gains 📱.
6. **Corner balls:** two absolutely-positioned aria-hidden spans on the root (on the blue frame band, bottom corners): `⚽🏈` left, `⚾🏀` right.
7. **Footer pills, progress bar:** unchanged (they already read as the poster family once Nunito loads). Focus order golden file **unchanged** — no new focusables.

### 3.2 `/tv` maximized routine

Same panel, same spec — the routine panel *is* the maximized view (panel navigation already handles it). The 8/8 celebration (§2.4) is the only state addition.

### 3.3 Screensaver overlay (`src/client/components/screensaver.rs`)

Keep the black-ground photo crossfade exactly as is (it is calm and correct — `tv-whiteboard.jpg` shows the family the panels; the screensaver should stay photos-first). Add one caption chip, bottom-left, inside 5% of the edges: `rounded-full bg-slate-800 px-6 py-2` containing `☀️ Sheffield Family Hub` in `text-white` `text-3xl` `font-poster` (pair `text-white`/`bg-slate-800` already declared; solid ground, never translucent over photos, so contrast stays computable).

### 3.4 QR overlay (`surface.rs::join_overlay()`)

Inherits the frame automatically via `screen_class()`. h1 → `📱 Add a phone` in `font-poster text-sheffield-dark`; QR card keeps `bg-white`; the URL line and Back button unchanged. Optional: card tint `bg-sheffield-paper` to read as "a smaller poster on the poster".

### 3.5 `/m` — all five tabs (`src/client/components/mobile/mod.rs`, shared panels)

- **Header:** `☀️ Sheffield Hub` beside the tab label, `font-poster` (white on `bg-sheffield-dark`, existing pair).
- **Tab bar** (`MobileTab::glyph()`): `✓▦✎▶⚙` → `☀️ 📅 🖍️ 📺 ⚙️` (labels unchanged; glyphs stay aria-hidden).
- **Routine tab** (`routine.rs::RoutineRow`): delete the trailing raw `{item.icon_name}` string (today it literally prints "graduation-cap"); lead the row with `icon_glyph(...)` at `text-2xl`. Checkbox gets the same `.stamp-check` checked treatment.
- **Calendar / Board / TV Remote / Settings tabs:** typography only — they inherit Nunito/Baloo via `font-display`/header automatically; no layout changes. The remote's D-pad buttons may adopt `font-poster` labels.
- **PWA icons** (`assets/icons/*` via xtask): keep for this phase; a poster-monogram refresh is a later nice-to-have.

### 3.6 Explicitly worth keeping as-is

The focus-ring system (sun ring on dark offset), the four-size type scale, the overscan mechanics, the row/card radius family, the progress bar, the footer pills, the profile-disc `best_ink_on` logic, the screensaver crossfade, the whiteboard palette dots (already the five hues), and the phone's information architecture. This direction adds identity; it does not rearrange working bones.

---

## 4. Implementation task list

Ownership is disjoint; D4.4 lands before D4.3. Acceptance tests are agent-executable in the style of `tests/palette_tests.rs`.

**D4.1 — Bundle the poster faces (tier S).**
Owns: `assets/fonts/**` (new), `input.css`, `tailwind.config.js`, `src/server/router.rs` (one `nest_service("/fonts", ServeDir::new(...))` route mirroring `/assets/screensaver`), `tests/font_tests.rs` (new).
Do: commit the three woff2 files + two OFL license texts (§5); three `@font-face` blocks in `input.css` (`font-display: swap`, absolute `src: url("/fonts/…")` — the CSS is served at `/tailwind.css`, so relative URLs would break); add `font-poster` family; rebuild `assets/tailwind.css`.
Accept: (a) router `oneshot` GET `/fonts/nunito-600-latin.woff2`, `/fonts/nunito-800-latin.woff2`, `/fonts/baloo2-800-latin.woff2` → 200, `content-type` `font/woff2`, body starts `wOF2`; (b) `input.css` contains exactly 3 `@font-face` and no `http` substring; (c) compiled `assets/tailwind.css` contains `Baloo 2` in a `.font-poster` rule; (d) each woff2 ≤ 120 KB and OFL files present.

**D4.2 — Glyph module + phone/screensaver polish (tier S).**
Owns: `src/client/components/glyphs.rs` (new), `mobile/mod.rs`, `routine.rs`, `screensaver.rs`, `tests/glyph_tests.rs` (new).
Do: `icon_glyph`, `ball_glyph`, tab/panel glyph swap, mobile RoutineRow per §3.5, screensaver caption per §3.3.
Accept: (a) unit test — all 8 seeded `icon_name`s (imported from `db::SHEFFIELD_MORNING_ROUTINE`) map to a non-ASCII glyph, unknown → `✅`; (b) SSR of the mobile routine tab contains `☀️` and does **not** contain the literal `graduation-cap`; (c) SSR of `Screensaver` contains `Sheffield Family Hub` inside an element classed `bg-slate-800`; (d) `cargo test` palette suite still green (no new color classes).

**D4.3 — The kiosk becomes the poster (tier O, deps D4.1 + D4.2 + D4.4).**
Owns: `src/client/components/tv/surface.rs`, `tv/style.rs`, `tests/tv_tests.rs`, `tests/golden/*`.
Do: §3.1, §3.2, §3.4 in full (frame + card, wordmark, row anatomy, stamp, balls, 8/8 state, QR overlay heading).
Accept: (a) SSR of the routine panel contains `☀` **before** the h1's text and `SHEFFIELD` with `tracking-[0.35em]`; (b) every rendered routine row contains its `icon_glyph`; (c) rendered root carries `bg-sheffield-light` **and** `p-[5%]`, and exactly one element carries `border-slate-800` + `bg-white` (the poster card) — assert via the existing tag-walker; (d) golden focus order file byte-identical (no new focusables); (e) type-scale golden unchanged and `t3_4_b` green; (f) hover-grep `t3_4_d` green; (g) an 8/8 fixture renders the count chip on `bg-sheffield-sun`, a 1/8 fixture on `bg-sheffield-accent`.

**D4.4 — Palette contract amendment (tier S, lands first).**
Owns: `src/client/components/palette.rs`, `tests/palette_tests.rs`, `input.css` *utility block only* (`.poster-outline`, `.stamp-check` — coordinate with D4.1 by appending, distinct file regions).
Do: add `Pair { ink: "text-sheffield-accent", ground: "bg-white", size: Large, used_by: "the wordmark word Morning — ≥44px/800 + .poster-outline" }`; amend the blanket 4.5:1 assertion per §2.2 (Body pairs ≥4.5, Large pairs ≥3.0 — **never below AA**); add a unit test computing `#E86A58` on `#FFFFFF` ∈ [3.1, 3.3].
Accept: `cargo test --test palette_tests` green; the printed table shows the new pair at 3.16:1 flagged Large; a deliberately-added Body pair under 4.5 still fails (negative test).

**Changed existing assertions, exhaustively:** only `palette_tests.rs::t3_4_a_every_declared_palette_pair_meets_wcag_aa`'s blanket-4.5 clause (per D4.4). `tv_tests.rs` assertions, both golden files, the overscan walker, the hover grep, and `NON_TEXT_PAIRS` all remain exactly as they are — D4.3 must land green against them.

**`docs/NON_RUST.md`:** no new row required. The woff2 files are static data assets served by the existing Rust router, the same category as the screensaver JPEGs and PWA PNGs (which have no rows); no non-Rust *code* is added. If the Boss prefers belt-and-braces, an optional row: *"Nunito + Baloo 2 woff2 (OFL 1.1) — font data assets, acquired at design time from Google Fonts, committed to the repo, served from Rust at `/fonts/*`; licenses committed alongside."*

---

## 5. Font asset plan

- **Faces:** Nunito (Vernon Adams et al., SIL OFL 1.1) — already named in `tailwind.config.js`, never loaded until now; Baloo 2 (Ek Type, SIL OFL 1.1) — the chunky rounded display face closest to the poster's MORNING letterforms.
- **Source:** the Google Fonts static-woff2 CDN, fetched **once at design/build time** and **committed to the repo** (a build-time acquisition, exactly like the Tailwind standalone binary — the served app never touches the network). Acquisition (run by D4.1, any UA that gets woff2):
  1. `curl -A "Mozilla/5.0 Chrome/120" "https://fonts.googleapis.com/css2?family=Nunito:wght@600;800&family=Baloo+2:wght@800&display=swap"` — read the three latin-subset `fonts.gstatic.com/...woff2` URLs from the response.
  2. Download to `assets/fonts/nunito-600-latin.woff2`, `assets/fonts/nunito-800-latin.woff2`, `assets/fonts/baloo2-800-latin.woff2` (latin subset ≈ 20–40 KB each; ≤ 120 KB asserted).
  3. Commit the OFL texts from `github.com/google/fonts` (`ofl/nunito/OFL.txt`, `ofl/baloo2/OFL.txt`) as `assets/fonts/OFL-nunito.txt`, `assets/fonts/OFL-baloo2.txt`.
- **`@font-face` in `input.css`** (absolute URLs — the stylesheet is served at `/tailwind.css`):

  ```css
  @font-face { font-family: "Nunito"; font-style: normal; font-weight: 400 600;
    font-display: swap; src: url("/fonts/nunito-600-latin.woff2") format("woff2"); }
  @font-face { font-family: "Nunito"; font-style: normal; font-weight: 700 900;
    font-display: swap; src: url("/fonts/nunito-800-latin.woff2") format("woff2"); }
  @font-face { font-family: "Baloo 2"; font-style: normal; font-weight: 700 900;
    font-display: swap; src: url("/fonts/baloo2-800-latin.woff2") format("woff2"); }
  ```

- **Fallback stacks:** `font-display` → `Nunito, ui-sans-serif, system-ui, sans-serif`; `font-poster` → `'Baloo 2', Nunito, ui-sans-serif, system-ui, sans-serif`. `font-display: swap` means the kiosk is readable in Segoe for the first frames and poster-faced forever after; the woff2s are same-origin so the swap is imperceptible on the LAN.
- **Serving:** `/fonts` `ServeDir` in `build_router` (`src/server/router.rs`), long `Cache-Control` fine (immutable data). Also precache the three files in `sw.js`'s app shell (stays well under the 6 KB script budget — it is three added strings).
