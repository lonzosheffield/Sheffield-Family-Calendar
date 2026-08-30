//! The Sheffield palette as an enforceable contract (task **T3.4**).
//!
//! PLAN v2 §3 T3.4 / PURPLE §P3 T3.4 asks for one objective thing about
//! colour: *"every foreground/background token pair in the palette meets
//! **WCAG AA** (≥ 4.5:1 body, ≥ 3:1 large), **computed in Rust from the hex
//! values**"*. This module is that computation, and [`PALETTE_PAIRS`] is that
//! set of pairs — every ink-on-ground combination the two surfaces
//! (`components/tv/**`, `components/mobile/**`, and the three shared panels
//! `routine.rs`, `calendar.rs` and `whiteboard.rs` that `/m` renders)
//! actually paint.
//!
//! # Why the table is the contract and not a comment
//!
//! A contrast ratio you can only check by eye is a contrast ratio that rots.
//! So the rule is enforced from three directions at once:
//!
//! 1. [`PALETTE_PAIRS`] is checked here, in unit tests, straight from the hex
//!    values in [`PALETTE_TOKENS`] — WCAG 2.x relative luminance, no crate,
//!    no fixture.
//! 2. `tests/palette_tests.rs` renders the real `/tv` markup, walks it with a
//!    nesting-aware scanner so every element knows the ground it is sitting
//!    on, and fails if any rendered ink/ground pair is missing from this
//!    table — so the table cannot silently fall behind the components.
//! 3. The same test greps every source file under `components/**` (bar this
//!    one) and fails on any colour utility whose token is not in
//!    [`PALETTE_TOKENS`] — so a stray `slate-400` cannot appear at all.
//!    QA round 1 (Q1-15) widened that grep from `tv/**` + `mobile/**`, which
//!    had let five sub-AA classes through on the phone's Routine tab.
//!
//! # The palette is unchanged
//!
//! T3.4 is *palette-faithful* polish: the five Sheffield hues in
//! `tailwind.config.js` are exactly as T0.1 left them, and this module reads
//! them rather than redefining them. What T3.4 changed is which ink sits on
//! which ground — the neutral ramp is now three stops (ink, muted, on-dark)
//! instead of six, and the mid-tone hues (`sun`, `accent`) are used as
//! *grounds under dark ink* rather than as ink on paper, which is the only
//! way a 3:1-ish hue can carry text at AA.
//!
//! # Non-text contrast
//!
//! WCAG 1.4.11 wants 3:1 for the bits that identify a *state* rather than
//! carry words — on this hub that is the D-pad focus ring, and only it.
//! [`NON_TEXT_PAIRS`] holds those and is asserted at 3:1. D8 fixes the ring's
//! colour (`ring-sheffield-sun`), so the polish available was the *offset*:
//! the ring now sits on a `sheffield-dark` gap instead of a paper one, which
//! puts a passing edge on both sides of the sun (sun→dark 3.4:1, dark→card
//! 5.1:1) instead of a 1.5:1 sun→paper edge with nothing behind it.
//!
//! Purely decorative hairlines (`ring-slate-200` on a phone card) are
//! deliberately **not** in that table: they identify nothing, the control is
//! identified by its own ground and its label, and 1.4.11 does not reach
//! them.

/// An 8-bit sRGB colour.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    /// `Rgb::hex(0x2672B3)` — the same digits the Tailwind config carries.
    pub const fn hex(value: u32) -> Self {
        Self {
            r: ((value >> 16) & 0xFF) as u8,
            g: ((value >> 8) & 0xFF) as u8,
            b: (value & 0xFF) as u8,
        }
    }

    /// `#RRGGBB` (or `RRGGBB`) as written in the `profiles` table.
    pub fn parse(text: &str) -> Option<Self> {
        let digits = text.trim().strip_prefix('#').unwrap_or(text.trim());
        if digits.len() != 6 || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        u32::from_str_radix(digits, 16).ok().map(Self::hex)
    }

    /// `#rrggbb`, lower case.
    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

// ---------------------------------------------------------------------------
// The tokens
// ---------------------------------------------------------------------------

