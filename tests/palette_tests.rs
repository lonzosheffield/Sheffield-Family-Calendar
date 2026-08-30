//! T3.4 acceptance — palette-faithful polish (PLAN v2 §3 T3.4, PURPLE §P3
//! T3.4).
//!
//! The contract, verbatim: *"A `#[test]` asserts: every foreground/background
//! token pair in the palette meets **WCAG AA** (≥ 4.5:1 body, ≥ 3:1 large),
//! **computed in Rust from the hex values**; the type scale uses **≤ 6
//! sizes, all ≥ 28 px on `/tv`**; every `/tv` container carries the **5 %
//! overscan** padding class; **no `hover:`-only affordance** appears in a
//! `/tv` component (grep assertion)."*
//!
//! Four assertions, and this file makes each of them structural rather than
//! declarative:
//!
//! * **(a) contrast.** The maths and the pair table live in
//!   `client::components::palette` and are unit-tested there against the
//!   WCAG reference values. Here the *rendered* `/tv` markup is walked with a
//!   nesting-aware scanner, so every element knows the ground it is actually
//!   sitting on — inherited through however many wrappers — and every
//!   ink/ground pair the kiosk paints is required to be in the table **and**
//!   to clear the AA floor its declared `size` names: 4.5:1 for `Body`, and
//!   3.0:1 for `Large`, of which Phase 4 declares exactly one (the wordmark
//!   word "Morning", asserted below). A pair that appears in the markup but not in the table
//!   fails; a pair in the table that no longer clears AA fails. The profile
//!   discs, whose ground is an inline `background-color` from the `profiles`
//!   row rather than a class, are checked straight from their hex.
//! * **(b) type scale.** ≤ 6 sizes, every one ≥ 28 px, asserted on the
//!   committed allowlist, on `tv::style::TV_TYPE_SCALE`, and on every size
//!   token that reaches the rendered markup.
//! * **(c) overscan.** Every full-screen `/tv` container in the markup — and
//!   every one in the source — carries `TV_OVERSCAN_CLASS`.
//! * **(d) no hover.** Grep of every `/tv` source file and of the rendered
//!   markup of every panel and overlay.
//!
//! Plus the guard that keeps (a) honest over time: a scan of every source
//! file under `components/**` that fails on **any** colour utility whose
//! token is not in `palette::PALETTE_TOKENS`. Without it a new
//! `text-slate-400` could be added to a component that no golden model
//! renders and nothing would notice.
//!
//! QA round 1 (Q1-15) widened that scan from `tv/**` + `mobile/**` to the
//! whole of `components/**`: `/m` renders `routine.rs`, `calendar.rs` and
//! `whiteboard.rs`, and five sub-AA classes had been living in that gap
//! (`text-red-500` 3.76:1, `text-sheffield-accent` on paper 3.11:1, white on
//! `bg-sheffield-accent` 3.17:1, `text-sheffield-light` 2.16:1, white on the
//! `bg-sheffield-light` discs 2.16:1).

#![cfg(feature = "server")]

use dioxus::prelude::*;

use family_calendar::client::components::palette::{
    contrast_ratio, is_colour_name, pair_contrast, pair_meets_wcag_aa, resolve, split_token, token,
    Pair, Rgb, TextSize, COLOURLESS_TOKENS, NON_TEXT_PAIRS, PALETTE_PAIRS, PALETTE_TOKENS,
};
use family_calendar::client::components::tv::fixture::canonical_model;
use family_calendar::client::components::tv::model::{TvModel, TvOverlay, TvPanel};
use family_calendar::client::components::tv::style::{
    TV_MIN_BODY_PX, TV_MIN_HEADING_PX, TV_OVERSCAN_CLASS, TV_TYPE_SCALE,
};
use family_calendar::client::components::tv::surface::TvSurface;

// ---------------------------------------------------------------------------
// Rendering the kiosk
// ---------------------------------------------------------------------------

fn render(model: &TvModel) -> String {
    let model = model.clone();
    dioxus::ssr::render_element(rsx! { TvSurface { model } })
}

