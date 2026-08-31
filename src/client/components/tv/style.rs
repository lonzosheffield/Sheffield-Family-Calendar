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
//! * **The frame and the card** (Phase 4 / D4.3) — [`TV_FRAME_CLASS`] paints
//!   the overscan band as the poster's sky-blue border and
//!   [`TV_POSTER_CARD_CLASS`] is the one white, dark-bordered card that
//!   carries every pixel of content. The frame never carries ink; the card
//!   is the only element on the kiosk with a visible dark border.
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

// ---------------------------------------------------------------------------
// Phase 4 / D4.3 — the poster's own furniture
// (`docs/design/DESIGN_DIRECTION.md` §2.3, §2.4, §2.6, §3.1)
// ---------------------------------------------------------------------------

/// The poster's sky-blue frame.
///
/// The 5 % overscan band *is* the frame: the television's bezel-cropped edge
/// and the poster's blue border are the same 96 px on a 1920 × 1080 panel, so
/// the safety margin stopped being empty space and became the design. Nothing
/// but the aria-hidden corner balls ever sits on it — §2.2 makes
/// `sheffield-light` decorative-only, never a ground under ink.
pub const TV_FRAME_CLASS: &str = "bg-sheffield-light";

/// The white poster card that sits inside the frame and carries everything.
///
/// The **only** element on the kiosk wearing a visible dark border (§2.3);
/// rows inside it stay border-free and lift on `shadow-lg`. It is also where
/// the kiosk's base ink lives, because the frame may not carry ink at all.
/// QA design round 1 / **QD-02** moved the inner padding from `p-10` to `p-8`
/// and folded the card's own `gap-6` in here (it used to be an inline
/// `gap-8`): at the kiosk's declared 1920 × 1080 render target those two
/// steps are 32 vertical pixels the profile rail did not have, and the rail
/// was 55 px short of its fourth boy. The arithmetic is
/// [`tv_rail_budget_px`], asserted below and in `tests/tv_tests.rs`.
pub const TV_POSTER_CARD_CLASS: &str = "flex w-full min-h-0 flex-1 flex-col gap-6 rounded-[2.5rem] border-4 border-slate-800 bg-white p-8 text-slate-800";

/// The wordmark's tracked eyebrow — the poster's quiet top line (§2.6).
///
/// Caps + tracking is reserved for this one word; instructions stay
/// sentence-case. The word is written in capitals in the markup rather than
/// set with a transform, so a reader and a `grep` both see what the
/// television shows.
pub const TV_EYEBROW_CLASS: &str = "font-bold tracking-[0.35em] text-slate-800";

/// The wordmark's loud word — the poster's outlined display red (§2.1, §2.6).
///
/// `sheffield-accent` on white is 3.16:1: AA **Large** only, which is why
/// this class is only ever applied together with [`TV_HEADING_LARGE`] (60 px)
/// at weight 800. It is declared in `palette::PALETTE_PAIRS` as the single
/// `TextSize::Large` pair on the two surfaces.
pub const TV_WORDMARK_DISPLAY_CLASS: &str =
    "font-poster font-extrabold text-sheffield-accent poster-outline";

/// The wordmark's quiet words (§2.6).
pub const TV_WORDMARK_QUIET_CLASS: &str = "font-poster font-extrabold text-slate-800";

/// A heading on a panel that is not the routine — the poster lockup belongs
/// to the routine, the heart; the others get the face and the blue (§2.6).
pub const TV_PANEL_HEADING_CLASS: &str = "font-poster font-bold text-sheffield-dark";

/// The checked checkbox's rubber stamp (§2.4).
///
/// `.stamp-check` (`input.css`, D4.4) rotates −4° and scales 1.06, with the
/// 150 ms ease-out behind a `prefers-reduced-motion` guard: the tick lands
/// slightly crooked, the way a child actually checks a box.
pub const TV_STAMP_CLASS: &str = "stamp-check";

/// The profile rail's own column.
///
/// `overflow-y-auto`, not `overflow-hidden` (QD-02): the rail is a scroll
/// container so the shell's scroll-into-view has something to scroll, and a
/// fifth profile is then reachable instead of merely focusable.
pub const TV_PROFILE_RAIL_CLASS: &str = "flex w-[26rem] shrink-0 flex-col gap-5 overflow-y-auto";

/// One boy's button on the rail, minus the focus ring and the fill.
pub const TV_PROFILE_BUTTON_CLASS: &str = "flex items-center gap-6 px-8 py-4 shadow-lg";