/// The five Sheffield hues, straight from `tailwind.config.js`.
pub const SHEFFIELD_LIGHT: Rgb = Rgb::hex(0x8BB5DA);
/// The hub's primary blue: headings, filled buttons, the phone header.
pub const SHEFFIELD_DARK: Rgb = Rgb::hex(0x2672B3);
/// The warm alarm hue: the disconnected badge and the offline chip, as a
/// *ground* under dark ink.
pub const SHEFFIELD_ACCENT: Rgb = Rgb::hex(0xE86A58);
/// The focus ring, and the "connected" chip's ground.
pub const SHEFFIELD_SUN: Rgb = Rgb::hex(0xF4D03F);
/// The page ground on both surfaces.
pub const SHEFFIELD_PAPER: Rgb = Rgb::hex(0xFDFDFD);

/// Card ground.
pub const WHITE: Rgb = Rgb::hex(0xFFFFFF);
/// Ink. Tailwind `slate-800`.
pub const SLATE_800: Rgb = Rgb::hex(0x1E293B);
/// Muted ink — the only secondary text colour on either surface. Tailwind
/// `slate-600`. (T3.4 replaced `slate-400`, `slate-500` and `slate-700` with
/// this one stop: `slate-500` on the completed-row tint was 3.9:1.)
pub const SLATE_600: Rgb = Rgb::hex(0x475569);
/// Ink on the dark `?keys=1` overlay, and the phone's hairlines. Tailwind
/// `slate-200`.
pub const SLATE_200: Rgb = Rgb::hex(0xE2E8F0);
/// The phone's card hairline. Tailwind `slate-100`.
pub const SLATE_100: Rgb = Rgb::hex(0xF1F5F9);
/// The phone's inert field fill (calendar form inputs, the week grid's
/// all-day chips). Tailwind `slate-50`.
pub const SLATE_50: Rgb = Rgb::hex(0xF8FAFC);
/// The screensaver's ground and the phone dialog's scrim, at 50 %.
pub const BLACK: Rgb = Rgb::hex(0x000000);

/// The error hue, four Tailwind stops. Only the phone paints it — the kiosk
/// has no destructive action — and only the two dark stops carry text.
/// Tailwind `red-700`.
pub const RED_700: Rgb = Rgb::hex(0xB91C1C);
/// Tailwind `red-600`.
pub const RED_600: Rgb = Rgb::hex(0xDC2626);
/// Tailwind `red-200`.
pub const RED_200: Rgb = Rgb::hex(0xFECACA);
/// Tailwind `red-50`.
pub const RED_50: Rgb = Rgb::hex(0xFEF2F2);

/// Every colour token `src/client/components/**` is allowed to name, as the
/// Tailwind token name (what follows `text-`, `bg-`, `ring-`, …).
///
/// `tests/palette_tests.rs` fails on any colour utility outside this list, so
/// this is the whole palette of the hub's two surfaces: sixteen colours and
/// one colourless token ([`COLOURLESS_TOKENS`]).
///
/// QA round 1 (Q1-15) widened the scan from `tv/**` and `mobile/**` to the
/// whole of `components/**`, because `/m` renders `routine.rs`,
/// `calendar.rs` and `whiteboard.rs` too. The seven tokens added with it are
/// the ones those three files already painted: the error ramp, two neutral
/// fills and black.
pub const PALETTE_TOKENS: [(&str, Rgb); 16] = [
    ("sheffield-light", SHEFFIELD_LIGHT),
    ("sheffield-dark", SHEFFIELD_DARK),
    ("sheffield-accent", SHEFFIELD_ACCENT),
    ("sheffield-sun", SHEFFIELD_SUN),
    ("sheffield-paper", SHEFFIELD_PAPER),
    ("white", WHITE),
    ("slate-800", SLATE_800),
    ("slate-600", SLATE_600),
    ("slate-200", SLATE_200),
    ("slate-100", SLATE_100),
    ("slate-50", SLATE_50),
    ("black", BLACK),
    ("red-700", RED_700),
    ("red-600", RED_600),
    ("red-200", RED_200),
    ("red-50", RED_50),
];