/// Every panel, the join overlay, and the `?keys=1` overlay — the same set
/// `tests/tv_tests.rs` calls the golden models, plus the debug HUD, which is
/// the only dark surface on the kiosk and therefore the only place the
/// on-dark ink pairs appear.
fn rendered_sections() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for panel in TvPanel::ALL {
        let mut model = canonical_model();
        model.state.panel = panel;
        out.push((format!("panel:{}", panel.slug()), render(&model)));

        let mut debug = canonical_model();
        debug.state.panel = panel;
        debug.keys_debug = true;
        out.push((format!("panel:{}+keys", panel.slug()), render(&debug)));
    }
    // The disconnected badge only renders when the hub is out of touch.
    let mut stale = canonical_model();
    stale.connected = false;
    stale.stale = true;
    out.push(("panel:routine+disconnected".to_string(), render(&stale)));

    let mut overlay = canonical_model();
    overlay.state.overlay = TvOverlay::JoinQr;
    out.push(("overlay:join-qr".to_string(), render(&overlay)));
    out
}

// ---------------------------------------------------------------------------
// A nesting-aware HTML scanner
// ---------------------------------------------------------------------------
//
// `tests/tv_tests.rs` has a *flat* tag scanner, which is all a focus-order
// assertion needs. Contrast needs the tree: the ink on a `<span>` is read
// against whatever ancestor last set a background. No HTML crate is added for
// this — `Cargo.toml` is not T3.4's to edit (PURPLE §P4) and the input is
// markup this crate produced two lines earlier.

/// The background an element sits on.
#[derive(Clone, Debug, PartialEq)]
enum Ground {
    /// A Tailwind class, e.g. `bg-sheffield-light/25`.
    Class(String),
    /// An inline `background-color: #rrggbb` — the profile discs.
    Inline(Rgb),
}

/// One rendered `(ink class, ground)` combination.
#[derive(Clone, Debug)]
struct Painted {
    tag: String,
    ink: String,
    ground: Ground,
}

/// HTML elements that never have a closing tag.
const VOID: [&str; 14] = [
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// Walk `html`, tracking the background each element inherits, and return
/// every ink/ground pair it paints.
///
/// `<svg>` subtrees are skipped whole: the join-QR code is injected as raw
/// markup by `fast_qr` and paints with `fill` attributes, not classes.
fn painted_pairs(section: &str, html: &str) -> Vec<Painted> {
    let chars: Vec<char> = html.chars().collect();
    let mut stack: Vec<(String, Option<Ground>)> = Vec::new();
    let mut out = Vec::new();
    let mut i = 0usize;

    while i < chars.len() {
        if chars[i] != '<' {
            i += 1;
            continue;
        }
        // `</name>`
        if chars.get(i + 1) == Some(&'/') {
            let end = find_gt(&chars, i);
            stack.pop();
            i = end + 1;
            continue;
        }
        // `<!doctype`, `<!--`, `<?`
        if matches!(chars.get(i + 1), Some('!') | Some('?')) {
            i = find_gt(&chars, i) + 1;
            continue;
        }
        let end = find_gt(&chars, i);
        let inner: String = chars[i + 1..end].iter().collect();
        let self_closing = inner.trim_end().ends_with('/');
        let name = inner
            .split([' ', '\t', '\n', '/', '>'])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();

        if name == "svg" {
            i = skip_subtree(&chars, end, "svg");
            continue;
        }

        let classes = attr(&inner, "class").unwrap_or_default();
        let inherited = stack.last().and_then(|(_, ground)| ground.clone());
        let own = own_ground(&classes, attr(&inner, "style").as_deref());
        let ground = own.or(inherited);

        for class in classes.split_whitespace() {
            // A `focus:`/`active:` colour is conditional; the unprefixed
            // classes are what the television paints.
            if class.contains(':') {
                continue;
            }
            let Some(rest) = class.strip_prefix("text-") else {
                continue;
            };
            if !is_colour_name(rest.split('/').next().unwrap_or(rest)) {
                continue;
            }
            let ground = ground.clone().unwrap_or_else(|| {
                panic!("[{section}] <{name}> paints `{class}` with no ground anywhere above it")
            });
            out.push(Painted {
                tag: name.clone(),
                ink: class.to_string(),
                ground,
            });
        }

        if !self_closing && !VOID.contains(&name.as_str()) {
            stack.push((name, ground));
        }
        i = end + 1;
    }
    out
}

fn find_gt(chars: &[char], from: usize) -> usize {
    let mut i = from;
    let mut quote: Option<char> = None;
    while i < chars.len() {
        match (quote, chars[i]) {
            (None, '"') | (None, '\'') => quote = Some(chars[i]),
            (Some(q), c) if c == q => quote = None,
            (None, '>') => return i,
            _ => {}
        }
        i += 1;
    }
    chars.len() - 1
}

/// Index just past `</name>`, starting from the `>` of its opening tag.
fn skip_subtree(chars: &[char], open_end: usize, name: &str) -> usize {
    let needle: Vec<char> = format!("</{name}").chars().collect();
    let mut i = open_end + 1;
    while i + needle.len() <= chars.len() {
        if chars[i..i + needle.len()] == needle[..] {
            return find_gt(chars, i) + 1;
        }
        i += 1;
    }
    chars.len()
}

fn attr(inner: &str, name: &str) -> Option<String> {
    let mut rest = inner;
    while let Some(at) = rest.find(name) {
        let before_ok = at == 0
            || rest[..at]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_whitespace());
        let after = &rest[at + name.len()..];
        if before_ok {
            if let Some(value) = after.strip_prefix("=\"") {
                return value.split('"').next().map(str::to_string);
            }
        }
        rest = after;
    }
    None
}

