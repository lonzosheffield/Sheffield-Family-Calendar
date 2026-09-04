VERDICT: FAIL

# QA — TV viewport wave (TV1, TV2, TV3), round 1

**Auditor:** Fable 5 (fresh context, no prior knowledge of this run) · **Date:** 2026-09-03 ·
**Tree:** `main` @ `22585ce` (the worktree was handed to me at the stale `a49a4ca`; re-based onto
`main`'s tip before anything was read or run — `docs/HANDOFF.md` H-BOSS-1 again, five for five).
Audited as the wave `a1c0416..5e578d5` restricted to `50d4a6a`/`f8322dd` (TV1),
`e0a2797`/`28dc76c` (TV2) and `5909314`/`5e578d5` (TV3) — every changed file read in full, plus
`src/client/app.rs`, `src/client/components/tv/{style,model,surface,shell,fixture}.rs`,
`src/client/components/mobile/mod.rs`, `src/server/router.rs` (`build_router`,
`ensure_public_dir_exists`, `public_bundle_present`), `tests/{router_tests,tv_tests}.rs`,
`tests/golden/*`, `migrations/0003_profiles.sql`, `src/server/db.rs::seed_routine_templates`,
`assets/tailwind.css`, and — because the plan's central claim is about a library, not about this
repo — the Dioxus 0.7.10 sources themselves:
`dioxus-server-0.7.10/src/{ssr.rs::render_head, index_html.rs, document.rs}` and
`dioxus-router-0.7.10/src/{components/router.rs, contexts/{router,outlet}.rs}`.
Also read: the built template `target/dx/family-calendar/release/web/public/index.html`.
**Contract:** `docs/design/PLAN_TV_VIEWPORT.md` in full — §1.1–§1.5 (the reasoning), §2.1–§2.5
(the design targets), **§3's task table (the Accept criteria)**, §4.1–§4.5 (on-device
verification and its Pass line), §5 (risks and rollback); `docs/BACKLOG.md` B-1 including the
2026-09-03 VERIFIED block and the new DONE block; `docs/FIRE_TV.md` (Branch A step 4, *Viewport
scale*, *Verifying the kiosk from the PC*, Troubleshooting); `docs/design/DESIGN_DIRECTION.md`
§2–§3; `docs/PLAN.md` §2 (D3′) and §5; `docs/HANDOFF.md` (the round-6 fix close and its gate
record); `docs/RESIDUAL.md` R-1…R-22 (R-4 and R-12 read closely, so a failure could be
classified before it was named).

## Verdict

**FAIL** on two Med. Nothing High is open, and the mechanism TV1 chose is the right one: I
independently confirmed, in `dioxus-server-0.7.10/src/ssr.rs::render_head` (lines 684–700), that
the template's `head_before_title` + `title` + `head_after_title` are written **first** and the
`ServerDocument`'s collected head elements **after** them and before `close_head` — so the kiosk's
`width=1920, user-scalable=no` really is the last `name="viewport"` tag in the production
document, and Chromium's `Document::ShouldOverrideLegacyDescription` lets a later
`kViewportMeta` replace an earlier one. The Boss's seven on-device pixel probes are internally
consistent with a 1920 × 1080 layout viewport to the pixel (`p-[5%]` of 1920 = 96; `border-4` at
x 96–99 and 1820–1823, y 96–99 and 980–983 — every probe lands exactly where that arithmetic puts
it), so **B-1 is genuinely fixed on the family's television.** The phone string is byte-identical
to the one it replaced, the golden files are untouched, and all five gates are green on this box.

What fails is not the fix but its **proof**. The whole thing rests on one property — *ours is the
last viewport meta* — and that property is pinned in neither of the two places the plan promised
it would be. The test suite does not merely omit it: `tests/router_tests.rs:296` asserts
`!body.contains("width=device-width")` for `/tv`, which is **false of the page the television
actually loads** (the `dx` template contributes exactly that string, as `docs/FIRE_TV.md:243-247`
correctly says it does). A maintainer reading the suite would conclude the template meta is gone
(QT-01). And the plan's own named mitigation for that risk — §5 row 2, *"the `dx` template's
`device-width` meta wins over ours … Very low / **High** (the fix would be inert)"*, mitigated by
*"proven on the device by the readout (§4.3)"* — was never executed: the `?keys=1` readout, the
whole point of TV2, was never captured. Yet `docs/BACKLOG.md:86` states *"On-device verification
per `docs/design/PLAN_TV_VIEWPORT.md` §4, recorded by the Boss at merge"* under a **DONE
(TV1–TV3, 2026-09-03)** heading, when §4's own Pass line is *"Pass = all of 4.2, 4.3, 4.4 and
4.5"* and only part of 4.2 was done — no readout (§4.3), no D-pad walk (§4.4), no logcat /
`ws_clients` / phone check (§4.5), and not one post-fix capture committed to
`docs/design/current-state/` as §3's Boss-at-merge line and §4.6 both require (QT-02).

Separately, and reported here because `docs/HANDOFF.md` itself made gate-reporting honesty a
standing rule two sections earlier: the post-wave gate record's **"34 `test result:` lines, 665
passed"** is wrong for this tree. This repo has 29 integration test files plus three unittest
targets plus `Doc-tests` — **33** result lines, not 34 — and my clean `cargo test --features
server -j 4` (exit 0) totals **642 passed, 0 failed**. 665 − 642 = 23 and 34 − 33 = 1, which is
exactly `homeschool_tests` counted a second time from the separate `--test-threads=1` run. See
*Gate output observed on this box*.

Three Lows follow, all in TV2's diagnostic and one in a doc comment.

## Findings

### QT-01 (Med) — the one property the fix depends on in production is untested, and the suite asserts its negation

**Tier/branch:** **O** — `tv/TV1-qa1` (TV1 owns `tests/router_tests.rs`; the fix touches the
`Once` harness shared by every test in that binary, which is why this is not a Sonnet edit).

