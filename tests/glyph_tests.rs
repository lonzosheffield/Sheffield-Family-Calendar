//! D4.2 acceptance — glyph module + phone/screensaver polish
//! (`docs/design/DESIGN_DIRECTION.md` §4 D4.2, §2.5, §3.3, §3.5).
//!
//! Four assertions, lettered as the task list letters them:
//!
//! * **(a)** all 8 seeded `icon_name`s (imported straight from
//!   `db::SHEFFIELD_MORNING_ROUTINE`, the real seed data — not a copy of it)
//!   map to a non-ASCII glyph through [`glyphs::icon_glyph`]; an unknown
//!   name falls back to `✅`.
//! * **(b)** SSR of the mobile routine row contains `☀️` (the `sun` row's
//!   glyph) and does **not** contain the literal string `graduation-cap`
//!   any more — the old code printed the raw `icon_name` as a debug
//!   leftover.
//! * **(c)** SSR of [`Screensaver`] contains `Sheffield Family Hub` inside
//!   an element classed `bg-slate-800`.
//! * **(d)** `cargo test` — specifically `tests/palette_tests.rs` — stays
//!   green: no new colour classes were introduced anywhere this task
//!   touched. That is a whole-suite property, not something this file can
//!   assert on its own, so it is exercised by running the palette suite
//!   itself (`cargo test --test palette_tests`) alongside this one, not
//!   reproduced here.
//!
//! Rendering follows `tests/tv_tests.rs`'s pattern: [`RoutineRow`] takes
//! plain props and needs no app context, so it renders the same way
//! `TvSurface` does — `dioxus::ssr::render_element`, no server, no
//! database. [`Screensaver`] does read `AppState` (to know whether a
//! schedule has forced the overlay on), so its test wraps it in a small
//! harness component that provides that one piece of context directly,
//! rather than pulling in a running server to get there.

#![cfg(feature = "server")]

use dioxus::prelude::*;

use family_calendar::client::app::AppState;
use family_calendar::client::components::glyphs::icon_glyph;
use family_calendar::client::components::routine::RoutineRow;
use family_calendar::client::components::screensaver::Screensaver;
use family_calendar::server::db::SHEFFIELD_MORNING_ROUTINE;
use family_calendar::shared::types::{MaximizedView, RoutineItemView};

// ---------------------------------------------------------------------------
// (a) every seeded icon_name maps to a non-ASCII glyph; unknown -> checkmark
// ---------------------------------------------------------------------------

#[test]
fn d4_2_a_every_seeded_icon_name_maps_to_a_non_ascii_glyph() {
    assert_eq!(
        SHEFFIELD_MORNING_ROUTINE.len(),
        8,
        "the Sheffield Morning Routine poster names 8 rows"
    );

    for (title, description, icon_name) in SHEFFIELD_MORNING_ROUTINE {
        let glyph = icon_glyph(icon_name);
        assert!(
            !glyph.is_ascii(),
            "icon_name {icon_name:?} (row {title:?} / {description:?}) mapped to \
             an ASCII-only glyph {glyph:?}"
        );
        assert_ne!(
            glyph, "✅",
            "icon_name {icon_name:?} is a real seeded icon and must not fall back \
             to the unknown-icon default"
        );
    }
}

#[test]
fn d4_2_a_an_unknown_icon_name_falls_back_to_the_check() {
    for unknown in ["", "not-a-real-icon", "toilet", "GRADUATION-CAP", "🚽"] {
        assert_eq!(
            icon_glyph(unknown),
            "✅",
            "icon_name {unknown:?} should fall back to the unknown-icon check"
        );
    }
}

// ---------------------------------------------------------------------------
// (b) the mobile routine row: glyph in, the raw icon_name string out
// ---------------------------------------------------------------------------

fn fixture_item(icon_name: &str) -> RoutineItemView {
    RoutineItemView {
        template_id: 1,
        title: "Test row".to_string(),
        description: "a description".to_string(),
        icon_name: icon_name.to_string(),
        sort_order: 0,
        completed: false,
    }
}

/// A thin wrapper so `RoutineRow`'s `on_toggle: EventHandler<bool>` is built
/// *inside* a running component scope. Dioxus 0.7's `Callback::new` grabs
/// the current `Runtime` the moment a closure prop is converted, which only
/// exists once `dom.rebuild_in_place()` starts rendering an actual
/// `#[component]` — building the `EventHandler` directly inside
/// `dioxus::ssr::render_element`'s top-level `rsx!` call (as `TvSurface`'s
/// plain, callback-free `model` prop can) panics with "Must be called from
/// inside a Dioxus runtime".
#[component]
fn RoutineRowHarness(item: RoutineItemView) -> Element {
    rsx! {
        RoutineRow { item, on_toggle: move |_: bool| {} }
    }
}