/// The background this element sets itself, if any.
fn own_ground(classes: &str, style: Option<&str>) -> Option<Ground> {
    if let Some(style) = style {
        if let Some(at) = style.find("background-color:") {
            let value = style[at + "background-color:".len()..]
                .split(';')
                .next()
                .unwrap_or("")
                .trim();
            if let Some(rgb) = Rgb::parse(value) {
                return Some(Ground::Inline(rgb));
            }
        }
    }
    classes
        .split_whitespace()
        .filter(|class| !class.contains(':'))
        .find(|class| class.starts_with("bg-") && resolve(class).is_some())
        .map(|class| Ground::Class(class.to_string()))
}

// ---------------------------------------------------------------------------
// (a) WCAG AA on every foreground/background token pair
// ---------------------------------------------------------------------------

#[test]
fn t3_4_a_every_declared_palette_pair_meets_wcag_aa() {
    assert!(
        !PALETTE_PAIRS.is_empty() && !NON_TEXT_PAIRS.is_empty(),
        "the palette contract is empty"
    );
    for pair in PALETTE_PAIRS {
        let ratio = pair_contrast(pair)
            .unwrap_or_else(|| panic!("`{}` on `{}` does not resolve", pair.ink, pair.ground));
        assert!(
            ratio >= pair.size.min_contrast(),
            "{} on {} is {ratio:.2}:1, under the {:?} floor",
            pair.ink,
            pair.ground,
            pair.size
        );
        assert!(
            pair_meets_wcag_aa(pair),
            "{} on {} is {ratio:.2}:1 and fails the checker",
            pair.ink,
            pair.ground
        );
        // T3.4's own, stricter bar — amended by Phase 4 / D4.4 (§2.2):
        // `Body` pairs still never lean on the 3:1 large-text allowance.
        // `Large` pairs are held only to their own AA floor (asserted
        // above): the wordmark's `text-sheffield-accent` on `bg-white` is
        // the one declared, deliberate exception. Never below WCAG AA
        // itself — 4.5:1 body, 3.0:1 large — for either size.
        if pair.size == TextSize::Body {
            assert!(
                ratio >= 4.5,
                "{} on {} is {ratio:.2}:1, under the 4.5:1 body floor",
                pair.ink,
                pair.ground
            );
        }
        println!("{:<26} on {:<28} {ratio:>6.2}:1", pair.ink, pair.ground);
    }
    for pair in NON_TEXT_PAIRS {
        let ratio = pair_contrast(pair).expect("resolves");
        assert!(
            ratio >= 3.0,
            "non-text {} against {} is {ratio:.2}:1, under WCAG 1.4.11's 3:1",
            pair.ink,
            pair.ground
        );
        println!(
            "{:<26} on {:<28} {ratio:>6.2}:1  (non-text)",
            pair.ink, pair.ground
        );
    }
}