**Where.** `tests/router_tests.rs:51-53` (the harness leaves `DIOXUS_PUBLIC_PATH` pointing at an
**empty** directory), `tests/router_tests.rs:283-287` (`assert_eq!(metas.len(), 1, …)` for `/tv`),
`tests/router_tests.rs:294-302` (`!body.contains("width=device-width")` and
`!body.contains("initial-scale")` for `/tv`), `tests/router_tests.rs:323-329`
(`assert_eq!(metas.len(), 1, …)` for `/m` and `/mobile`).

**What ships.** With no `index.html` in the public directory, `ServeConfig::new()` falls back to
`IndexHtml::ssr_only()`, whose head is a single space
(`dioxus-server-0.7.10/src/index_html.rs:105-115`). Every route therefore carries exactly one
viewport meta *in the tests* and the assertions above pass. In production `build_router`
(`src/server/router.rs:169`) calls the same `ServeConfig::new()`, which loads the `dx` bundle's
`public/index.html` — and that file, verbatim from
`target/dx/family-calendar/release/web/public/index.html`, carries
`<meta name="viewport" content="width=device-width, initial-scale=1">` in its head before
anything Dioxus writes. So the page the television loads carries **two** viewport metas and
**does** contain both `width=device-width` and `initial-scale`. Three of the suite's assertions
are true only of a document configuration that is never served.

**Why it is wrong.** Plan §5's risk table names *"The `dx` template's `device-width` meta wins
over ours"* as **Very low / High — "the fix would be inert"**, and gives exactly two mitigations:
the browser ordering rule, and *"proven on the device by the readout (§4.3)"*. §4.3 was not run
(QT-02). That leaves the ordering rule with **no guard at all** in the repo, while the suite
positively suggests the template meta does not exist. Plan §1.5's table promises that TV1's tests
are where the viewport policy is "asserted in the SSR tests for free" (§1.4, the row rejecting
server-side head injection); the assertion that was actually written is not the one that matters.
Concretely: a `dx` upgrade that moves its template meta after the Dioxus head block, a Dioxus
upgrade that reorders `render_head`, or a hand-edit of `public/index.html` that appends a
`width=device-width` meta, would each return the family's television to the exact B-1 clipping —
and `cargo test --features server` would stay green.