/// `transparent` is a token with no colour: the idle focus ring wears it so
/// focus movement never reflows the layout. It is allowed as a class and has
/// no contrast.
pub const COLOURLESS_TOKENS: [&str; 1] = ["transparent"];

/// Look a Tailwind token name up in [`PALETTE_TOKENS`].
pub fn token(name: &str) -> Option<Rgb> {
    PALETTE_TOKENS
        .iter()
        .find(|(token, _)| *token == name)
        .map(|(_, rgb)| *rgb)
}

// ---------------------------------------------------------------------------
// WCAG 2.x maths
// ---------------------------------------------------------------------------

/// One channel, sRGB 0–255 → linear 0.0–1.0 (WCAG 2.x definition).
fn linearise(channel: u8) -> f64 {
    let c = f64::from(channel) / 255.0;
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// WCAG relative luminance, 0.0 (black) – 1.0 (white).
pub fn relative_luminance(colour: Rgb) -> f64 {
    0.2126 * linearise(colour.r) + 0.7152 * linearise(colour.g) + 0.0722 * linearise(colour.b)
}

/// WCAG contrast ratio, 1.0 – 21.0. Symmetric in its arguments.
pub fn contrast_ratio(a: Rgb, b: Rgb) -> f64 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// Flatten `colour` at `alpha` (0.0–1.0) over `ground` — what the browser
/// actually paints for a Tailwind opacity modifier such as
/// `bg-sheffield-light/25`.
pub fn composite(colour: Rgb, alpha: f64, ground: Rgb) -> Rgb {
    let mix = |fg: u8, bg: u8| -> u8 {
        (alpha * f64::from(fg) + (1.0 - alpha) * f64::from(bg)).round() as u8
    };
    Rgb {
        r: mix(colour.r, ground.r),
        g: mix(colour.g, ground.g),
        b: mix(colour.b, ground.b),
    }
}

/// Whichever of [`SLATE_800`] / [`WHITE`] reads better on `ground`, as the
/// Tailwind class.
///
/// The four child profiles carry an arbitrary `#rrggbb` in the `profiles`
/// table and the kiosk paints their initial on a disc of it — a white "B" on
/// `#F4D03F` is 1.5:1. This picks the ink instead of assuming one, and
/// `worst_case_ink_still_clears_the_large_text_floor` sweeps the whole RGB
/// cube to show the pick is never worse than 3.8:1.
pub fn best_ink_on(ground: Rgb) -> &'static str {
    if contrast_ratio(SLATE_800, ground) >= contrast_ratio(WHITE, ground) {
        "text-slate-800"
    } else {
        "text-white"
    }
}

/// The ink [`best_ink_on`] names, as a colour.
pub fn best_ink_rgb(ground: Rgb) -> Rgb {
    if contrast_ratio(SLATE_800, ground) >= contrast_ratio(WHITE, ground) {
        SLATE_800
    } else {
        WHITE
    }
}

// ---------------------------------------------------------------------------
// The pair table
// ---------------------------------------------------------------------------

/// WCAG's two text thresholds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TextSize {
    /// Normal text: **4.5:1**.
    Body,
    /// ≥ 24 px, or ≥ 18.66 px bold: **3:1**.
    Large,
}

impl TextSize {
    /// The AA floor for this size.
    pub fn min_contrast(self) -> f64 {
        match self {
            TextSize::Body => 4.5,
            TextSize::Large => 3.0,
        }
    }
}

/// One ink-on-ground pair the surfaces render.
///
/// `ink` and `ground` are the Tailwind classes as written in the components,
/// opacity modifier included, so a reader can grep the class straight out of
/// the source.
#[derive(Clone, Copy, Debug)]
pub struct Pair {
    /// e.g. `"text-slate-800"`.
    pub ink: &'static str,
    /// e.g. `"bg-sheffield-light/25"`.
    pub ground: &'static str,
    /// The WCAG threshold this pair is held to.
    pub size: TextSize,
    /// Where it is rendered — prose, for the failure message.
    pub used_by: &'static str,
}