#[test]
fn t3_4_a_every_pair_the_kiosk_actually_paints_is_in_the_table_and_passes_aa() {
    let declared = |ink: &str, ground: &str| -> Option<&'static Pair> {
        PALETTE_PAIRS
            .iter()
            .find(|pair| pair.ink == ink && pair.ground == ground)
    };

    let mut seen: Vec<(String, String)> = Vec::new();
    let mut large: Vec<(String, String)> = Vec::new();
    let mut inline_grounds: Vec<String> = Vec::new();

    for (section, html) in rendered_sections() {
        let painted = painted_pairs(&section, &html);
        assert!(
            !painted.is_empty(),
            "[{section}] rendered no coloured text at all"
        );
        for Painted { tag, ink, ground } in painted {
            match ground {
                Ground::Class(ground) => {
                    let pair = declared(&ink, &ground).unwrap_or_else(|| {
                        panic!(
                            "[{section}] <{tag}> paints `{ink}` on `{ground}`, \
                             which is not a declared palette pair"
                        )
                    });
                    let ratio = pair_contrast(pair).expect("resolves");
                    // Phase 4 / D4.4 (§2.2) split the blanket 4.5:1 into the
                    // two WCAG AA floors the `size` column already names, and
                    // this is the second place that blanket was written: a
                    // painted pair is held to *its own* declared floor — 4.5:1
                    // for `Body`, 3.0:1 for the one `Large` pair (the wordmark
                    // word "Morning", 60 px / 800 / `.poster-outline`). Never
                    // below AA for either size; `Body` is not relaxed at all.
                    let floor = pair.size.min_contrast();
                    assert!(
                        ratio >= floor,
                        "[{section}] <{tag}> {ink} on {ground} is {ratio:.2}:1, \
                         under the {:?} floor of {floor:.1}:1",
                        pair.size
                    );
                    let key = (ink.clone(), ground.clone());
                    if pair.size == TextSize::Large && !large.contains(&key) {
                        large.push(key.clone());
                    }
                    if !seen.contains(&key) {
                        seen.push(key);
                    }
                }
                Ground::Inline(rgb) => {
                    // The profile discs: the ground is a `#rrggbb` from the
                    // `profiles` row, so contrast is computed straight from
                    // it rather than looked up.
                    let ink_rgb = resolve(&ink)
                        .unwrap_or_else(|| panic!("[{section}] `{ink}` is not a palette token"));
                    let ratio = contrast_ratio(ink_rgb, rgb);
                    assert!(
                        ratio >= 4.5,
                        "[{section}] <{tag}> {ink} on the inline ground {} is {ratio:.2}:1",
                        rgb.to_hex()
                    );
                    let hex = rgb.to_hex();
                    if !inline_grounds.contains(&hex) {
                        inline_grounds.push(hex);
                    }
                }
            }
        }
    }

    assert!(
        seen.len() >= 8,
        "only {} distinct ink/ground pairs were exercised: {seen:?}",
        seen.len()
    );
    // The 3:1 large-text allowance is a door, not a corridor: exactly one
    // pair on the kiosk may walk through it, and it is the poster wordmark's
    // display red. Anything else painted as `Large` fails here even though it
    // would clear its own floor above.
    large.sort();
    assert_eq!(
        large,
        vec![("text-sheffield-accent".to_string(), "bg-white".to_string())],
        "the only pair allowed the AA large-text floor is the wordmark's display red"
    );
    inline_grounds.sort();
    assert_eq!(
        inline_grounds,
        vec!["#2672b3", "#8bb5da", "#e86a58", "#f4d03f"],
        "the four seeded profile discs should each have been checked"
    );
    println!("{} distinct rendered pairs, all AA: {seen:?}", seen.len());
    println!("profile discs checked against their own hex: {inline_grounds:?}");
}