**Reproduction (no device needed).**
1. `cp target/dx/family-calendar/release/web/public/index.html` into the directory
   `router_tests`' `init_test_env()` sets as `DIOXUS_PUBLIC_PATH`
   (`%TEMP%\familyhub-router-tests-<pid>\public\`), before the binary starts.
2. `cargo test --features server --test router_tests`.
3. `tv_route_pins_the_kiosk_viewport_at_the_render_width` fails on `metas.len() == 1` (it is 2)
   and on `!body.contains("width=device-width")`; `the_phone_routes_keep_the_device_width_viewport`
   fails on `metas.len() == 1` for both `/m` and `/mobile`. Nothing about the shipped page changed
   — only the test harness stopped lying about it.

**Which tests fail to catch it, and why.** All three of TV1's new tests, by construction: they
were specified (plan §3 TV1 Accept (a)/(b)) against an SSR-only index, and §1.2 wrote that
limitation down rather than closing it. `tests/pwa_tests.rs` and `tests/http_tests.rs` assert
manifest/body content, never head order. Nothing anywhere reads
`dioxus_server::IndexHtml`/`ServeConfig` ordering.

**Complete solution to apply verbatim.**

(1) In `tests/router_tests.rs`, above `init_test_env()`, add:

```rust
/// The head shape of the `dx build` bundle's own `public/index.html`
/// (`target/dx/family-calendar/release/web/public/index.html`), reduced to the
/// parts that decide viewport policy: a `<title>`, the template's own
/// `width=device-width, initial-scale=1` meta, and the `id="main"` mount point
/// `dioxus_server::IndexHtml` splits on. The wasm `<script>` is deliberately
/// left out — this binary asserts SSR output, never hydration.
///
/// QT-01 / B-1: production always serves this template
/// (`build_router` → `ServeConfig::new()` → `<public>/index.html`), so the page
/// the television loads carries **two** viewport metas. `dioxus-server`'s
/// `ssr::render_head` writes the template head first and the collected
/// `ServerDocument` head elements after it, before `</head>`, so ours is last —
/// and Chromium applies viewport metas in document order, each replacing the
/// previous description. *That ordering is the whole fix.* Without this file the
/// harness falls back to `IndexHtml::ssr_only()` (empty head) and the ordering
/// is never exercised at all.
const DX_TEMPLATE_VIEWPORT_META: &str = "width=device-width, initial-scale=1";

const DX_TEMPLATE_INDEX_HTML: &str = r#"<!DOCTYPE html>
<html>
    <head>
        <title>Sheffield Family Hub</title>
        <meta content="text/html;charset=utf-8" http-equiv="Content-Type">
        <meta name="viewport" content="width=device-width, initial-scale=1">
        <meta charset="UTF-8">
    </head>
    <body>
        <div id="main"></div>
    </body>
</html>"#;
```

(2) In `init_test_env()`, replace

```rust
        let public = base.join("public");
        std::fs::create_dir_all(&public).expect("test public directory is creatable");
        std::env::set_var("DIOXUS_PUBLIC_PATH", &public);
```

with

```rust
        let public = base.join("public");
        std::fs::create_dir_all(&public).expect("test public directory is creatable");
        // QT-01: serve the same document shape production serves — template
        // head first, Dioxus's collected head elements after it. See
        // `DX_TEMPLATE_INDEX_HTML` above.
        std::fs::write(public.join("index.html"), DX_TEMPLATE_INDEX_HTML)
            .expect("test index.html is writable");
        std::env::set_var("DIOXUS_PUBLIC_PATH", &public);
```

(3) In `tv_route_pins_the_kiosk_viewport_at_the_render_width`, replace the whole block from
`let metas = viewport_meta_contents(&body);` to the end of the function with:

```rust
    let metas = viewport_meta_contents(&body);
    assert_eq!(
        metas,
        vec![
            DX_TEMPLATE_VIEWPORT_META.to_string(),
            TV_VIEWPORT_META.to_string(),
        ],
        "/tv must serve the dx template's viewport meta first and the kiosk's \
         **last**: Chromium applies viewport metas in document order and the last \
         one wins, which is the only reason B-1's fix takes on the television \
         (plan §1.2, §5 risk 2). Got {metas:?}"
    );

    // ...and the winning tag is the kiosk's own: `device-width` is 960 on the
    // Insignia, and `initial-scale` beside `width=1920` would turn the kiosk
    // into a horizontally scrolling page (plan §1.3).
    let winning = metas.last().expect("at least one viewport meta on /tv");
    assert!(
        !winning.contains("width=device-width"),
        "the last viewport meta on /tv must not be a device-width one (B-1), got {winning:?}"
    );
    assert!(
        !winning.contains("initial-scale"),
        "the last viewport meta on /tv must not set initial-scale (plan §1.3), got {winning:?}"
    );
```

(4) In `the_phone_routes_keep_the_device_width_viewport`, replace the
`assert_eq!(metas.len(), 1, …)` / `let content = &metas[0];` pair with:

```rust
        assert_eq!(
            metas.len(),
            2,
            "{path} must carry the dx template's viewport meta and then its own, got {metas:?}"
        );
        assert_eq!(
            metas[0], DX_TEMPLATE_VIEWPORT_META,
            "{path}: the template's meta is expected first"
        );
        let content = metas.last().expect("a viewport meta on the phone route");
```

(leave the `for needle in […]` loop and the `!body.contains(&format!("width={TV_RENDER_WIDTH_PX}"))`
assertion exactly as they are — the template contributes no `1920`, so the body-wide assertion
still holds and is still the stronger one).

(5) Add one test whose name says the property out loud, so a future reader cannot miss it:

```rust
/// B-1's load-bearing property, stated once, plainly. Plan §5's risk table
/// rates "the `dx` template's `device-width` meta wins over ours" *Very low /
/// High — the fix would be inert*, and offers exactly one proof: the on-device
/// readout of §4.3. This is the other one, and it runs on every commit.
#[tokio::test]
async fn the_kiosk_viewport_meta_is_the_last_one_in_the_document() {
    let config = test_config();
    let addr = spawn_router(&config).await;

    let body = http_client()
        .get(format!("http://{addr}/tv"))
        .header("accept", "text/html")
        .send()
        .await
        .expect("GET /tv should respond")
        .text()
        .await
        .expect("response body");

    let ours = body
        .rfind(TV_VIEWPORT_META)
        .expect("/tv must serve TV_VIEWPORT_META");
    let template = body
        .find(DX_TEMPLATE_VIEWPORT_META)
        .expect("/tv must still serve the dx template's own viewport meta");
    assert!(
        template < ours,
        "the kiosk's viewport meta must come **after** the dx template's — the last \
         one wins in Chromium, and B-1's fix is inert if that order ever inverts \
         (template at {template}, kiosk at {ours})"
    );
    let close_head = body.find("</head>").expect("/tv must have a </head>");
    assert!(
        ours < close_head,
        "the kiosk's viewport meta must be inside <head> (Dioxus writes collected \
         head elements before </head>; a meta emitted during streaming would land \
         after it — see dioxus-server ssr::render_head)"
    );
}
```

**Also fold into the same branch:** `src/client/app.rs:137-139` and
`tests/router_tests.rs:198-205` both tell the reader the tests boot "without a template … so each
route carries exactly one". Update both comments to say the harness now ships the template's head
shape *on purpose*, and that the assertion is about **order**, not count.

---

### QT-02 (Med) — B-1 is stamped DONE on an on-device verification that was never performed and is not recorded anywhere

**Tier/branch:** **S** — `tv/TV3-qa1` (`docs/BACKLOG.md`, `docs/FIRE_TV.md`).

**Where.** `docs/BACKLOG.md:57` (`**DONE (TV1–TV3, 2026-09-03).**`) and `docs/BACKLOG.md:86`
(`* On-device verification per docs/design/PLAN_TV_VIEWPORT.md §4, recorded by the Boss at
merge.`).

**What ships.** A DONE block, dated, whose final bullet asserts that §4 verification exists and
was recorded at merge. Nothing in the repository records it. `docs/design/current-state/` contains
nine images, the newest of which are the *pre-fix* `tv-clipped-fullykiosk-adb-2026-09-03.png` and
`tv-fullykiosk-welcome-clipped-2026-09-03.png` (`git log -- docs/design/current-state` stops at
`ae94e87`, the commit *before* the plan). Not one post-fix capture was added by `5909314`,
`5e578d5` or `22585ce`.

**Why it is wrong.** Plan §4 ends with an explicit gate: *"**Pass = all of 4.2, 4.3, 4.4 and
4.5.** Commit the PNGs (or ≤ 110 KB JPEGs, as `docs/qa/` does) and stamp B-1."* Plan §3's
Boss-at-merge line repeats it: *"Commit the on-device screenshots into
`docs/design/current-state/` and stamp the date into B-1's DONE block."* Of that gate:

* **§4.2** — partially done. The seven pixel probes were run and matched, and four rail cards, the
  `Add a phone` pill and the School tab were seen inside the frame. §4.2 item 2 also requires the
  progress bar and `0 / 8` chip inside the card's right edge and three routine rows — the *exact*
  symptom of the owner's photograph — and that was not reported. (The hub ran on a scratch
  database; `src/server/db.rs::seed_routine_templates` does seed the routine, so the rows were
  probably on screen, but "probably" is not a record.)
* **§4.3** — **not done.** The `?keys=1` readout, which is the entirety of TV2's deliverable and
  §5's named mitigation for the wave's only High-impact risk, was never captured.
* **§4.4** — **not done.** No D-pad walk: the join-QR overlay, the Whiteboard mirror at its new
  1200 × 532, the School panel, `scrollIntoView` to row 8 (QD-08) and the screensaver at
  `fixed inset-0` are all untested at the new layout size, and all four are things the fix moves.
* **§4.5** — **not done.** No `adb logcat` sweep for a wasm panic (the QD-11 watch the plan
  explicitly asks for while the whiteboard's backing store quadruples), no `ws_clients`, no phone
  re-check.

So the plan's Pass gate is unmet and the DONE claim is not justified by what is in the repo. This
is precisely the class of error `docs/HANDOFF.md`'s own *"Gate reporting"* paragraph — two
sections above the TV wave's own close — set the standard against.

**Reproduction.** `git log --oneline -- docs/design/current-state` → newest entry `ae94e87`,
pre-fix. `grep -rn "keys=1" docs/design/current-state docs/qa` → nothing. `sed -n '86p'
docs/BACKLOG.md` → the claim.

**Which tests fail to catch it, and why.** `tests/docs_tests.rs` link-checks every `docs/**/*.md`
and pins `FIRE_TV.md`'s `### <n>.` step shape (I confirmed the count is 7 on both `a1c0416` and
`main`), but it cannot know whether a sentence is true. TV3's own Accept (d) asks only that B-1's
body *contains* `DONE (TV1` — a criterion an overclaiming block satisfies as easily as an honest
one.