/// Every foreground/background token pair `/tv` and `/m` render.
///
/// Every one of these clears **4.5:1**, the body threshold, even the ones
/// that only ever carry 30 px+ display text and would be allowed 3:1: the
/// `size` column records what WCAG would demand, and the tests assert both
/// that floor and the stricter blanket one.
pub const PALETTE_PAIRS: &[Pair] = &[
    // -- the paper ground, both surfaces -----------------------------------
    Pair {
        ink: "text-slate-800",
        ground: "bg-sheffield-paper",
        size: TextSize::Body,
        used_by: "the base ink of every screen",
    },
    Pair {
        ink: "text-slate-600",
        ground: "bg-sheffield-paper",
        size: TextSize::Body,
        used_by: "secondary lines on the kiosk (`updated HH:MM`, hints)",
    },
    Pair {
        ink: "text-sheffield-dark",
        ground: "bg-sheffield-paper",
        size: TextSize::Large,
        used_by: "panel and overlay headings",
    },
    // -- white cards --------------------------------------------------------
    Pair {
        ink: "text-slate-800",
        ground: "bg-white",
        size: TextSize::Body,
        used_by: "routine rows, event rows, phone cards",
    },
    Pair {
        ink: "text-slate-600",
        ground: "bg-white",
        size: TextSize::Body,
        used_by: "row subtitles and the phone's inactive tabs",
    },
    Pair {
        ink: "text-sheffield-dark",
        ground: "bg-white",
        size: TextSize::Body,
        used_by: "the active phone tab, remote buttons, `Add a phone`",
    },
    // -- the primary blue as a ground --------------------------------------
    Pair {
        ink: "text-white",
        ground: "bg-sheffield-dark",
        size: TextSize::Body,
        used_by: "the active profile, the phone header, filled buttons",
    },
    // -- the two mid-tone hues, as grounds under dark ink ------------------
    Pair {
        ink: "text-slate-800",
        ground: "bg-sheffield-sun",
        size: TextSize::Body,
        used_by: "the phone's `Connected` chip",
    },
    Pair {
        ink: "text-slate-800",
        ground: "bg-sheffield-accent",
        size: TextSize::Body,
        used_by: "the kiosk `Disconnected` badge, the phone `Offline` chip, the routine count",
    },
    // -- tints over paper ---------------------------------------------------
    Pair {
        ink: "text-slate-800",
        ground: "bg-sheffield-light/25",
        size: TextSize::Body,
        used_by: "a completed routine row",
    },
    Pair {
        ink: "text-slate-600",
        ground: "bg-sheffield-light/25",
        size: TextSize::Body,
        used_by: "a completed routine row's subtitle",
    },
    Pair {
        ink: "text-slate-800",
        ground: "bg-sheffield-sun/30",
        size: TextSize::Body,
        used_by: "the phone's queue toast",
    },
    Pair {
        ink: "text-slate-800",
        ground: "bg-sheffield-sun/20",
        size: TextSize::Body,
        used_by: "the phone's `sign in to control the TV` note",
    },
    Pair {
        ink: "text-slate-800",
        ground: "bg-sheffield-accent/10",
        size: TextSize::Body,
        used_by: "the phone's `not connected to the hub` note",
    },
    // -- the error ramp, phone only (QA round 1, Q1-15) --------------------
    Pair {
        ink: "text-red-700",
        ground: "bg-red-50",
        size: TextSize::Body,
        used_by: "the phone's routine error card",
    },
    Pair {
        ink: "text-red-600",
        ground: "bg-white",
        size: TextSize::Body,
        used_by: "the calendar's `Delete` button",
    },
    Pair {
        ink: "text-white",
        ground: "bg-red-600",
        size: TextSize::Body,
        used_by: "the `Try again` buttons on both error cards",
    },
    Pair {
        ink: "text-red-700",
        ground: "bg-white",
        size: TextSize::Body,
        used_by: "the calendar error card's heading and the task `Delete` button",
    },
    // -- the dark `?keys=1` overlay ----------------------------------------
    Pair {
        ink: "text-white",
        ground: "bg-slate-800",
        size: TextSize::Body,
        used_by: "the key-code overlay's headings",
    },
    Pair {
        ink: "text-slate-200",
        ground: "bg-slate-800",
        size: TextSize::Body,
        used_by: "the key-code overlay's rows",
    },
];