#[test]
fn t3_4_a_no_source_file_on_either_surface_names_a_colour_outside_the_palette() {
    let mut checked = 0usize;
    for (path, source) in surface_sources() {
        for word in source.split(|c: char| !(c.is_ascii_alphanumeric() || "-/:[]%._".contains(c))) {
            // Strip Tailwind variants: `focus:ring-sheffield-sun`.
            let class = word.rsplit(':').next().unwrap_or(word);
            let Some((name, _)) = split_token(class) else {
                continue;
            };
            checked += 1;
            assert!(
                token(name).is_some() || COLOURLESS_TOKENS.contains(&name),
                "{path} names `{class}`, which is not in the Sheffield palette \
                 ({} tokens + {:?})",
                PALETTE_TOKENS.len(),
                COLOURLESS_TOKENS
            );
        }
    }
    assert!(
        checked > 40,
        "only {checked} colour utilities were scanned — the scanner found nothing to check"
    );
    println!("{checked} colour utilities scanned across components/**, all in-palette");
}

// ---------------------------------------------------------------------------
// (b) the type scale: <= 6 sizes, all >= 28 px, on /tv
// ---------------------------------------------------------------------------

fn golden_type_scale() -> Vec<(String, u32)> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("tv_type_scale.txt");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("reading {path:?}: {err}"))
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let (class, px) = line.split_once(' ').expect("`<class> <px>`");
            (class.to_string(), px.trim().parse::<u32>().expect("px"))
        })
        .collect()
}

/// A Tailwind font-size utility, or `None`.
fn font_size_class(class: &str) -> Option<&str> {
    const SIZES: [&str; 13] = [
        "xs", "sm", "base", "lg", "xl", "2xl", "3xl", "4xl", "5xl", "6xl", "7xl", "8xl", "9xl",
    ];
    let rest = class.strip_prefix("text-")?;
    if rest.starts_with('[') || SIZES.contains(&rest) {
        Some(class)
    } else {
        None
    }
}

#[test]
fn t3_4_b_the_tv_type_scale_is_at_most_six_sizes_and_never_under_twenty_eight_px() {
    let allowlist = golden_type_scale();
    assert!(
        allowlist.len() <= 6,
        "the /tv type scale is {} sizes; T3.4 caps it at 6",
        allowlist.len()
    );
    assert!(!allowlist.is_empty());
    for (class, px) in &allowlist {
        assert!(
            *px >= TV_MIN_BODY_PX,
            "{class} is {px}px, under the {TV_MIN_BODY_PX}px 10-foot minimum"
        );
    }
    // The constants the components use are the same table.
    let from_code: Vec<(String, u32)> = TV_TYPE_SCALE
        .iter()
        .map(|(class, px)| ((*class).to_string(), *px))
        .collect();
    assert_eq!(from_code, allowlist);
    const { assert!(TV_MIN_HEADING_PX >= TV_MIN_BODY_PX) };

    // ...and nothing outside it reaches the markup.
    let mut rendered: Vec<String> = Vec::new();
    for (section, html) in rendered_sections() {
        for class in html
            .split(|c: char| c.is_whitespace() || c == '"')
            .filter_map(font_size_class)
        {
            let px = allowlist
                .iter()
                .find(|(allowed, _)| allowed == class)
                .map(|(_, px)| *px)
                .unwrap_or_else(|| panic!("[{section}] renders `{class}`, which is off the scale"));
            assert!(px >= TV_MIN_BODY_PX);
            if !rendered.iter().any(|s| s == class) {
                rendered.push(class.to_string());
            }
        }
    }
    println!(
        "/tv renders {} of the {} allowlisted sizes: {rendered:?}",
        rendered.len(),
        allowlist.len()
    );
}

// ---------------------------------------------------------------------------
// (c) overscan on every /tv container
// ---------------------------------------------------------------------------