**Complete solution to apply verbatim.** Replace `docs/BACKLOG.md:86` with:

```markdown
* **On-device verification (per `docs/design/PLAN_TV_VIEWPORT.md` §4) — partial, 2026-09-03.**
  Done: §4.2's geometry. With the merged build served from a scratch-database hub, the television
  rendered `/tv` at 1920 × 1080 with all four rail cards, the `Add a phone` pill and the School tab
  inside the frame, and all seven pixel probes matched — `(48,540)`/`(960,48)` = `#8BB5DA`;
  `(98,540)`/`(1821,540)`/`(960,98)`/`(960,981)` = `#1E293B`; `(140,540)` = `#FFFFFF`. Those
  coordinates are `p-[5%]` of 1920 = 96 px plus `border-4`, so they are a direct measurement of a
  1920 CSS px layout viewport: **B-1's defect is fixed on the panel.**
  **Not yet done, and therefore not claimed:** §4.3 (the `?keys=1` viewport readout was never
  captured — it is §5's named mitigation for the "template meta wins" risk); §4.4 (the D-pad walk:
  join-QR overlay, Whiteboard mirror, School panel, row 8 / QD-08, screensaver); §4.5 (`adb
  logcat` for a wasm panic under the QD-11 watch, `ws_clients`, the phone re-check); and no capture
  was taken against a database with real routine data, so §4.2 item 2's routine rows, progress bar
  and `0 / 8` chip are unconfirmed. No post-fix screenshot is committed to
  `docs/design/current-state/`. **Close this bullet by running §4.3–§4.5 and committing the
  captures the plan names.**
```

and change the heading at `docs/BACKLOG.md:57` from

```markdown
**DONE (TV1–TV3, 2026-09-03).**
```

to

```markdown
**FIXED IN CODE (TV1–TV3, 2026-09-03) — on-device verification partial; see the last bullet.**
```

Finally, add one sentence to `docs/FIRE_TV.md`'s *The viewport readout* paragraph (after line
351): `The readout is the one check on this page nobody has yet run on the television; until it
has been, the fix's reliance on "the last viewport meta wins" rests on the pixel probes alone.`
Do not renumber a `### <n>.` step; `tests/docs_tests.rs` pins the count at 7.

---

### QT-03 (Low) — `viewport_verdict` says "as designed" on any panel that is 1920 wide, including one where the rail budget's 1080 no longer holds

**Tier/branch:** **S** — `tv/TV2-qa1` (`src/client/components/tv/model.rs`, `tests/tv_tests.rs`).

**Where.** `src/client/components/tv/model.rs:285-291`.

**What ships.**

```rust
pub fn viewport_verdict(v: &TvViewport) -> &'static str {
    if v.width_px == TV_RENDER_WIDTH_PX { "as designed" } else { "not the design width — …" }
}
```

Only the width is compared, while `surface.rs:876-889` prints `(target 1920×1080)` beside the
verdict, so the readout advertises a two-dimensional target and grades one dimension.

**Why it is wrong.** The doc comment justifies this with *"the height follows the width once the
WebView's scale is set correctly"* — true only on a 16:9 panel. `docs/FIRE_TV.md` Branch B and
plan §5's last risk row both contemplate a replacement device (a Vega stick, "a real 1920 panel"),
and plan §2.1 keeps 1920 × 1080 as the render target because *"the `?keys=1` readout will show the
same number on the TV that the QA harness showed on the PC."* On a 16:10 or 21:9 panel
`width=1920` yields a layout viewport of 1920 × 1200 or 1920 × 823. In the 823 case
`tv_rail_budget_px()` — which subtracts from `TV_RENDER_HEIGHT_PX` = 1080 — is wrong by 257 px and
the fourth boy and the *Add a phone* pill clip again, i.e. **QD-02's defect returns**, and the one
diagnostic built to catch it reports "as designed". This is the same class of mistake B-1 itself
was: an assumption about the panel that the page does not check.

