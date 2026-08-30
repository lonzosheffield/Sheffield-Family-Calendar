//! Emoji glyph mapping (**D4.2**, `docs/design/DESIGN_DIRECTION.md` §2.5).
//!
//! One shared mapping from a routine template's `icon_name` (seeded in
//! [`crate::server::db::SHEFFIELD_MORNING_ROUTINE`]) to the poster-style
//! emoji glyph that stands in for it, plus the panel/tab glyphs and the
//! four profile-rail "sports ball" glyphs lifted straight off the poster's
//! bottom corners.
//!
//! No icon font, no SVG sprite sheet, no network fetch: every glyph here is
//! a `'static` string the platform's own emoji font already renders. Every
//! caller wraps these in an `aria-hidden="true"` span beside real text —
//! never as the only label — and never wraps them in a `text-*` colour
//! class: emoji carry their own colour, and a colour class here would put
//! this file in the path of `tests/palette_tests.rs`'s "no source names a
//! colour outside the palette" scan, which is not a scan this module wants
//! to attract (§2.5, "no `text-*` color class").

/// Map a seeded routine `icon_name` to its poster-emoji glyph.
///
/// An unknown name — a custom task's icon, or a future seed the poster
/// mapping has not caught up with — falls back to `✅` rather than
/// rendering nothing.
pub fn icon_glyph(icon_name: &str) -> &'static str {
    match icon_name {
        "sun" => "☀️",
        "bed" => "🛏️",
        // The poster's toilet + toothbrush row; §2.8 keeps the toothbrush,
        // not the poster's 🚽.
        "sparkles" => "🪥",
        "droplet" => "🥤",
        "utensils" => "🍳",
        // The family's own running boy — the skin-tone modifier is kept
        // deliberately (§2.5): it is the family's poster, not a generic one.
        "activity" => "🏃🏾",
        "book-open" => "📖",
        "graduation-cap" => "📚",
        _ => "✅",
    }
}

/// The four sports-ball glyphs from the poster's bottom corners, keyed by a
/// 1-based rail position. A 5th+ profile cycles back to the first ball
/// rather than falling back to a placeholder — the rail has no "unknown
/// boy" case, only more boys than the poster drew.
pub fn ball_glyph(index: u32) -> &'static str {
    const BALLS: [&str; 4] = ["⚽", "🏈", "⚾", "🏀"];
    let position = index.saturating_sub(1) as usize % BALLS.len();
    BALLS[position]
}

/// The Routine panel/tab glyph — also the wordmark's two flanking suns.
pub const ROUTINE_GLYPH: &str = "☀️";
/// The Calendar/Today panel/tab glyph.
pub const CALENDAR_GLYPH: &str = "📅";
/// The Whiteboard/Board panel/tab glyph.
pub const WHITEBOARD_GLYPH: &str = "🖍️";
/// The TV Remote tab glyph.
pub const TV_REMOTE_GLYPH: &str = "📺";
/// The Settings tab glyph.
pub const SETTINGS_GLYPH: &str = "⚙️";
/// The "Add a phone" / join-QR overlay glyph.
pub const ADD_PHONE_GLYPH: &str = "📱";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_seeded_family_calendar_icon_maps_to_a_non_ascii_glyph() {
        // Mirrors `db::SHEFFIELD_MORNING_ROUTINE`'s 8 `icon_name`s without
        // pulling in `server::db` from a client-side unit test: the acceptance
        // test in `tests/glyph_tests.rs` is what checks this module stays in
        // sync with the real seed list; this one just pins the mapping's
        // shape for the icons known at the time this module was written.
        for icon_name in [
            "sun",
            "bed",
            "sparkles",
            "droplet",
            "utensils",
            "activity",
            "book-open",
            "graduation-cap",
        ] {
            let glyph = icon_glyph(icon_name);
            assert!(
                !glyph.is_ascii(),
                "icon_name {icon_name:?} mapped to an ASCII-only glyph {glyph:?}"
            );
            assert_ne!(glyph, "✅", "icon_name {icon_name:?} should not fall back");
        }
    }

    #[test]
    fn unknown_icon_names_fall_back_to_the_check() {
        for unknown in ["", "not-a-real-icon", "toilet", "GRADUATION-CAP"] {
            assert_eq!(icon_glyph(unknown), "✅", "icon_name {unknown:?}");
        }
    }

    #[test]
    fn ball_glyph_cycles_after_the_fourth_profile() {
        assert_eq!(ball_glyph(1), "⚽");
        assert_eq!(ball_glyph(2), "🏈");
        assert_eq!(ball_glyph(3), "⚾");
        assert_eq!(ball_glyph(4), "🏀");
        assert_eq!(ball_glyph(5), "⚽", "a 5th profile cycles, not falls back");
        assert_eq!(ball_glyph(8), "🏀");
    }
}