#[test]
fn t3_4_c_every_full_screen_tv_container_carries_the_overscan_class() {
    let mut containers = 0usize;
    for (section, html) in rendered_sections() {
        let mut surfaces = 0usize;
        for tag in html.split('<') {
            let Some(classes) = attr(tag, "class") else {
                continue;
            };
            let is_surface = attr(tag, "data-tv-surface").is_some();
            let is_full_screen = classes.split_whitespace().any(|c| c == "min-h-screen");
            if !is_surface && !is_full_screen {
                continue;
            }
            surfaces += 1;
            containers += 1;
            assert!(
                classes.split_whitespace().any(|c| c == TV_OVERSCAN_CLASS),
                "[{section}] a full-screen container is missing `{TV_OVERSCAN_CLASS}`: {classes}"
            );
        }
        assert!(
            surfaces > 0,
            "[{section}] rendered no full-screen container"
        );
    }

    // The same rule in the source, so a container that no model renders
    // cannot escape it.
    for (path, source) in surface_sources() {
        if !path.contains("tv") {
            continue;
        }
        for line in logical_lines(&source) {
            if line.contains("min-h-screen") {
                assert!(
                    line.contains(TV_OVERSCAN_CLASS) || line.contains("TV_OVERSCAN_CLASS"),
                    "{path}: `min-h-screen` without the overscan class:\n  {}",
                    line.trim()
                );
            }
        }
    }
    println!("{containers} full-screen /tv containers, all wearing `{TV_OVERSCAN_CLASS}`");
}

// ---------------------------------------------------------------------------
// (d) no `hover:`-only affordance on /tv
// ---------------------------------------------------------------------------

#[test]
fn t3_4_d_no_tv_component_or_rendered_page_uses_a_hover_variant() {
    // A D-pad has no pointer. Spelled by concatenation so this assertion
    // does not trip over its own source text.
    let banned = concat!("hover", ":");
    let mut files = 0usize;
    for (path, source) in surface_sources() {
        if !path.contains("tv") {
            continue;
        }
        files += 1;
        assert!(!source.contains(banned), "{path} uses a `{banned}` variant");
    }
    assert!(files >= 8, "only {files} /tv sources were scanned");

    for (section, html) in rendered_sections() {
        assert!(
            !html.contains(banned),
            "[{section}] rendered a `{banned}` class"
        );
    }
    println!("{files} /tv sources and every rendered panel are free of `{banned}`");
}

// ---------------------------------------------------------------------------
// Shared: the source files of the two surfaces
// ---------------------------------------------------------------------------

/// Fold Rust's line-continued string literals (a `\` as the last character of
/// a source line) back into one logical line, so a class list written across
/// two source lines is checked as the single class list the compiler sees.
fn logical_lines(source: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut pending: Option<String> = None;
    for line in source.replace('\r', "").lines() {
        let joined = match pending.take() {
            Some(head) => head + line.trim_start(),
            None => line.to_string(),
        };
        match joined.strip_suffix('\\') {
            Some(head) => pending = Some(head.to_string()),
            None => out.push(joined),
        }
    }
    if let Some(rest) = pending {
        out.push(rest);
    }
    out
}

/// Every `.rs` file under `src/client/components/**` as `(path, contents)`,
/// bar `palette.rs`.
///
/// QA round 1 (Q1-15) widened this from `{tv,mobile}` to the whole tree: the
/// phone renders `routine.rs`, `calendar.rs` and `whiteboard.rs` as well as
/// `mobile/**`, and five sub-AA classes were living in exactly that blind
/// spot. `palette.rs` is skipped because it is the module that *defines* the
/// palette — it has to be able to name `slate-400` in order to prove
/// `slate-400` is not a token, and it renders nothing.
fn surface_sources() -> Vec<(String, String)> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("client")
        .join("components");
    let mut out = Vec::new();
    let mut queue = vec![root];
    while let Some(dir) = queue.pop() {
        for entry in std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("reading {dir:?}: {e}")) {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                queue.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs")
                || path.file_name().and_then(|n| n.to_str()) == Some("palette.rs")
            {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("reading a source file");
            out.push((path.to_string_lossy().replace('\\', "/"), text));
        }
    }
    out.sort();
    assert!(
        out.len() >= 20,
        "expected both surfaces' modules and the shared panels, got {}",
        out.len()
    );
    for shared in ["routine.rs", "calendar.rs", "whiteboard.rs"] {
        assert!(
            out.iter().any(|(path, _)| path.ends_with(shared)),
            "the widened scan (Q1-15) must reach {shared}"
        );
    }
    out
}