/// Render [`RoutineRow`] standalone via [`RoutineRowHarness`]. It takes
/// plain props and reads no app context, the same shape
/// `tests/tv_tests.rs::render` renders `TvSurface`.
fn render_row(item: RoutineItemView) -> String {
    dioxus::ssr::render_element(rsx! {
        RoutineRowHarness { item }
    })
}

#[test]
fn d4_2_b_the_mobile_routine_row_renders_the_sun_glyph() {
    let html = render_row(fixture_item("sun"));
    assert!(
        html.contains("☀️"),
        "the `sun` row must render its poster glyph: {html}"
    );
}

#[test]
fn d4_2_b_the_mobile_routine_row_never_prints_the_raw_icon_name_again() {
    // Q1-15-style regression: the pre-D4.2 row ended with a literal
    // `{item.icon_name}` span, so the school row rendered the word
    // "graduation-cap" straight onto the phone. It must render the poster
    // glyph (📚) instead, never the identifier itself.
    let html = render_row(fixture_item("graduation-cap"));
    assert!(
        !html.contains("graduation-cap"),
        "the row must not print the raw icon_name string any more: {html}"
    );
    assert!(
        html.contains("📚"),
        "the school row must render its glyph instead: {html}"
    );
}

// ---------------------------------------------------------------------------
// (c) the screensaver caption chip
// ---------------------------------------------------------------------------

/// A tiny harness that provides the one piece of context [`Screensaver`]
/// reads (`AppState`), with `current_view` pre-set to
/// `MaximizedView::Screensaver` — the same "a schedule forced the overlay
/// on" path `ServerMessage::SetView` drives in production
/// (`src/client/components/screensaver.rs`'s `scheduled_on`). This makes the
/// overlay active without needing a running idle timer.
#[component]
fn ScreensaverForcedOn() -> Element {
    use_context_provider(|| AppState {
        current_view: Signal::new(MaximizedView::Screensaver),
        active_user_id: Signal::new(1),
    });
    rsx! {
        Screensaver {}
    }
}

fn render_screensaver_active() -> String {
    dioxus::ssr::render_element(rsx! {
        ScreensaverForcedOn {}
    })
}

#[test]
fn d4_2_c_the_screensaver_caption_names_the_hub_on_a_solid_dark_chip() {
    let html = render_screensaver_active();
    assert!(
        html.contains("Sheffield Family Hub"),
        "the active screensaver must caption itself: {html}"
    );

    // "inside an element classed bg-slate-800": locate the opening tag that
    // carries the class, then confirm the caption text falls between it and
    // its own closing tag (the chip nests only `span`s, so the first
    // `</div>` after the opening tag is its close) — the same coarse but
    // sufficient nesting check `tests/tv_tests.rs`'s tag scanner exists to
    // avoid needing an HTML crate for.
    let open_at = html
        .find("bg-slate-800")
        .expect("a bg-slate-800 element must be rendered while the overlay is active");
    let tag_start = html[..open_at]
        .rfind('<')
        .expect("bg-slate-800 must appear inside an opening tag");
    let tag_end = html[tag_start..]
        .find('>')
        .map(|offset| tag_start + offset)
        .expect("the bg-slate-800 opening tag must close");
    let close_at = html[tag_end..]
        .find("</div>")
        .map(|offset| tag_end + offset)
        .expect("the bg-slate-800 element must have a matching </div>");

    let chip_html = &html[tag_start..close_at];
    assert!(
        chip_html.contains("Sheffield Family Hub"),
        "`Sheffield Family Hub` must be inside the bg-slate-800 element, got: {chip_html}"
    );
}

#[test]
fn d4_2_c_an_inactive_screensaver_renders_nothing() {
    // The overlay must still be gate-able off — no schedule, no idle time
    // elapsed — the same as before D4.2's caption was added.
    let html = dioxus::ssr::render_element(rsx! {
        InactiveScreensaverHarness {}
    });
    assert!(
        html.trim().is_empty(),
        "an inactive screensaver must render nothing: {html}"
    );
}

#[component]
fn InactiveScreensaverHarness() -> Element {
    use_context_provider(|| AppState {
        current_view: Signal::new(MaximizedView::None),
        active_user_id: Signal::new(1),
    });
    rsx! {
        Screensaver {}
    }
}