/// The coloured disc carrying a boy's initial.
pub const TV_PROFILE_DISC_CLASS: &str =
    "flex h-20 w-20 shrink-0 items-center justify-center rounded-full font-bold";

/// The rail's last entry — *Add a phone*, one line, pinned to the bottom.
pub const TV_JOIN_PILL_CLASS: &str = "mt-auto bg-white px-8 py-4 shadow-lg";

/// The 8/8 celebration (§2.4): the two wordmark suns turn, slowly, once every
/// eight seconds — and not at all for a viewer who has asked for less motion.
/// No confetti and no sound: the poster is exuberant, not noisy.
pub const TV_CELEBRATION_SPIN_CLASS: &str =
    "inline-block motion-safe:animate-spin motion-safe:[animation-duration:8s]";

// ---------------------------------------------------------------------------
// QA design round 1 / QD-02 — the profile rail's vertical budget
// ---------------------------------------------------------------------------
//
// Nothing here paints anything. It is the *arithmetic* of the classes above,
// written down so that "does the rail still hold four boys on a 1080-line
// television?" is a `cargo test` question rather than a screenshot question.
// No SSR test can measure a browser, so the numbers below are the measured
// heights of the two elements that are not made of spacing steps (the
// wordmark lockup and the panel-hint row) plus plain Tailwind arithmetic for
// everything else. `tests/tv_tests.rs` pins each class this depends on to the
// markup it is actually rendered into, so the sum cannot drift away from the
// page behind its back.

/// The kiosk's declared render size (`docs/device.toml`).
pub const TV_RENDER_WIDTH_PX: u32 = 1920;
/// The kiosk's declared render size (`docs/device.toml`).
pub const TV_RENDER_HEIGHT_PX: u32 = 1080;

/// One step of Tailwind's spacing scale: `p-8` is 8 × 4 px = 32 px.
pub const TV_SPACING_STEP_PX: u32 = 4;

/// [`TV_OVERSCAN_CLASS`] is `p-[5%]`, and percentage padding resolves against
/// the container's **width** on all four sides — 96 px at 1920.
pub const TV_OVERSCAN_PERCENT: u32 = 5;

/// `border-4` on [`TV_POSTER_CARD_CLASS`].
pub const TV_CARD_BORDER_PX: u32 = 4;
/// `p-8` on [`TV_POSTER_CARD_CLASS`] (was `p-10` before QD-02).
pub const TV_CARD_PADDING_STEP: u32 = 8;
/// `gap-6` on [`TV_POSTER_CARD_CLASS`] (was `gap-8` before QD-02). The card
/// stacks three children — header, body row, hints — so it pays this twice.
pub const TV_CARD_GAP_STEP: u32 = 6;

/// The wordmark lockup's measured height: a 30 px eyebrow in a 36 px line
/// box, then `gap-1`, then the 60 px `text-6xl` line, which Baloo 2's metrics
/// render as 63.2 px. Rounded up — a budget may only ever be pessimistic.
pub const TV_HEADER_PX: u32 = 104;
/// The panel-hint row: one 30 px pill (36 px line box) with `py-2`.
pub const TV_HINTS_PX: u32 = 52;

/// `gap-5` between the rail's entries.
pub const TV_RAIL_GAP_STEP: u32 = 5;
/// `h-20 w-20` — the profile disc. QD-02 took it down from `h-24`: 96 px of
/// disc is what forced a 128 px row, and four of those plus *Add a phone*
/// cannot be made to fit 1080 lines however the padding is shuffled.
pub const TV_PROFILE_DISC_PX: u32 = 80;
/// `py-4` on a profile button (was `py-6` before QD-02).
pub const TV_PROFILE_PADDING_Y_STEP: u32 = 4;
/// The *Add a phone* pill: one 36 px `text-4xl` line box with `py-4`.
///
/// QD-02 folded its second line ("Play/Pause shows the code") away — the key
/// is already on the hint row, and a focus stop nobody can see is worse than
/// a hint nobody reads.
pub const TV_JOIN_PILL_PX: u32 = 40 + 2 * TV_PROFILE_PADDING_Y_STEP * TV_SPACING_STEP_PX;

/// Height of one profile button on the rail.
pub const fn tv_profile_button_px() -> u32 {
    TV_PROFILE_DISC_PX + 2 * TV_PROFILE_PADDING_Y_STEP * TV_SPACING_STEP_PX
}