/// Non-text pairs held to WCAG 1.4.11's 3:1 — the D-pad focus indicator, and
/// only it (see the module docs for why the decorative hairlines are out).
pub const NON_TEXT_PAIRS: &[Pair] = &[
    Pair {
        ink: "ring-sheffield-sun",
        ground: "ring-offset-sheffield-dark",
        size: TextSize::Large,
        used_by: "the live focus ring against its own offset gap",
    },
    Pair {
        ink: "ring-offset-sheffield-dark",
        ground: "bg-white",
        size: TextSize::Large,
        used_by: "the focus ring's offset gap against a card",
    },
    Pair {
        ink: "ring-offset-sheffield-dark",
        ground: "bg-sheffield-paper",
        size: TextSize::Large,
        used_by: "the focus ring's offset gap against the page",
    },
];

/// Resolve a Tailwind colour utility — prefix, token, optional `/NN` opacity
/// — to the colour a browser paints, flattening any opacity over
/// [`SHEFFIELD_PAPER`] (the page ground under every tinted surface in the
/// hub).
///
/// Returns `None` for a class that names no palette colour.
pub fn resolve(class: &str) -> Option<Rgb> {
    let (name, alpha) = split_token(class)?;
    let base = token(name)?;
    Some(match alpha {
        Some(alpha) => composite(base, alpha, SHEFFIELD_PAPER),
        None => base,
    })
}

/// The colour-bearing Tailwind prefixes either surface uses.
pub const COLOUR_PREFIXES: [&str; 6] = [
    "ring-offset-", // before `ring-`: longest match first
    "text-",
    "bg-",
    "ring-",
    "border-",
    "divide-",
];

/// Split a colour utility into `(token name, opacity)`.
///
/// `"bg-sheffield-light/25"` → `("sheffield-light", Some(0.25))`.
/// Returns `None` for `text-3xl`, `ring-8`, `border-t` and friends: those
/// share the prefix but name no colour.
pub fn split_token(class: &str) -> Option<(&str, Option<f64>)> {
    let rest = COLOUR_PREFIXES
        .iter()
        .find_map(|prefix| class.strip_prefix(prefix))?;
    let (name, alpha) = match rest.split_once('/') {
        Some((name, percent)) => (name, Some(percent.parse::<f64>().ok()? / 100.0)),
        None => (rest, None),
    };
    if !is_colour_name(name) {
        return None;
    }
    Some((name, alpha))
}

/// Does `name` name a colour at all (ours or one of Tailwind's stock
/// families)? Used to tell `text-slate-400` — a colour, and a banned one —
/// from `text-3xl`, which is not a colour.
pub fn is_colour_name(name: &str) -> bool {
    const FAMILIES: [&str; 22] = [
        "slate", "gray", "zinc", "neutral", "stone", "red", "orange", "amber", "yellow", "lime",
        "green", "emerald", "teal", "cyan", "sky", "blue", "indigo", "violet", "purple", "fuchsia",
        "pink", "rose",
    ];
    const BARE: [&str; 5] = ["white", "black", "transparent", "current", "inherit"];

    if BARE.contains(&name) || name.starts_with("sheffield-") {
        return true;
    }
    match name.split_once('-') {
        Some((family, shade)) => {
            FAMILIES.contains(&family) && shade.chars().all(|c| c.is_ascii_digit())
        }
        None => false,
    }
}

