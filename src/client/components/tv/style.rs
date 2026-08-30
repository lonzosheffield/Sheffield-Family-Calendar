//! The kiosk's visual contract (PLAN v2 **D8**, task T2.1).
//!
//! Every class the 10-foot surface uses that carries a *rule* lives here as a
//! constant, so the rule is enforced in one place and asserted in one place
//! (`tests/tv_tests.rs`, acceptance (a) and (f)):
//!
//! * **Focus ring** — every focusable element carries [`TV_FOCUSABLE_CLASS`],
//!   which contains both the ring geometry (`ring-8 ring-offset-4`) and the
//!   browser-focus form of the ring colour (`focus:ring-sheffield-sun`). The
//!   element the *remote* has focused additionally carries the unprefixed
//!   [`TV_FOCUS_RING_ACTIVE`]; everything else carries
//!   [`TV_FOCUS_RING_IDLE`]. Two rings, one geometry: the D-pad ring is what
//!   a child at 10 feet actually sees, and the `focus:` ring keeps a real
//!   keyboard/`autofocus` user honest.
//! * **Type scale** — exactly four sizes, all ≥ 28 px, headings ≥ 44 px.
//!   The committed allowlist is `tests/golden/tv_type_scale.txt`; nothing
//!   outside it may appear in a rendered `/tv` page.
//! * **Overscan** — [`TV_OVERSCAN_CLASS`] is 5 % padding on every
//!   full-screen container, so nothing lands in the bezel-cropped edge of a
//!   television (D8).
//! * **No pointer-only affordances** — a D-pad has no pointer, so Tailwind's
//!   hover variant is banned outright. `tests/tv_tests.rs` greps this whole
//!   directory (and the rendered markup) for that prefix and fails on a hit,
//!   which is why neither the prefix nor an example of it is written out
//!   anywhere in here.
//!
//! Sizes are the stock Tailwind scale (`tailwind.config.js` is unmodified —
//! it belongs to T0.1) at the default 16 px root: `text-3xl` = 1.875rem =
//! 30 px, `text-4xl` = 2.25rem = 36 px, `text-5xl` = 3rem = 48 px,
//! `text-6xl` = 3.75rem = 60 px.

/// 5 % overscan padding (D8). Percentage padding resolves against the
/// container's *width*, so on the 1920 × 1080 render target this is 96 px on
/// all four sides — comfortably inside the ~5 % a television may crop.
pub const TV_OVERSCAN_CLASS: &str = "p-[5%]";

/// Carried by **every** focusable element on the kiosk.
///
/// `ring-offset-4` punches a gap between the element and the ring so the ring
/// reads as a ring rather than a thick border at distance. **T3.4** made that
/// gap `sheffield-dark` rather than `sheffield-paper`: D8 fixes the ring
/// itself at `sheffield-sun`, and sun on paper is 1.5:1, so the indicator had
/// no edge WCAG 1.4.11 would accept. A dark gap gives it two — sun on dark is
/// 3.4:1 and dark on a white card is 5.1:1. Both are asserted from the hex
/// values by [`crate::client::components::palette::NON_TEXT_PAIRS`].
pub const TV_FOCUSABLE_CLASS: &str = "tv-focusable block w-full text-left rounded-3xl outline-none ring-8 ring-offset-4 ring-offset-sheffield-dark focus:ring-sheffield-sun";

/// Ring colour on the element the remote has focused.
pub const TV_FOCUS_RING_ACTIVE: &str = "ring-sheffield-sun";

/// Ring colour on every other focusable: present, transparent, so focus
/// moving between elements never reflows the layout.
pub const TV_FOCUS_RING_IDLE: &str = "ring-transparent";

/// Body copy: 30 px.
pub const TV_BODY_TEXT: &str = "text-3xl";
/// Emphasised body copy (a routine item's title, an event's summary): 36 px.
pub const TV_BODY_LARGE: &str = "text-4xl";
/// Section heading: 48 px (≥ 44 px, D8).
pub const TV_HEADING: &str = "text-5xl";
/// Page heading: 60 px.
pub const TV_HEADING_LARGE: &str = "text-6xl";

/// The four sizes above with their computed pixel values — the same table
/// `tests/golden/tv_type_scale.txt` commits, so a drift in either is a test
/// failure rather than a silent regression.
pub const TV_TYPE_SCALE: [(&str, u32); 4] = [
    (TV_BODY_TEXT, 30),
    (TV_BODY_LARGE, 36),
    (TV_HEADING, 48),
    (TV_HEADING_LARGE, 60),
];

/// Smallest body size the kiosk may use (D8).
pub const TV_MIN_BODY_PX: u32 = 28;
/// Smallest heading size the kiosk may use (D8).
pub const TV_MIN_HEADING_PX: u32 = 44;

/// The class string for one focusable element.
///
/// `focused` is the *remote's* cursor, not the DOM's `:focus` — on a kiosk
/// the DOM focus stays on the root key-handler element so a single listener
/// sees every press (see [`super::shell`]).
pub fn focus_class(focused: bool) -> String {
    let ring = if focused {
        TV_FOCUS_RING_ACTIVE
    } else {
        TV_FOCUS_RING_IDLE
    };
    format!("{TV_FOCUSABLE_CLASS} {ring}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_type_scale_entry_clears_the_ten_foot_minimum() {
        for (class, px) in TV_TYPE_SCALE {
            assert!(
                px >= TV_MIN_BODY_PX,
                "{class} is {px}px, below the {TV_MIN_BODY_PX}px body minimum"
            );
        }
    }

    #[test]
    fn the_heading_sizes_clear_the_heading_minimum() {
        for class in [TV_HEADING, TV_HEADING_LARGE] {
            let px = TV_TYPE_SCALE
                .iter()
                .find(|(name, _)| *name == class)
                .map(|(_, px)| *px)
                .expect("heading class is in the scale");
            assert!(
                px >= TV_MIN_HEADING_PX,
                "{class} is {px}px, below the {TV_MIN_HEADING_PX}px heading minimum"
            );
        }
    }

    #[test]
    fn the_focused_element_is_the_only_one_wearing_the_live_ring() {
        let focused = focus_class(true);
        let idle = focus_class(false);

        // Exact-token comparison: `focus:ring-sheffield-sun` *contains*
        // `ring-sheffield-sun` as a substring, and the difference between
        // the two is the whole point.
        let tokens =
            |class: &str| -> Vec<String> { class.split_whitespace().map(str::to_string).collect() };

        assert!(tokens(&focused).iter().any(|t| t == TV_FOCUS_RING_ACTIVE));
        assert!(!tokens(&idle).iter().any(|t| t == TV_FOCUS_RING_ACTIVE));
        assert!(tokens(&idle).iter().any(|t| t == TV_FOCUS_RING_IDLE));

        // ...and both still carry the ring geometry and the `:focus` form.
        for class in [&focused, &idle] {
            assert!(class.contains("ring-8"), "{class}");
            assert!(class.contains("focus:ring-sheffield-sun"), "{class}");
        }
    }

    // The "no pointer-only affordance" rule is asserted in
    // `tests/tv_tests.rs`, which greps this whole directory's source *and*
    // the rendered markup for the prefix. It deliberately is not asserted
    // here: a test written here would have to spell the forbidden prefix
    // out, and the grep would then trip over its own assertion.
}
