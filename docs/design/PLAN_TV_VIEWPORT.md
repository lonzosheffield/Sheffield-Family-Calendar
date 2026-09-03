# Plan — the kiosk at monitor scale on the Insignia Fire TV (BACKLOG B-1)

**Author:** Fable 5.1 (planner, fresh context) · **Date:** 2026-09-03 · **Base:** `main` @ `ae94e87` (the commit that recorded the B-1 root cause) · **Status:** plan only, awaiting the Boss's approval.

**The problem and the target, in one paragraph.** The television's WebView lays the kiosk out at roughly 960 CSS px wide (`wm size` override `1920x1080`, `wm density` 320 = 2 dppx), while every pixel of `/tv` is designed, budgeted and tested at `TV_RENDER_WIDTH_PX × TV_RENDER_HEIGHT_PX` = 1920 × 1080 CSS px (`src/client/components/tv/style.rs`). `src/client/app.rs:79-82` hands every route one viewport meta — `width=device-width, initial-scale=1, viewport-fit=cover, user-scalable=no` — so on this panel `device-width` is 960 and the whole poster paints at 2×: one rail card, the first routine row cut at the card's right edge, the `0 / 8` chip severed (`docs/design/current-state/tv-clipped-fullykiosk-adb-2026-09-03.png`, and the owner's photo `tv-routine-clipped-insignia-2026-09-02.jpg`). Fully Kiosk is not involved (its own welcome page clips identically; it was not installed when the owner first photographed the fault). The target is the owner's words — *"feel like if it were on a monitor"* — which in this codebase has a precise meaning: the television must show exactly the composition the design QA signed off at a true 1920 × 1080 CSS viewport (`docs/design/current-state/tv-after-d4.3.jpg`, `docs/design/qa/round2-*.jpg`): the blue frame on all four sides, the white card with its thin dark border, four boys plus *Add a phone* on the rail, the routine rows and the footer pills inside the card, crisp type, and a focus ring a boy can see from the sofa — with every existing `/tv` acceptance test still green and the phone surface untouched.

---

## 1. The decision

### 1.1 What we do: a **route-scoped viewport meta**, `width=1920`, emitted by the kiosk route itself

`document::Meta` in Dioxus 0.7.10 is not tied to the root component. It is a `use_hook` that calls `Document::create_meta` on whichever `Document` provider is in context (`dioxus-document-0.7.10/src/elements/meta.rs`). On the server that provider is `ServerDocument`, which **collects** every meta rendered during the first SSR frame into a `Vec` and writes them into `<head>` after the index template's own head and before `</head>` (`dioxus-server-0.7.10/src/document.rs`, `render`). It does not matter which component rendered the meta, only that it rendered in the first frame, outside any suspense boundary. Both `/tv` routes (`Tv`, `Home`) render `KioskDashboard` synchronously; `TvShell`'s `use_resource`s do not suspend. So a meta placed in `KioskDashboard` lands in the head of `/tv` and `/`, and a meta placed in `Mobile` lands in the head of `/m` and `/mobile`. **The Router renders exactly one route, so the page carries exactly one Dioxus-emitted viewport meta.** No document-level effect, no server-side head injection, no JS shim is needed.

Hydration is safe by construction: `ServerDocument::create_head_component` writes a `true` into the hydration stream for every head element it rendered, and on the wasm side `FullstackWebDocument::create_head_component` returns `!head_element_written_on_server()`, so the client's `Meta` hook sees the element already exists and does **not** append a second one (`dioxus-fullstack-core-0.7.10/src/document.rs`, `dioxus-web-0.7.10/src/document.rs`). This is the same path the existing `apple-mobile-web-app-*` metas already take on every load today.

**The change, concretely** (all in `src/client/app.rs`; the constant in `tv/style.rs`):

```rust
// src/client/components/tv/style.rs — beside TV_RENDER_WIDTH_PX
/// The `/tv` viewport meta (B-1). `width=<TV_RENDER_WIDTH_PX>` makes any
/// WebView lay the kiosk out at the width the budget below is computed for
/// and scale the result to its panel (960 CSS px at 2 dppx on the Insignia:
/// scale 0.5; a 1280-px stick: 0.667; a real 1920 panel: 1). No
/// `initial-scale`, on purpose — see docs/design/PLAN_TV_VIEWPORT.md §1.3.
pub const TV_VIEWPORT_META: &str = "width=1920, user-scalable=no";
```

```rust
// src/client/app.rs
#[component]
fn KioskDashboard() -> Element {
    rsx! {
        document::Meta { name: "viewport", content: TV_VIEWPORT_META }
        div { class: "relative h-full w-full bg-sheffield-paper font-display text-slate-800",
            TvShell {}
            Screensaver {}
        }
    }
}

#[component]
pub fn Mobile() -> Element {
    rsx! {
        document::Meta {
            name: "viewport",
            content: "width=device-width, initial-scale=1, viewport-fit=cover, user-scalable=no",
        }
        MobileShell {}
    }
}
```

…and the global `document::Meta { name: "viewport", … }` at `App` (`app.rs:79-82`) is **deleted**. The phone string is byte-identical to today's, so `/m` and `/mobile` (which renders `Mobile`) see no change at all; `pb-[env(safe-area-inset-bottom)]` in `mobile/mod.rs` keeps its `viewport-fit=cover`.

### 1.2 One fact the Boss must know before reading the served HTML

The `dx build` bundle's `public/index.html` — the one beside `family-hub.exe` in `C:\Program Files\FamilyHub\public\` and in `target/dx/family-calendar/release/web/public/` — carries **its own** `<meta name="viewport" content="width=device-width, initial-scale=1">` in the template head, ahead of everything Dioxus collects. Production `/tv` therefore already serves *two* viewport metas today (the template's, then `App`'s), and after this change it will serve the template's followed by ours. Chromium applies viewport metas in document order, each one replacing the previous description, so **the last one wins** — ours. Two consequences:

* `curl http://<hub-ip>:8080/tv` after the merge will show two `name="viewport"` tags; that is expected. The one that counts is the last, `width=1920, user-scalable=no`.
* The integration tests (`tests/router_tests.rs`) boot the router without a `public/index.html` (`ServeConfig::new()` falls back to `IndexHtml::ssr_only()`, whose head is empty), so in tests each route carries **exactly one** viewport meta, and that is what the tests assert.

If — and only if — the on-device readout (§4 step 3) reports a 960-wide layout with the meta present, the fallback is to strip the template's viewport meta server-side: in `build_router`, load `<public>/index.html` the way `ensure_public_dir_exists` already resolves the directory, drop the `<meta name="viewport" …>` tag, and pass `ServeConfig::with_index_html(IndexHtml::new(&stripped, "main"))` instead of `ServeConfig::new()`. It is ~20 lines in `src/server/router.rs` and is **not** in this plan's task table because the browser rule makes it unnecessary; it is written down so nobody has to rediscover it.

### 1.3 `initial-scale`, `viewport-fit`, `user-scalable`

* **No `initial-scale` on `/tv`.** With `width=1920` alone Chromium computes the initial scale to fit the layout width into the screen: 960 / 1920 = 0.5 on this panel. `initial-scale=1` together with `width=1920` would instead show a 960-px window onto a 1920-px page — a horizontally scrolling kiosk, worse than today. `initial-scale=0.5` would hard-code this panel's 2 dppx and render a DPR-1 or DPR-1.5 replacement (a Vega stick at 720p/1080p) at the wrong size. `width=1920` adapts to all of them and to a real 1920-px panel (scale 1).
* **Height follows.** The visual viewport at scale 0.5 is 960 × 540 screen-CSS px, so the layout viewport becomes 1920 × 1080 and `min-h-screen` (100vh) on the surface root is 1080 — the exact number `tv_rail_budget_px()` is computed from.
* **`viewport-fit=cover` is dropped from the TV meta.** It only affects notch/home-indicator insets; a television has none, and the phone keeps it.
* **`user-scalable=no` stays.** Android WebView honours it (Chrome only overrides it under the accessibility "force enable zoom" setting), and it pins `minimum-scale = maximum-scale = initial`, so the kiosk cannot drift off 0.5. There is no touch on the TV anyway; this is belt-and-braces.

### 1.4 Rejected alternatives, and why they lose

| Candidate | Why it loses |
| --- | --- |
| **B-1 #2 — adaptive `zoom` / `transform: scale(innerWidth / 1920)` on the root, re-run on `resize`** | Needs wasm code on mount and a resize listener where today there is none; CSS `zoom` changes how `%`, `vh` and `getBoundingClientRect` resolve (the whiteboard's `css_size()` and the QD-08 `scrollIntoView` both read them); `transform: scale` leaves the layout viewport at 960 so `min-h-screen`, `p-[5%]` and `fixed inset-0` (the screensaver) all size to the *unscaled* viewport and have to be re-expressed in fixed pixels. It re-implements, badly, what the viewport meta is for. |
| **B-1 #3 — Fully Kiosk "Initial scale" / "Desktop mode"** | Device configuration, so Silk (Branch B), a replacement stick and any future `dx serve` on a laptop keep the bug; D2′ makes the kiosk shell a runbook choice with zero code impact and the fix belongs in the page. "Desktop mode" also swaps the UA and lays out at a fake desktop width, which is the wrong width. |
| **Server-side head injection (custom `IndexHtml`)** | Works, but it is the fallback of §1.2, not the fix: it adds a second place viewport policy lives and duplicates `dx`'s template handling. Route-scoped `document::Meta` keeps the policy beside the surface that needs it and is asserted in the SSR tests for free. |
| **A JS shim (`document.head.appendChild(meta)` on load)** | The head is available at SSR time; appending later means one frame at 2× before the re-layout, and it bypasses Dioxus's hydration bookkeeping. |
| **Reset the `wm size` override to render at 3840 × 2160** | Not an app change, and not a good idea for this panel — §2.4. |

### 1.5 What must not move — the acceptance assertions this touches, and how they stay true

Every test below is in `tests/tv_tests.rs` unless named otherwise. **None is weakened, deleted or skipped; no contract amendment is needed.**

| Assertion | Touched by | Stays true because |
| --- | --- | --- |
| `t2_1_a_the_rendered_focus_order_matches_the_golden_file`, `…every_focusable_element_carries_a_visible_focus_ring`, `…exactly_one_element_wears_the_live_ring…` and `tests/golden/tv_focus_order.txt` | TV2 (adds a `<p>` to the `?keys=1` HUD) | The HUD is not focusable and gains no `data-tv-focus`; `document::Meta` renders `VNode::empty()` into the body. Golden file byte-identical (`git diff --exit-code tests/golden`). |
| `t2_1_f_every_rendered_font_size_is_on_the_committed_allowlist` (renders every model with `keys_debug = true`) and `tests/golden/tv_type_scale.txt` | TV2 | The readout carries **no** `text-<size>` class; it inherits the HUD's `TV_BODY_TEXT`. |
| `t2_1_f_every_heading_clears_forty_four_pixels` | TV2 | The readout is a `<p>`, never an `h1`–`h3`. |
| `t2_1_f_every_full_screen_container_carries_the_five_percent_overscan`, `d4_3_c_the_frame_is_the_overscan_band_and_the_card_is_the_only_one`, `qd_02_the_poster_card_and_the_rail_wear_the_measured_spacing` | nobody | `screen_class`, `TV_OVERSCAN_CLASS`, `TV_POSTER_CARD_CLASS` and the rail classes are not edited. |
| `qd_02_the_rail_budget_at_1080p_holds_four_boys_and_the_phone_pill` and `style.rs::the_rail_holds_four_boys_and_the_phone_pill_at_ten_eighty` (`budget == 612`, `needed == 600`, target `(1920, 1080)`) | TV1 (doc comment only) | The numbers do not change. What changes is their *precondition*: the 1920 × 1080 viewport the budget assumes is now **enforced** by `TV_VIEWPORT_META` rather than assumed from `docs/device.toml`. TV1 says so in the comment above `TV_RENDER_WIDTH_PX` and pins the two together with a unit test (`TV_VIEWPORT_META` contains `width=1920` built from the constant). |
| `t2_1_f_the_kiosk_has_no_hover_only_affordance`, `the_kiosk_never_reaches_for_a_pointer_event` (grep `tv/**` + `screensaver.rs`) | TV2 (`shell.rs`) | The new platform function reads `innerWidth`/`innerHeight`/`devicePixelRatio` and adds no listener; no `onclick:`/`onpointerdown:`/`hover:` anywhere. |
| `hs6_*`, `d4_3_a/b/g`, `qd_08_*`, `t2_1_b/c/d/e` | nobody | Model logic, navigation and markup outside the HUD are untouched. |
| `tests/glyph_tests.rs` (all of it) | nobody | It renders the phone routine row, the screensaver and the School tab; none of those files is in any task's Owns list. |
| `tests/router_tests.rs::tv_route_serves_the_kiosk_dashboard`, `m_route_serves_the_phone_routine_view`; `tests/pwa_tests.rs::the_phone_page_links_the_manifest_at_its_root_url` | TV1 (adds tests beside them) | Body markers and the root-URL manifest link are unchanged; the manifest `Link` stays global in `App`. |
| `tests/docs_tests.rs` (link checker over every `docs/**/*.md`; FIRE_TV `### <n>.` step shape; Branch A/B/B′ headings) | TV3 | TV3 adds a `####` subsection and a troubleshooting row, never renumbers a step, and cites only files that exist. |

One backlog line, not a test, deserves an honest note: B-1's own acceptance sentence asks for "a headless Chrome run at 960 × 540 DPR 2 … or a `#[test]` computing the layout budget for that viewport". With the meta in place there is no 960-px layout to budget for — the layout viewport is 1920 on every device — so that sentence is satisfied by (a) the SSR assertion that `/tv` pins `width=1920`, and (b) the on-device readout in §4 that proves the WebView honoured it. TV3 records exactly that in B-1's DONE block.

---

## 2. Design targets — what "feels like a monitor" means on this panel

The panel: Insignia NS-50F301NA22, 50" 16:9 (≈ 43.6" × 24.5" of picture), 3840 × 2160 native, driven by Fire OS at a 1920 × 1080 framebuffer. After the fix **1 CSS px = 1 framebuffer px = 0.577 mm = a 2 × 2 block of panel pixels.** Viewing distance for the family: about 10 ft (3 m).

### 2.1 Logical width: stay at 1920 CSS px

1920 × 1080 is the right logical size, and not only because everything is already built for it: it is the framebuffer the device actually composes (`wm size` override), it is the size every design-QA measurement was taken at (`docs/qa/QA_DESIGN_ROUND_2.md` §3: card 96→984, rail 259→872, canvas 1200 × 532), and the `?keys=1` readout (TV2) will show the same number on the TV that the QA harness showed on the PC. A 1280-px "TV-dp" grid (Android TV's convention) would mean re-deriving the type scale, the budget and both golden files for no visible gain. **Decision: `TV_RENDER_WIDTH_PX` stays 1920, and the meta enforces it.**

### 2.2 Type scale: unchanged, and here is the arithmetic that says it is right

At 0.577 mm per CSS px and 3 m: `text-3xl` 30 px body = 17.3 mm em ≈ 20 arcmin; `text-4xl` 36 px row titles ≈ 24 arcmin; `text-5xl` 48 px ≈ 32 arcmin; `text-6xl` 60 px ≈ 40 arcmin. Comfortable reading needs roughly 15–20 arcmin per em and the 10-foot guidelines converge on ≥ 24 px at 1080p; D8's ≥ 28 px floor and the four-size scale (`tests/golden/tv_type_scale.txt`) are therefore at or above the comfortable floor for the boys, and the eyebrow/wordmark lockup reads across the room. **No size changes.** The `.poster-outline` text-shadow (`input.css`, "2 px at 1920 × 1080") also becomes correct for the first time on the device — today it paints at 4 framebuffer px.

### 2.3 Overscan and the safe area: the 5 % frame is right, and this panel does not crop

The owner's photo and the adb capture agree: the blue band is visible on all four sides and the corner balls at `bottom-[1.6%] left/right-[1.6%]` are fully on screen, so **the Insignia shows the whole 1920 × 1080 framebuffer with no overscan crop in this mode.** `TV_OVERSCAN_CLASS` = `p-[5%]` (96 px) therefore stays what D4.3 made it — the poster's frame, not a safety margin the panel is eating — and every pixel of content stays inside the card, inside the frame. If a replacement television does crop (typical 2–5 %), the card is still inside it. **No change.**

### 2.4 Crispness: what the "2× device pixels" actually buy, and why we do not chase them

There are no spare device pixels for the page to use. The `wm size 1920x1080` override means SurfaceFlinger composes at 1080p and the television's own scaler upscales to 3840 × 2160; the WebView never rasterises at 4K. After the fix the page rasterises at an effective scale of exactly 1.0 (device scale factor 2.0 × page scale 0.5), i.e. one framebuffer pixel per CSS pixel, then the panel upscales — which is precisely what a 1080p source looks like on a 4K screen, and precisely what the owner asked for. Fonts are hinted at their designed sizes, the 4-px card border is 4 framebuffer px, the 8-px ring is 8.

True 4K rendering would need `adb shell wm size reset` (framebuffer 3840 × 2160 at density 320 → `device-width` = 1920 at 2 dppx). It is worth noting that in that configuration the original bug would never have existed; and that `width=1920` yields the identical 1920 × 1080 layout there too, so the fix is robust to either device state. But resetting the override is: (a) a device setting outside the page, which D2′ keeps at zero code impact; (b) 4× the compositor fill and 4× the WebView raster memory on a 2020-era TV SoC that Amazon deliberately runs at 1080p; (c) no gain for the screensaver, whose photos are re-encoded at ≤ 1600 px (`MAX_DIMENSION`, `src/server/api/photos.rs`) and would be upscaled anyway; (d) of unknown persistence across reboots. **Decision: leave the override alone.** If the Boss ever wants to see it, it is a one-line experiment with a one-line rollback (`adb shell wm size 1920x1080`), and the readout of §4 will report `1920×1080 … dpr 2.00` in both states — which is the point.

### 2.5 What the fix does to the things a boy actually looks at

* **Focus ring** — `ring-8 ring-offset-4 ring-offset-sheffield-dark ring-sheffield-sun`: 8 + 4 CSS px = 6.9 mm of sun-on-dark at 3 m, ≈ 8 arcmin of colour band. Today it paints at 16 + 8 framebuffer px and still looks fine; at the designed size it is what QA round 2 photographed. Unchanged.
* **The rail** — four 112-px boys + 20-px gaps + the 72-px *Add a phone* pill = 600 px in a 612-px rail: all visible, nothing to scroll, exactly `tv_rail_needed_px(4) ≤ tv_rail_budget_px()`.
* **The routine list** — three rows visible, the rest reached by Down with `scrollIntoView` (QD-08); row 8 lands inside the list on the seventh press as measured in round 2.
* **The whiteboard mirror** — the canvas goes from ~350 px tall to the 1200 × 532 CSS px of the QA measurement; strokes are normalised 0..1 so a phone's drawing lands in the same place.
* **The screensaver** — `fixed inset-0` becomes 1920 × 1080; the caption chip at `bottom-[5%] left-[5%]` sits on the frame's inset, inside the safe area.
* **Emoji** — the photo proves Noto Color Emoji is present on this Fire OS build (suns, balls, toothbrush all rendered); no fallback needed.

---

## 3. Task table

House convention: **O** = Opus, **S** = Sonnet. Tasks share no file. **All three run in parallel**; the Boss merges TV1 → TV2 → TV3 (TV3 cites the other two). Every shell starts with the mandatory environment line (`FAMILY_HUB_DATA_DIR` under `%TEMP%`, `FAMILY_HUB_REFUSE_SYSTEM_DIR=1`); no agent touches the television, the service, or `C:\ProgramData\FamilyHub`; nobody runs two `cargo test` suites at once.

| ID | Tier | Owns | Do | Accept |
| --- | --- | --- | --- | --- |
| **TV1** — the kiosk pins its own viewport | **O** | `src/client/app.rs` · `src/client/components/tv/style.rs` · `tests/router_tests.rs` | (1) `style.rs`: add `pub const TV_VIEWPORT_META: &str = "width=1920, user-scalable=no";` beside `TV_RENDER_WIDTH_PX`, with a doc comment that says the budget's 1920 × 1080 precondition is now enforced by this meta (B-1), and a unit test in the existing `mod tests` that `TV_VIEWPORT_META` contains `format!("width={TV_RENDER_WIDTH_PX}")`, contains no `initial-scale`, and no `device-width`. (2) `app.rs`: delete the global viewport `document::Meta` from `App`; render `document::Meta { name: "viewport", content: TV_VIEWPORT_META }` as the first node of `KioskDashboard` (so `/tv` and `/` both carry it); render the phone meta — byte-identical to today's string — as the first node of `Mobile` (so `/m` and `/mobile` both carry it). Doc comments cite B-1, the Dioxus collection/hydration path (§1.1) and the `dx` template meta + last-wins rule (§1.2). (3) `router_tests.rs`: three new tests using the existing `test_config()` / `spawn_router()` helpers. | (a) `GET /tv` (accept `text/html`) → 200 and the body contains **exactly one** `name="viewport"` meta; that tag's `content` equals `TV_VIEWPORT_META` (parse the tag, do not assume attribute order); the body contains neither `width=device-width` nor `initial-scale`. (b) `GET /m` and `GET /mobile` each → 200 with exactly one viewport meta whose `content` contains `width=device-width`, `initial-scale=1` and `viewport-fit=cover`; neither body contains `width=1920`. (c) `GET /m` still contains `href="/manifest.webmanifest"` (the manifest link stayed global) — asserted in the same test or by `pwa_tests` staying green. (d) `tv_route_serves_the_kiosk_dashboard` and `m_route_serves_the_phone_routine_view` unchanged and green. (e) `git diff --exit-code tests/golden` empty; `cargo test --features server` green (full suite, run once alone); `cargo fmt --check`; `cargo clippy --features server --all-targets -- -D warnings`; `cargo clippy --features web --target wasm32-unknown-unknown -- -D warnings`. (f) `grep -c 'name: "viewport"' src/client/app.rs` = 2 and `App` contains none. |
| **TV2** — `?keys=1` reports the viewport the WebView actually gave us | **S** | `src/client/components/tv/model.rs` · `src/client/components/tv/surface.rs` · `src/client/components/tv/shell.rs` · `src/client/components/tv/fixture.rs` · `tests/tv_tests.rs` | (1) `model.rs`: `#[derive(Clone, Copy, PartialEq, Eq, Debug)] pub struct TvViewport { pub width_px: u32, pub height_px: u32, pub dpr_centi: u32 }` (dpr × 100, integer so the model stays `PartialEq`); `TvModel` gains `pub viewport: Option<TvViewport>` (`empty()` → `None`; `fixture.rs::canonical_model` → `None`); `pub fn viewport_verdict(v: &TvViewport) -> &'static str` returning `"as designed"` when `v.width_px == TV_RENDER_WIDTH_PX`, else `"not the design width — see docs/FIRE_TV.md, Viewport scale"`. (2) `surface.rs::keys_overlay`: directly under the `Key codes` `h2`, a `p { id: "tv-viewport-readout", class: "text-slate-200", … }` reading `viewport 1920×1080 css px, dpr 2.00 — as designed (target 1920×1080)` from the model, or `viewport: not measured yet` when `None`. No size class, no new colour class, no new element type. (3) `shell.rs`: `platform::viewport() -> Option<TvViewport>` (wasm: `window.inner_width()/inner_height()` as f64 → u32, `device_pixel_ratio()` × 100 rounded; SSR stub → `None`); a `use_signal(|| None::<TvViewport>)` set once inside the existing `onmounted` handler after `set_focus` (so SSR and the first client frame agree and hydration sees no mismatch); pass it into the model as `viewport`. No listeners added. | (a) New test `tv_viewport_the_keys_overlay_reports_the_measured_viewport_against_the_design_width`: with `keys_debug = true` and `viewport = Some(1920, 1080, 200)` the rendered HTML contains `id="tv-viewport-readout"`, `1920×1080`, `dpr 2.00` and `as designed`; with `Some(960, 540, 200)` it contains `960×540` and `not the design width`; with `None` it contains `not measured yet`; with `keys_debug = false` it contains no `tv-viewport-readout`. (b) Unit assertions on `viewport_verdict` for 1920 and 960 (and 1919 → not as designed). (c) `t2_1_a_*` (focus order golden byte-identical), `t2_1_f_*` (allowlist with `keys_debug = true`; heading minimum; overscan; hover grep), `the_kiosk_never_reaches_for_a_pointer_event`, `d4_3_*`, `qd_02_*`, `qd_08_*`, `hs6_*` all green **unmodified**. (d) `the_key_code_debug_overlay_is_off_unless_keys_equals_one` unchanged and green. (e) Same gates as TV1 (e): full suite once, fmt, both clippies; `git diff --exit-code tests/golden` empty; no new class token appears in `assets/tailwind.css`'s rebuild (`tailwindcss -i input.css -o assets/tailwind.css --minify` leaves `git diff --exit-code assets/tailwind.css` empty). |
| **TV3** — the runbook and the backlog tell the truth | **S** | `docs/FIRE_TV.md` · `docs/BACKLOG.md` · `docs/design/DESIGN_DIRECTION.md` | (1) `FIRE_TV.md`, under the existing `#### Viewport scale - the confirmed cause of docs/BACKLOG.md B-1` (step 4): a paragraph stating the fix — `/tv` now serves `width=1920, user-scalable=no` (`TV_VIEWPORT_META`), what that does at 2 dppx (scale 0.5, 1920 × 1080 CSS), that `curl` will show the `dx` template's `device-width` meta *before* it and the last one wins, and that the phone routes keep `device-width`. Add to the step-4 table the two Android-WebView settings that could defeat a viewport meta, phrased conditionally because their exact labels in Fully Kiosk 1.61.2 are unverified: *Web Content Settings → "Use Wide Viewport" (if present) → on; "Desktop Mode" → off; "Initial Scale" → 0 / default*. Add to **Verifying the kiosk from the PC** the readout check (`/tv?keys=1` → `viewport 1920×1080 css px, dpr 2.00 — as designed`) and the pixel-probe recipe of §4.2. Add one **Troubleshooting** row: *Kiosk is huge and clipped (one rail card, text off the card's edge)* → *the page is laying out at `device-width`: the running binary predates TV1, or a WebView setting is ignoring the viewport meta* → *`/tv?keys=1` readout; reinstall the service; check the two step-4 rows*. Never renumber a `### <n>.` step; never cite a file that does not exist. (2) `BACKLOG.md` B-1: a **DONE (TV1–TV3, 2026-09-0N)** block in the B-3 style — mechanism, files, the tests that pin it, the honest note about the 960 × 540 acceptance sentence (§1.5 last paragraph), and a line *"on-device verification per `docs/design/PLAN_TV_VIEWPORT.md` §4, recorded by the Boss at merge"*. (3) `DESIGN_DIRECTION.md` §2.7: one sentence after the QD-02 amendment — the 1920 × 1080 render target is enforced on the television by the `/tv` viewport meta (`tv::style::TV_VIEWPORT_META`), not assumed from `docs/device.toml`; and in §3.6 add the viewport meta to the list of things worth keeping as-is. | (a) `cargo test --features server --test docs_tests` green (link checker over all `docs/**/*.md`; FIRE_TV headings and step shape; every backticked `docs/*.md` / `.toml` in the runbook set exists). (b) `docs/FIRE_TV.md` line 1 still `STATUS: FIRE_OS`; `grep -c "^### [0-9]\. " docs/FIRE_TV.md` unchanged from `main`. (c) `docs/FIRE_TV.md` contains the strings `TV_VIEWPORT_META`, `width=1920`, `tv-viewport-readout` or `?keys=1`, and `last one wins`; the Troubleshooting table gains exactly one row. (d) `docs/BACKLOG.md` B-1 heading unchanged and its body contains `DONE (TV1`. (e) `DESIGN_DIRECTION.md` §2.7 contains `TV_VIEWPORT_META`. (f) Diff touches only the three owned files. |

**Boss, at the merge (in this order):** merge TV1, TV2, TV3; run the Tailwind rebuild with fail-on-diff (R-5's lesson — no new classes are expected, so the diff must be empty); `dx build --platform web --release`; elevated reinstall of the `FamilyHub` service with the fresh binary and `public/` bundle (the owner queue already notes the stale service binary); then §4. Commit the on-device screenshots into `docs/design/current-state/` and stamp the date into B-1's DONE block.

---

## 4. On-device verification (Boss, live over adb)

Preconditions: the merged binary is what the service is running (`http://<hub-ip>:8080/health` → `"db": true`; `curl http://<hub-ip>:8080/tv` shows `width=1920, user-scalable=no` as the **last** `name="viewport"` meta — a preceding `width=device-width, initial-scale=1` from the `dx` template is expected, §1.2). Reload the kiosk without the remote: `adb shell am force-stop de.ozerov.fully` then `adb shell monkey -p de.ozerov.fully -c android.intent.category.LAUNCHER 1` (Fully loads its Start URL on launch).

### 4.1 Capture

```powershell
adb exec-out screencap -p > docs\design\current-state\tv-viewport-routine-adb-2026-09-0N.png
```

Repeat after each step below with `-today`, `-whiteboard`, `-school`, `-rail-add-a-phone`, `-join-qr`, `-keys`, `-screensaver` suffixes. Every capture is a 1920 × 1080 PNG of the framebuffer — the same pixels the panel upscales.

### 4.2 Pass criteria for the routine capture (measurable, not eyeballed)

1. **Frame and card geometry.** Probe pixels with `Add-Type -AssemblyName System.Drawing; $b=[System.Drawing.Bitmap]::FromFile("<png>"); $b.GetPixel(x,y)`:
   * `(48, 540)` and `(960, 48)` ≈ `#8BB5DA` (`sheffield-light`, the frame) — tolerance ± 8 per channel;
   * `(98, 540)`, `(1821, 540)`, `(960, 98)`, `(960, 981)` ≈ `#1E293B` (`slate-800`): the card's `border-4` occupies x 96–99 and y 96–99, and x 1820–1823 / y 980–983;
   * `(140, 540)` ≈ `#FFFFFF` (the card).
   Any of these off by more than the tolerance means the layout is not 1920 wide.
2. **Composition** (compare with `docs/design/qa/round2-tv-dashboard-1080p.jpg`): the wordmark `SHEFFIELD / ☀️ Morning Routine ☀️ Isaiah` on one row with `updated HH:MM` at the right; **four** rail cards (Isaiah, Nathaniel, Simeon, Ezekiel) **and** the `📱 Add a phone` pill, all inside the card; the progress bar and the `0 / 8` chip entirely inside the card's right edge; three routine rows with glyph → checkbox → text; the four footer pills (`Morning Routine · Today · Whiteboard · School` — the owner's older screenshots predate School) inside the card; the four balls in the bottom corners on the blue band; **no** horizontal scrollbar, no text crossing the border.
3. **Failure signatures.** One rail card + "Wake up / and thank" wrapped and cut = still `device-width` (binary not reinstalled, or a WebView setting — §3 TV3 rows). Everything tiny with the blue frame filling most of the screen = the meta is being applied twice / the template strip fallback is not needed but something else is off — read the `?keys=1` readout first.

### 4.3 The readout

Open `http://<hub-ip>:8080/tv?keys=1` (Fully → Settings → Start URL temporarily, or Fully's remote admin `loadUrl`). The HUD's first line must read `viewport 1920×1080 css px, dpr 2.00 — as designed (target 1920×1080)`. `960×540` here with the meta present in `curl` output is the one outcome that triggers the §1.2 fallback (strip the template meta) — after first checking Fully's Web Content Settings. Restore the Start URL to `/tv` afterwards.

### 4.4 The D-pad walk (`adb shell input keyevent`: 19 Up · 20 Down · 21 Left · 22 Right · 23 Enter/Center · 4 Back · 85 Play/Pause)

| Presses | Pass |
| --- | --- |
| Down × 4 | The sun ring sits on `Add a phone`, fully visible at the bottom of the rail (QD-02's premise, finally true on the device). |
| Enter, then Back | The join-QR overlay fits inside the card (heading, sentence, 320-px code, URL, Back — nothing on the frame); Back returns the ring to `Add a phone`. |
| Up × 4, Right | Today panel; rail unchanged; `Nothing on the calendar today.` or the day's rows inside the card. |
| Right | Whiteboard: the mirror canvas fills the panel body inside its `ring-sheffield-light` frame; draw a stroke from a phone and it appears in proportion. |
| Right | School panel (or `No school plan for Isaiah`), rows inside the card. |
| Right (wraps to Routine), Enter, Enter | Ring enters the list on row 1; Enter stamps it (blue box, rotated tick), `1 / 8`. |
| Down × 7 | Row 8 is fully inside the list (QD-08). Up × 7 returns to row 1. |
| Play/Pause, wait, Play/Pause | Screensaver covers the whole panel; the `☀️ Sheffield Family Hub` chip sits bottom-left inside the 5 % inset; the second press dismisses it. |

### 4.5 Health, logs, phone

* `adb logcat -d | Select-String -Pattern "chromium|RuntimeError|panicked"` → no wasm panic across the walk (QD-11 watch).
* `/health` → `ws_clients` ≥ 1 while the kiosk is up.
* On a parent phone (CA installed): `https://<hub-ip>:8443/m` — six tabs, tab bar above the home indicator, nothing resized; `curl -k https://<hub-ip>:8443/m | Select-String viewport` → `width=device-width, … viewport-fit=cover`.
* Optional but cheap: one `adb reboot`; the kiosk comes back at the correct scale because the policy lives in the page, not the device.

**Pass = all of 4.2, 4.3, 4.4 and 4.5.** Commit the PNGs (or ≤ 110 KB JPEGs, as `docs/design/qa/` does) and stamp B-1.

---

## 5. Risks and rollback

| Risk | Likelihood / impact | Mitigation |
| --- | --- | --- |
| **Phone regression** — `/m` loses `device-width` or `viewport-fit=cover` | Low / High (the PWA's tab bar and iPhone safe-area rely on it) | The phone string is byte-identical and asserted for both `/m` and `/mobile` by TV1 (b); the manifest `Link` and the `apple-*` metas stay global in `App`. |
| **The `dx` template's `device-width` meta wins over ours** | Very low / High (the fix would be inert) | Chromium replaces the viewport description per meta in document order; ours is emitted last. Proven on the device by the readout (§4.3); fallback documented in §1.2 (strip via `ServeConfig::with_index_html`), ~20 lines in `router.rs`. |
| **The WebView ignores viewport metas** (Fully's "wide viewport" off, "Desktop mode" on) | Low / High | TV3's conditional step-4 rows; readout distinguishes it from a stale binary. Fully's own welcome page clipping the same way tells us the WebView is in its default (meta-honouring) mode today. |
| **Hydration duplicate or mismatch** | Very low / Medium (a second meta would still be last-wins; a mismatch would log a warning) | `Meta` in the first SSR frame, outside suspense; the fullstack document skips client re-creation for server-written head elements (§1.1). TV2's readout is set in `onmounted`, so SSR and first client frame both render `not measured yet`. |
| **Whiteboard canvas** — `devicePixelRatio` still reports 2 under a 0.5 page scale, so `sync_device_pixel_ratio` allocates a 2400 × 1064 backing store for a 1200 × 532 CSS canvas | Certain / Low (≈ 10 MB, crisp, harmless) | Nothing to do; strokes are normalised 0..1. Watch logcat for the unreproduced QD-11 `ResizeObserver` panic during §4.4. |
| **Screensaver** | None expected | `fixed inset-0` follows the layout viewport; the chip's `bottom-[5%] left-[5%]` is the same 96 px inset as the frame. Verified in §4.4. |
| **Focus ring / D-pad** | None expected | Ring geometry, `scrollIntoView` and the pure `on_key` transition are untouched; §4.4 walks every panel. |
| **Type-scale allowlist trips on TV2's readout** | Low / Low (test fails, fix is a class removal) | Spec forbids a size class on the readout; `t2_1_f` renders with `keys_debug = true` and catches it. |
| **`assets/tailwind.css` drift** (R-5) | Low | No new utility tokens in any task; the Boss runs the fail-on-diff rebuild at the merge. |
| **Replacement television** (Vega stick, 1.33–1.5 dppx, or a real 1920 panel) | n/a | `width=1920` without `initial-scale` scales to fit on all of them; the readout reports `1920×1080` wherever the panel is 16:9. |

**Rollback.** `git revert` of TV1's commit restores the single global meta exactly as it is today (the phone never changed, the TV goes back to 2×); TV2 and TV3 are purely additive and can stay. Nothing on the television was configured, so there is nothing to undo on the device — reinstall the previous service binary and the kiosk is where it was.

---

## Appendix — evidence read for this plan

`docs/BACKLOG.md` B-1 (with the 2026-09-03 VERIFIED block) · `docs/FIRE_TV.md` (Branch A incl. the *Viewport scale* subsection, verification and troubleshooting) · `docs/design/DESIGN_DIRECTION.md` §2–§3 · `docs/PLAN.md` §2 D1/D2′/D3′/D8, §5 · `docs/qa/QA_DESIGN_ROUND_1.md` QD-02, `QA_DESIGN_ROUND_2.md` §2–§3 (the 1080p measurements) · `docs/RESIDUAL.md` R-4/R-5/R-12 · `docs/HANDOFF.md` H-20/H-21 and the round-4/5 closes · `src/client/app.rs` · `src/client/components/tv/{style,shell,surface,model,fixture,mod}.rs` · `src/client/components/{screensaver,whiteboard}.rs` · `src/client/components/mobile/mod.rs` (viewport-fit note) · `src/server/router.rs` (`serve_dioxus_application`, `ensure_public_dir_exists`) · `tests/{tv_tests,glyph_tests,router_tests,pwa_tests,docs_tests}.rs` · `tests/golden/*` · `input.css`, `tailwind.config.js`, `Dioxus.toml`, `docs/device.toml` · the built `public/index.html` (template viewport meta) · Dioxus 0.7.10 sources: `dioxus-document/src/elements/meta.rs`, `dioxus-server/src/{document,index_html,config}.rs`, `dioxus-fullstack-core/src/document.rs`, `dioxus-web/src/document.rs` · images: `tv-clipped-fullykiosk-adb-2026-09-03.png`, `tv-routine-clipped-insignia-2026-09-02.jpg`, `tv-after-d4.3.jpg`, `tv-routine.jpg`, `tv-whiteboard.jpg`.