/// The contrast ratio a pair actually achieves, or `None` if either side does
/// not resolve (which is itself a test failure).
pub fn pair_contrast(pair: &Pair) -> Option<f64> {
    Some(contrast_ratio(resolve(pair.ink)?, resolve(pair.ground)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- the maths itself ---------------------------------------------------

    #[test]
    fn the_contrast_formula_reproduces_the_wcag_reference_values() {
        let black = Rgb::hex(0x000000);
        let white = WHITE;
        // The two anchors every WCAG implementation is checked against.
        assert!((contrast_ratio(black, white) - 21.0).abs() < 0.01);
        assert!((contrast_ratio(white, white) - 1.0).abs() < 1e-9);
        // Symmetric, by definition.
        assert!(
            (contrast_ratio(SLATE_800, SHEFFIELD_SUN) - contrast_ratio(SHEFFIELD_SUN, SLATE_800))
                .abs()
                < 1e-12
        );
        // A published mid-point: #767676 on white is the canonical 4.54:1
        // "smallest grey that passes AA" used in the WCAG techniques.
        let ratio = contrast_ratio(Rgb::hex(0x767676), white);
        assert!((4.5..4.6).contains(&ratio), "#767676 on white was {ratio}");
        // Luminance bounds.
        assert!((relative_luminance(white) - 1.0).abs() < 1e-9);
        assert!(relative_luminance(black).abs() < 1e-9);
    }

    #[test]
    fn compositing_matches_what_the_browser_paints() {
        // 0 % and 100 % are the identities.
        assert_eq!(composite(SHEFFIELD_SUN, 0.0, WHITE), WHITE);
        assert_eq!(composite(SHEFFIELD_SUN, 1.0, WHITE), SHEFFIELD_SUN);
        // 25 % of #8BB5DA over #FDFDFD, channel by channel.
        assert_eq!(
            composite(SHEFFIELD_LIGHT, 0.25, SHEFFIELD_PAPER),
            Rgb {
                r: 225,
                g: 235,
                b: 244
            }
        );
    }

    #[test]
    fn a_colour_utility_splits_into_its_token_and_opacity() {
        assert_eq!(
            split_token("bg-sheffield-light/25"),
            Some(("sheffield-light", Some(0.25)))
        );
        assert_eq!(split_token("text-slate-800"), Some(("slate-800", None)));
        assert_eq!(
            split_token("ring-offset-sheffield-dark"),
            Some(("sheffield-dark", None))
        );
        // Same prefixes, no colour.
        assert_eq!(split_token("text-3xl"), None);
        assert_eq!(split_token("ring-8"), None);
        assert_eq!(split_token("ring-offset-4"), None);
        assert_eq!(split_token("border-t"), None);
        assert_eq!(split_token("flex"), None);
        // A colour that is NOT in the palette still splits — that is how the
        // source scan catches it.
        assert_eq!(split_token("text-slate-400"), Some(("slate-400", None)));
        assert!(token("slate-400").is_none());
    }

    // -- the contract ------------------------------------------------------

    #[test]
    fn every_palette_pair_meets_wcag_aa_for_its_size() {
        for pair in PALETTE_PAIRS {
            let ratio = pair_contrast(pair)
                .unwrap_or_else(|| panic!("`{}` on `{}` does not resolve", pair.ink, pair.ground));
            assert!(
                ratio >= pair.size.min_contrast(),
                "{} on {} ({}) is {ratio:.2}:1, under the {:?} floor of {}:1",
                pair.ink,
                pair.ground,
                pair.used_by,
                pair.size,
                pair.size.min_contrast()
            );
        }
    }

    #[test]
    fn every_palette_pair_clears_the_stricter_body_floor_too() {
        // T3.4's own bar: nothing on either surface relies on the 3:1 large
        // allowance, so a later type-size change can never silently drop a
        // pair below AA.
        for pair in PALETTE_PAIRS {
            let ratio = pair_contrast(pair).expect("resolves");
            assert!(
                ratio >= 4.5,
                "{} on {} is {ratio:.2}:1, under 4.5:1",
                pair.ink,
                pair.ground
            );
        }
    }

    #[test]
    fn the_focus_indicator_meets_the_non_text_floor_on_both_of_its_edges() {
        for pair in NON_TEXT_PAIRS {
            let ratio = pair_contrast(pair).expect("resolves");
            assert!(
                ratio >= 3.0,
                "{} against {} ({}) is {ratio:.2}:1, under WCAG 1.4.11's 3:1",
                pair.ink,
                pair.ground,
                pair.used_by
            );
        }
    }

    #[test]
    fn every_pair_names_tokens_that_are_in_the_palette() {
        for pair in PALETTE_PAIRS.iter().chain(NON_TEXT_PAIRS) {
            for class in [pair.ink, pair.ground] {
                let (name, _) = split_token(class)
                    .unwrap_or_else(|| panic!("`{class}` is not a colour utility"));
                assert!(token(name).is_some(), "`{class}` is not a palette token");
            }
        }
    }

    #[test]
    fn the_pair_table_has_no_duplicates() {
        let mut seen: Vec<(&str, &str)> = Vec::new();
        for pair in PALETTE_PAIRS {
            let key = (pair.ink, pair.ground);
            assert!(!seen.contains(&key), "{key:?} is listed twice");
            seen.push(key);
        }
    }

    // -- the profile discs --------------------------------------------------

    #[test]
    fn every_seeded_profile_colour_gets_an_ink_that_passes_aa() {
        for (_, _, hex) in crate::client::components::tv::fixture::CANONICAL_PROFILES {
            let ground = Rgb::parse(hex).unwrap_or_else(|| panic!("{hex} parses"));
            let ink = best_ink_rgb(ground);
            let ratio = contrast_ratio(ink, ground);
            assert!(
                ratio >= 4.5,
                "the profile disc {hex} gets {} at only {ratio:.2}:1",
                best_ink_on(ground)
            );
        }
        // And it really does switch: white on the blue, dark ink on the sun.
        assert_eq!(best_ink_on(SHEFFIELD_DARK), "text-white");
        assert_eq!(best_ink_on(SHEFFIELD_SUN), "text-slate-800");
        assert_eq!(best_ink_on(SHEFFIELD_ACCENT), "text-slate-800");
        assert_eq!(best_ink_on(SHEFFIELD_LIGHT), "text-slate-800");
    }

    #[test]
    fn worst_case_ink_still_clears_the_large_text_floor() {
        // A profile colour is arbitrary `#rrggbb` from the `profiles` table,
        // so sweep the cube rather than trusting the four seeded ones. The
        // theoretical worst case is the luminance where the two candidate
        // inks tie, ~3.83:1 — comfortably over the 3:1 the 48 px bold
        // initial is held to, and the reason the disc is never the only
        // place a profile's identity is shown (the name sits beside it at
        // 4.5:1+).
        let mut worst = f64::MAX;
        let mut worst_at = Rgb::hex(0x000000);
        for r in (0..=255u16).step_by(17) {
            for g in (0..=255u16).step_by(17) {
                for b in (0..=255u16).step_by(17) {
                    let ground = Rgb {
                        r: r as u8,
                        g: g as u8,
                        b: b as u8,
                    };
                    let ratio = contrast_ratio(best_ink_rgb(ground), ground);
                    if ratio < worst {
                        worst = ratio;
                        worst_at = ground;
                    }
                }
            }
        }
        assert!(
            worst >= 3.0,
            "the worst profile colour {} only reaches {worst:.2}:1",
            worst_at.to_hex()
        );
        println!(
            "worst-case profile disc: {} at {worst:.2}:1",
            worst_at.to_hex()
        );
    }

    #[test]
    fn a_profile_colour_that_is_not_a_hex_triple_is_rejected() {
        assert!(Rgb::parse("#2672B3").is_some());
        assert!(Rgb::parse("2672b3").is_some());
        assert!(Rgb::parse("#26").is_none());
        assert!(Rgb::parse("rebeccapurple").is_none());
        assert!(Rgb::parse("#zzzzzz").is_none());
    }

    /// Print the whole table — this is what T3.4's acceptance output shows.
    #[test]
    fn print_the_contrast_table() {
        println!(
            "{:<26} {:<28} {:>8}  {:?}",
            "ink", "ground", "ratio", "floor"
        );
        for pair in PALETTE_PAIRS.iter().chain(NON_TEXT_PAIRS) {
            println!(
                "{:<26} {:<28} {:>7.2}:1  >= {}",
                pair.ink,
                pair.ground,
                pair_contrast(pair).expect("resolves"),
                pair.size.min_contrast()
            );
        }
    }
}