**Reproduction.** Open `/tv?keys=1` in a desktop Chrome window sized to 1920 × 700 (desktop Chrome
ignores viewport metas, so `innerWidth`/`innerHeight` are the window's). The readout prints
`viewport 1920×700 css px, dpr 1.00 — as designed (target 1920×1080)`. The rail is visibly short of
its budget.

**Which tests fail to catch it, and why.** `viewport_verdict_reads_as_designed_only_at_the_exact_design_width`
(`model.rs:845-874`) varies only the width — 1920, 960, 1919 — and holds `height_px` at 1080 in
every case; `tv_viewport_the_keys_overlay_reports_the_measured_viewport_against_the_design_width`
(`tests/tv_tests.rs:764-796`) uses the two matched pairs 1920 × 1080 and 960 × 540. Neither ever
presents a right width with a wrong height, which is the only input that separates the two
implementations.

**Complete solution to apply verbatim.** In `src/client/components/tv/model.rs`, replace
`viewport_verdict` (and the second sentence of its doc comment) with:

```rust
/// Does the measured viewport match the size every pixel of `/tv` is designed
/// and budgeted for ([`TV_RENDER_WIDTH_PX`] × [`TV_RENDER_HEIGHT_PX`],
/// `src/client/components/tv/style.rs`)?
///
/// **Both** dimensions are judged (QT-03). The width is what the viewport meta
/// pins and what B-1 was about; the height is what `tv_rail_budget_px()`
/// subtracts from, and on a panel that is not 16:9 a correct width can arrive
/// with a height that puts the fourth boy and the *Add a phone* pill back
/// outside the card — QD-02's defect, wearing B-1's clothes. A readout that
/// said "as designed" there would be worse than no readout.
pub fn viewport_verdict(v: &TvViewport) -> &'static str {
    match (
        v.width_px == TV_RENDER_WIDTH_PX,
        v.height_px == TV_RENDER_HEIGHT_PX,
    ) {
        (true, true) => "as designed",
        (true, false) => "the design width, but not the design height — the rail budget \
                          assumes 1080 lines; see docs/FIRE_TV.md, Viewport scale",
        _ => "not the design width — see docs/FIRE_TV.md, Viewport scale",
    }
}
```

and extend the unit test at `model.rs:845` with:

```rust
        // QT-03: the right width with the wrong height is the case that used to
        // read "as designed" while the rail budget's 1080 lines were gone.
        let short = TvViewport {
            width_px: TV_RENDER_WIDTH_PX,
            height_px: 823,
            dpr_centi: 100,
        };
        assert_eq!(
            viewport_verdict(&short),
            "the design width, but not the design height — the rail budget \
             assumes 1080 lines; see docs/FIRE_TV.md, Viewport scale"
        );
```

and, in `tests/tv_tests.rs`, inside
`tv_viewport_the_keys_overlay_reports_the_measured_viewport_against_the_design_width`, after the
`960×540` case:

```rust
    // QT-03: right width, wrong height — the readout must not say "as designed".
    model.viewport = Some(TvViewport {
        width_px: 1920,
        height_px: 823,
        dpr_centi: 100,
    });
    let html = render(&model);
    assert!(html.contains("1920×823"), "{html}");
    assert!(!html.contains("as designed"), "{html}");
    assert!(html.contains("not the design height"), "{html}");
```

No new class token is introduced, so `assets/tailwind.css` stays byte-identical.

---

### QT-04 (Low) — the readout is a mount-time snapshot presented as a live reading

**Tier/branch:** **S** — `tv/TV2-qa1` (`src/client/components/tv/surface.rs`, `tests/tv_tests.rs`,
`docs/FIRE_TV.md`).

**Where.** `src/client/components/tv/shell.rs:485-494` (`viewport.set(platform::viewport())` inside
`onmounted`, once, with no listener — correct per plan §3 TV2(3)) and
`src/client/components/tv/surface.rs:876-889`, which renders it as
`viewport 1920×1080 css px, dpr 2.00 — …` with nothing saying *when*.

**What ships.** A value read once, after `event.set_focus(true).await` resolves, and never
refreshed. On the television that is right and the plan is right to forbid a resize listener (§1.5
pins `the_kiosk_never_reaches_for_a_pointer_event`). But plan §2.1 states the readout's *other*
job explicitly: *"the `?keys=1` readout will show the same number on the TV that the QA harness
showed on the PC"* — and the QA harness is a desktop browser window, which resizes. There the
readout silently keeps reporting the size the window had when the page mounted, and
`viewport_verdict` renders a confident verdict on a stale number.

**Why it is wrong.** `docs/FIRE_TV.md:346-352` tells the Boss to treat a wrong readout as evidence
of a WebView setting or a stale binary — a diagnostic whose stale case is indistinguishable from
its true case is worse than one that admits its limits, and this is the instrument §4.3 exists to
read.

**Reproduction.** `/tv?keys=1` in Chrome at 1920 × 1080 → `as designed`. Resize the window to
1200 × 800 without reloading → the HUD still reads `viewport 1920×1080 css px … as designed`.

**Which tests fail to catch it, and why.** Nothing can: the SSR tests feed `TvModel.viewport`
directly and never model a second measurement. The fix is in the *wording*, which is testable.

**Complete solution to apply verbatim.** In `src/client/components/tv/surface.rs`, replace the body
of `viewport_readout`:

```rust
fn viewport_readout(viewport: Option<TvViewport>) -> String {
    match viewport {
        Some(v) => {
            let dpr = v.dpr_centi as f64 / 100.0;
            format!(
                "viewport at mount {}×{} css px, dpr {dpr:.2} — {} (target {TV_RENDER_WIDTH_PX}×{TV_RENDER_HEIGHT_PX}; reload after a resize)",
                v.width_px,
                v.height_px,
                viewport_verdict(&v),
            )
        }
        None => "viewport: not measured yet".to_string(),
    }
}
```

Extend the existing assertion in `tests/tv_tests.rs` (the 1920 × 1080 case) with:

```rust
    assert!(html.contains("viewport at mount"), "{html}");
    assert!(html.contains("reload after a resize"), "{html}");
```

and update `docs/FIRE_TV.md:347-348` to quote the new string:
`read the HUD's viewport line: it must say` `viewport at mount 1920×1080 css px, dpr 2.00 — as
designed (target 1920×1080; reload after a resize)`. (While there: "the HUD's **first line**" is
not quite true — at `TV_BODY_TEXT` = 30 px inside `w-[38rem]` minus `p-8` ≈ 544 px of content the
sentence wraps to two lines. "the HUD's viewport line" is both shorter and correct.)

---

### QT-05 (Low) — `TvViewport`'s doc comment justifies `dpr_centi` with a claim that is false of `TvModel`

**Tier/branch:** **S** — `tv/TV2-qa1` (`src/client/components/tv/model.rs`, comment only).

**Where.** `src/client/components/tv/model.rs:268-271`:

> `dpr_centi` is `devicePixelRatio * 100`, rounded to an integer, so this struct — and therefore
> [`TvModel`] — can stay `PartialEq` (a bare `f64` field cannot derive `Eq`, and `TvModel` is
> compared in tests).

**What ships.** Two wrong statements in one parenthesis. An `f64` field derives `PartialEq`
perfectly well — it is `Eq` it cannot derive — and `TvModel` is **not** `Eq`:
`src/client/components/tv/model.rs:310` is `#[derive(Clone, PartialEq, Debug)]`. So the stated
reason ("`TvModel` can stay `PartialEq`") does not follow from anything, and a reader who trusts
it will believe `TvModel: Eq` and be surprised.

**Why it matters.** `style.rs` and `model.rs` are the two files in this component whose comments
are load-bearing — `docs/design/DESIGN_DIRECTION.md` §3.6 now lists `TV_VIEWPORT_META` among the
things "worth keeping as-is", and the reasons *why* live in these comments. The integer is still
the right choice; only the argument for it is wrong. (`docs/qa/QA_HS_ROUND_6.md` QH6-04 filed the
same class of finding, so it has precedent here.)

**Reproduction.** `grep -n "^#\[derive" src/client/components/tv/model.rs` → line 310 shows
`TvModel` deriving `Clone, PartialEq, Debug`, no `Eq`. Adding `pub dpr: f64` to `TvViewport` (with
`Eq` dropped from `TvViewport`'s own derive) compiles.

**Which tests fail to catch it, and why.** None can — it is a comment. It is filed because it is
the only thing in the wave that would mislead the next person to touch this struct.

**Complete solution to apply verbatim.** Replace `src/client/components/tv/model.rs:268-271` with:

```rust
/// `dpr_centi` is `devicePixelRatio * 100`, rounded to an integer, rather than
/// a bare `f64`: an exact integer compare is what a diagnostic wants (`2.00`
/// either is or is not the Insignia's 2 dppx), floats do not implement `Eq`,
/// and `NaN != NaN` would make a model carrying one unequal to itself in the
/// `assert_eq!`s of `tests/tv_tests.rs`. [`TvModel`] itself derives
/// `PartialEq` only, and keeps doing so.
```

---

## Gate output observed on this box

Shell preamble on every command (PowerShell): `PATH` (cargo / scoop / npm), `RUST_BACKTRACE=1`,
`FAMILY_HUB_DATA_DIR=%TEMP%\familyhub-tvqa`, `FAMILY_HUB_REFUSE_SYSTEM_DIR=1`.
`C:\ProgramData\FamilyHub` was never opened, listed or resolved by anything run here; the
`FamilyHub` service was not stopped, started or reinstalled; no `adb` command was issued and the
Fire TV was not touched; port 8085 was left alone; **no two `cargo test` suites ran concurrently**
(the two clippy invocations are builds and ran while I read, never beside a test suite).

| Gate | Result |
| --- | --- |
| `cargo fmt --check` | **exit 0**, no output |
| `cargo clippy --features server --all-targets -j 4 -- -D warnings` | **exit 0** — `Finished dev profile` |
| `cargo clippy --features web --target wasm32-unknown-unknown -j 4 -- -D warnings` | **exit 0** — `Finished dev profile ... in 1m 25s` |
| `cargo test --features server -j 4` (one run, alone) | **exit 0** — **33 `test result:` lines, 642 passed, 0 failed, 1 ignored**; zero `FAILED` lines |
| `cargo test --features server -j 4 --test homeschool_tests -- --test-threads=1` (run 1) | **exit 0** — `test result: ok. 23 passed; 0 failed; 0 ignored; … finished in 1.00s` |
| …(run 2, immediately after) | **exit 0** — `test result: ok. 23 passed; 0 failed; 0 ignored; … finished in 0.87s` |
| `tailwindcss -i input.css -o %TEMP%\tvqa_tailwind.css --minify` then SHA-256 vs `assets/tailwind.css` | **byte-identical** — 20 991 bytes both, hashes equal (rebuilt to a temp path so the tracked file was never written) |
| `git diff --stat a1c0416 HEAD -- tests/golden` | **empty** — `tests/golden` genuinely unchanged across the whole wave |
| `grep -c 'name: "viewport"' src/client/app.rs` | **2**, and `App` contains none (TV1 Accept (f)) |
| `grep -c "^### [0-9]\. " docs/FIRE_TV.md` on `a1c0416` and on `HEAD` | **7 and 7** — no step renumbered (TV3 Accept (b)); `head -1` is still `STATUS: FIRE_OS` |

The wave's own new tests, all green and named individually in the run:

```
test client::components::tv::model::tests::viewport_verdict_reads_as_designed_only_at_the_exact_design_width ... ok
test client::components::tv::style::tests::the_tv_viewport_meta_pins_the_render_width ... ok
test tv_route_pins_the_kiosk_viewport_at_the_render_width ... ok
test the_phone_routes_keep_the_device_width_viewport ... ok
test the_phone_route_still_links_the_manifest_at_its_root_url ... ok
test tv_viewport_the_keys_overlay_reports_the_measured_viewport_against_the_design_width ... ok
```

**Confirmed** from the Boss's report: `cargo fmt --check`, both clippy gates, the byte-identical
Tailwind rebuild, `cargo test --features server -j 4` **exit 0**, and `homeschool_tests
--test-threads=1` **23 passed** (twice, consecutively).

**Refuted:** *"34 `test result:` lines and 665 passed"*. This tree has 29 files in `tests/`, three
unittest targets (`src/lib.rs` 309 passed, `src/main.rs` 0, `src/bin/family_hub.rs` 0) and one
`Doc-tests` line — **33** `test result:` lines, and they sum to **642 passed, 0 failed**. The
per-binary tally, in run order: 309, 0, 0, 11, 15, 7, 4, 9, 11, 17, 9, 33, 1, 2, 26, 1, 23, 14, 1,
6, 7, 6, 16, 18, 15, 10, 5, 5, 9, 8, 41, 3, 0. There is no 34th target for cargo to run.
665 − 642 = 23 and 34 − 33 = 1, which is exactly the `homeschool_tests` binary (23 passed) counted
twice — once inside the full run and once from the separate `--test-threads=1` run. Nothing failed
either way, so the verdict is unaffected; but `docs/HANDOFF.md`'s post-wave gate record should be
corrected to **33 lines / 642 passed**, with the serial `homeschool_tests` 23 reported as its own
line, since that same document made accurate gate reporting a standing rule one section earlier.

**No test failed on this box, so no R-4 / R-12 classification was needed.** For the record, I read
both before running anything: R-4's six catalogued load flakes live in `realtime_tests`,
`loop_tests`, `backup_tests`, `homeschool_db_tests` and `service_tests`, and R-12's is in
`curriculum_tests`; every one of those binaries was green here (`realtime_tests` 18,
`loop_tests` 14, `backup_tests` 11, `homeschool_db_tests` 26, `service_tests` 5,
`curriculum_tests` 4).

## Regression risk — phone, whiteboard, screensaver, focus ring, hydration

**The phone PWA — no regression, and I checked it rather than trusting the commit message.**
`git show a1c0416:src/client/app.rs` lines 79–82 hold
`width=device-width, initial-scale=1, viewport-fit=cover, user-scalable=no`; `app.rs:181` holds the
same 68 bytes. `viewport-fit=cover` therefore still reaches `pb-[env(safe-area-inset-bottom)]` in
`mobile/mod.rs:244`, and both `/m` and `/mobile` render `Mobile` (via `MobileShort`), so both get
it. The manifest `Link` (`app.rs:66-69`, `pwa::MANIFEST_PATH`), the `apple-touch-icon` link, the
three `apple-mobile-web-app-*` metas and `theme-color` all stayed global in `App` — the R-16 hazard
(a hashed `asset!()` manifest URL putting `start_url: "/m"` out of scope) is untouched, and
`pwa_tests` (6) and `http_tests` (14) are green. The service worker carries no HTML of its own
(`grep -rn "viewport\|device-width"` over every `.rs`/`.js`/`.css`/`.html` in the tree returns
nothing outside `app.rs`, `tv/`, and doc comments), so there is no cached shell holding a stale
meta.

**The whiteboard.** Untouched by the wave. Plan §5 predicts `sync_device_pixel_ratio` will now
allocate a 2400 × 1064 backing store for a 1200 × 532 CSS canvas — ≈ 10 MB, harmless, and strokes
are normalised 0..1 so a phone's drawing still lands in the same place. That prediction is sound
but **unverified on the device**: §4.4's Whiteboard step was never walked, and §4.5's `logcat`
sweep for the unreproduced QD-11 `ResizeObserver` panic — which the plan asks for precisely
*because* the canvas grew — was not run. Folded into QT-02.

**The screensaver.** Rendered inside `KioskDashboard` (`app.rs:146`) beside `TvShell`, so it is
inside the route that now carries the meta; there is no route on which the screensaver renders
without one. `fixed inset-0` follows the layout viewport, which is now 1920 × 1080, and the
caption chip's `bottom-[5%] left-[5%]` is the same 96 px inset as the frame. One thing worth the
Boss's eye at §4.4: screensaver photos are re-encoded at `MAX_DIMENSION` ≤ 1600 px
(`src/server/api/photos.rs`), so a full-bleed photo is now upscaled to 1920 rather than displayed
at ~960. Expected, cosmetic, and named in plan §2.4(c).

**The focus ring and D-pad traversal.** `TV_FOCUSABLE_CLASS`, `TV_FOCUS_RING_ACTIVE/IDLE`,
`focus_class`, `TV_OVERSCAN_CLASS`, `TV_POSTER_CARD_CLASS` and every rail constant are unedited;
`tests/golden/tv_focus_order.txt` and `tv_type_scale.txt` are byte-identical
(`git diff --stat a1c0416 HEAD -- tests/golden` empty). TV2's readout is a `<p>` with no
`data-tv-focus`, no size class and no new colour token (`.text-slate-200` already exists in
`assets/tailwind.css`), so `t2_1_a_*`, `t2_1_f_*` (which renders every golden model with
`keys_debug = true`, so a size class on the readout *would* trip it) and
`the_kiosk_never_reaches_for_a_pointer_event` all pass unmodified — all 41 `tv_tests` green. The
readout is gated on `model.keys_debug` at `surface.rs:120-121`, so it never renders outside
`?keys=1`. The remaining risk is entirely §4.4's unrun D-pad walk (QT-02).

**Hydration.** Sound, and checked in the library rather than taken from the doc comment.
`dioxus-server`'s `ServerDocument::create_head_component` writes a `true` into the hydration stream
for each head element it rendered and `dioxus-web`'s `FullstackWebDocument` skips re-creating
those, so the eight head elements `App` + `KioskDashboard` emit in a fixed order are matched
one-for-one on the client — the same path the `apple-mobile-web-app-*` metas already took before
this wave. TV2's `viewport` signal is `None` on the server *and* on the first client frame
(`shell.rs:125`, `platform::viewport()` SSR stub at `shell.rs:591`), and is only set after
`event.set_focus(true).await` resolves inside `onmounted`, so nothing about it can mismatch.

**Client-side route changes: none exist.** I grepped `src/client/` for `Route::Home`,
`Route::Tv`, `Route::Mobile`, `navigator()` and `Link {` — the only hit is
`window.navigator().service_worker()` in `pwa.rs:179`. So the "two route-scoped metas alive in one
head after an in-app navigation" case cannot arise; every page load is a fresh document, and the
"stale after a route change" half of the TV2 measurement question is moot.

**Unmatched routes.** `Route` has no catch-all variant, and `dioxus-router-0.7.10`'s
`RouterContext::current` (`contexts/router.rs:274-280`) calls `dioxus_core::throw_error` on a parse
failure, so a typo'd URL renders the error boundary and **no** route component — hence no
Dioxus-emitted viewport meta. In production the `dx` template still supplies
`width=device-width, initial-scale=1`, which is what such a page effectively got before TV1 as
well, so there is no regression; on a hub whose `public/` has no `index.html` a 404 page now
carries no viewport meta at all. **Observed, not filed** — a 404 on a LAN family hub is not a
surface anyone reads, and it is the same class of page in both worlds.

**"Last wins", checked properly, and the one ordering under which ours could lose.** Blink's
`Document::ShouldOverrideLegacyDescription(origin)` returns
`origin >= legacy_viewport_description_.type`, and two ordinary `<meta name="viewport">` tags are
both `kViewportMeta`, so the second **replaces** the first: ours wins. The single exception is
`Document::ShouldMergeWithLegacyDescription`, gated on `Settings::ViewportMetaMergeContentQuirk` —
an Android-WebView compatibility quirk enabled only for host apps targeting very old SDK levels.
Under it the second meta *merges* into the first instead of replacing it, so
`width=1920, user-scalable=no` layered onto `width=device-width, initial-scale=1` would inherit
**`initial-scale=1`** and produce exactly the horizontally-scrolling 960-px window onto a 1920-px
page that plan §1.3 calls "worse than today". The Boss's pixel probes prove that quirk is *not*
active in this WebView (the frame and card land where a plain 1920 layout puts them, not where a
scrolled one would), so this is a live-fire pass, not a defect — but it is the ordering under
which ours could lose, it is invisible to `curl`, and the readout of §4.3 is the only cheap way to
see it. Worth one sentence in `docs/FIRE_TV.md` beside the "last one wins" paragraph when QT-02 is
closed.

## Observed, not filed (no contract clause is failed)

* **`/tv` from a phone.** `build_http_router` upgrades only `/m*`, `/manifest.webmanifest` and
  `/sw.js` to HTTPS, so a parent can open `http://<hub>:8080/tv` and now gets the whole poster at
  ~0.2 scale, unzoomable (`user-scalable=no`). That is strictly better than before — the old global
  meta carried `user-scalable=no` too, and at `device-width` the page was clipped rather than
  merely small — so there is nothing to fix. Not a supported surface.
* **`user-scalable=no` on the TV.** Cargo-culted, on the plan's own admission ("There is no touch
  on the TV anyway; this is belt-and-braces"), and §1.3's claim that it "pins minimum = maximum =
  initial" is a Blink implementation detail I could not confirm. It is harmless either way and the
  unit test pins it, so leaving it is right.
* **The `?keys=1` HUD's height.** The readout adds ~2 wrapped lines at 30 px to an `aside` that
  already stacks two `h2`s, the key log and 13 map rows past 1080 CSS px. Pre-existing overflow,
  debug-only, and the readout is at the top where §4.3 needs it.
* **`the_tv_viewport_meta_pins_the_render_width`** uses
  `contains(&format!("width={TV_RENDER_WIDTH_PX}"))`, so a hypothetical `TV_RENDER_WIDTH_PX = 192`
  would match `"width=1920"`. Not reachable from any real edit; noted only so the next reader does
  not re-derive it.
* **`viewport_meta_contents`** (`tests/router_tests.rs:213-261`) is a genuinely careful little
  parser — real attribute-boundary check, both quoting styles, unquoted values, order-independent,
  and it terminates on every input. I could not construct a page on which it over- or under-counts.
  Good work; it is the assertion built on top of it that QT-01 is about, not the helper.

## Is B-1 fixed, and is the DONE block justified?

**B-1 is genuinely fixed.** The mechanism is right (route-scoped `document::Meta`, verified against
the Dioxus 0.7.10 collection path rather than taken on trust), the constant is right
(`width=1920`, no `initial-scale`, so the WebView scales to fit on 960-, 1280- and 1920-px panels
alike), the phone is byte-for-byte untouched, and the television has been measured at a 1920 × 1080
layout viewport by seven independent pixel probes whose coordinates are derivable from
`TV_OVERSCAN_CLASS` and `TV_POSTER_CARD_CLASS` alone. The owner's *"feel like if it were on a
monitor"* is met on the geometry that was checked.

**The DONE block is not justified as written.** It is headed `DONE (TV1–TV3, 2026-09-03)` and
closes with a bullet asserting on-device verification "recorded by the Boss at merge", while the
plan's own Pass gate — §4.2 **and** §4.3 **and** §4.4 **and** §4.5 — is unmet, §4.3 (the readout,
which is §5's named mitigation for the wave's only High-impact risk) was never captured, no
post-fix image exists anywhere in `docs/design/current-state/`, and no capture was taken against a
database with real routine data — so the specific composition item the owner photographed, the
first routine row overflowing the card's right edge, has not been re-observed. QT-02 gives the
honest replacement text. The 960 × 540 acceptance-sentence note, by contrast, **is** honest: it
states plainly that the original acceptance line is satisfied differently, says how, and cites
plan §1.5's last paragraph as its source. That paragraph is the model the rest of the block should
have followed.

## Notes for the fixing wave

Take QT-01 first and alone: it changes a `Once` harness that every test in `router_tests` shares,
so run `cargo test --features server --test router_tests` before and after, then the full suite once
by itself. QT-02 is pure documentation and can ride with QT-04's `FIRE_TV.md` wording change on
`tv/TV3-qa1`. QT-03 and QT-04 both edit `model.rs`/`surface.rs`/`tv_tests.rs` and should be one
`tv/TV2-qa1` branch; neither introduces a class token, so `git diff --exit-code assets/tailwind.css`
after the rebuild must stay empty. Prove each new assertion **red before the fix** — that is the
standard `docs/HANDOFF.md` set at the round-6 close, and it is the only thing that would have
caught QT-01 in the first place.