/// The vertical pixels the profile rail actually gets on a 1920 × 1080
/// television, once the frame, the card, the wordmark and the hint row have
/// taken theirs.
pub const fn tv_rail_budget_px() -> u32 {
    let overscan = TV_RENDER_WIDTH_PX * TV_OVERSCAN_PERCENT / 100;
    TV_RENDER_HEIGHT_PX
        - 2 * overscan
        - 2 * TV_CARD_BORDER_PX
        - 2 * TV_CARD_PADDING_STEP * TV_SPACING_STEP_PX
        - TV_HEADER_PX
        - 2 * TV_CARD_GAP_STEP * TV_SPACING_STEP_PX
        - TV_HINTS_PX
}

/// The vertical pixels the rail needs to show `profiles` boys **and** the
/// *Add a phone* pill without clipping either.
pub const fn tv_rail_needed_px(profiles: u32) -> u32 {
    let gap = TV_RAIL_GAP_STEP * TV_SPACING_STEP_PX;
    profiles * tv_profile_button_px() + profiles * gap + TV_JOIN_PILL_PX
}

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

    /// QD-02: the arithmetic and the classes it claims to describe are the
    /// same thing. If someone edits a class string without editing the
    /// budget (or the other way round), this fails before the television
    /// does.
    #[test]
    fn the_budget_is_made_of_the_classes_it_is_made_of() {
        let has = |class: &str, token: &str| class.split_whitespace().any(|t| t == token);

        assert_eq!(TV_OVERSCAN_CLASS, "p-[5%]");
        assert!(has(TV_POSTER_CARD_CLASS, "p-8"), "{TV_POSTER_CARD_CLASS}");
        assert!(has(TV_POSTER_CARD_CLASS, "gap-6"), "{TV_POSTER_CARD_CLASS}");
        assert!(
            has(TV_POSTER_CARD_CLASS, "border-4"),
            "{TV_POSTER_CARD_CLASS}"
        );
        assert!(
            has(TV_PROFILE_RAIL_CLASS, "gap-5"),
            "{TV_PROFILE_RAIL_CLASS}"
        );
        assert!(
            has(TV_PROFILE_RAIL_CLASS, "overflow-y-auto"),
            "the rail must be a scroll container for QD-02's scroll-into-view"
        );
        assert!(
            has(TV_PROFILE_BUTTON_CLASS, "py-4"),
            "{TV_PROFILE_BUTTON_CLASS}"
        );
        assert!(
            has(TV_PROFILE_DISC_CLASS, "h-20"),
            "{TV_PROFILE_DISC_CLASS}"
        );
        assert!(has(TV_JOIN_PILL_CLASS, "py-4"), "{TV_JOIN_PILL_CLASS}");

        assert_eq!(tv_profile_button_px(), 112);
        assert_eq!(TV_JOIN_PILL_PX, 72);
    }

    /// QD-02's actual defect, as a number: the rail was 580 px and needed
    /// 816 px, so Boy 4 lost his bottom 38 % and *Add a phone* was invisible.
    #[test]
    fn the_rail_holds_four_boys_and_the_phone_pill_at_ten_eighty() {
        let budget = tv_rail_budget_px();
        let needed = tv_rail_needed_px(4);
        assert_eq!(budget, 612, "the rail's budget at 1920x1080 moved");
        assert_eq!(needed, 600, "what four boys plus the phone pill cost moved");
        assert!(
            needed <= budget,
            "the rail needs {needed}px and has {budget}px: the fourth boy or \
             the phone pill is clipped at {TV_RENDER_WIDTH_PX}x{TV_RENDER_HEIGHT_PX}"
        );
        // ...and the pre-QD-02 geometry (p-10, gap-8, py-6, a 96px disc) did
        // not fit, which is the whole finding. Recomputed here rather than
        // asserted from memory.
        let before_budget = budget - 2 * 2 * TV_SPACING_STEP_PX - 2 * 2 * TV_SPACING_STEP_PX;
        let before_needed = 4 * (96 + 2 * 6 * TV_SPACING_STEP_PX)
            + 4 * TV_RAIL_GAP_STEP * TV_SPACING_STEP_PX
            + 40
            + 36
            + 2 * 6 * TV_SPACING_STEP_PX;
        assert_eq!(before_budget, 580);
        assert!(before_needed > before_budget, "{before_needed}");
    }

    // The "no pointer-only affordance" rule is asserted in
    // `tests/tv_tests.rs`, which greps this whole directory's source *and*
    // the rendered markup for the prefix. It deliberately is not asserted
    // here: a test written here would have to spell the forbidden prefix
    // out, and the grep would then trip over its own assertion.
}
