//! T2.1 acceptance — the Fire TV kiosk (PLAN v2 D8, PURPLE_TEAM.md §P3 T2.1).
//!
//! Six assertions, lettered as the contract letters them:
//!
//! * **(a)** a walk of the rendered TV component tree produces exactly the
//!   ordered list of focusable element ids in `tests/golden/tv_focus_order.txt`,
//!   and every one of them carries a visible focus ring class;
//! * **(b)** an injected `ServerMessage::SetView { Whiteboard }` changes the
//!   rendered view;
//! * **(c)** the same for `ServerMessage::SetActiveProfile`;
//! * **(d)** a pure-function test on the key handler for each of
//!   `ArrowUp/ArrowDown/ArrowLeft/ArrowRight/Enter/Backspace/MediaPlayPause`;
//! * **(e)** every routine item is reachable from the profile selector in
//!   ≤ 12 key presses;
//! * **(f)** typography and overscan, by grepping the compiled Tailwind
//!   classes out of the rendered markup and checking them against the
//!   committed allowlist `tests/golden/tv_type_scale.txt`.
//!
//! **No screenshot review** (the contract's words): everything here is HTML
//! the renderer produced and Rust functions the kiosk actually runs.
//!
//! The unit tests inside `src/client/components/tv/**` cover the same key
//! handler at a finer grain — one test per transition — and run in the same
//! `cargo test --features server` pass.

#![cfg(feature = "server")]

use dioxus::prelude::*;

use family_calendar::client::components::calendar::CalendarState;
use family_calendar::client::components::glyphs::icon_glyph;
use family_calendar::client::components::tv::fixture::{canonical_model, CANONICAL_ROUTINE};
use family_calendar::client::components::tv::keymap::{
    keys_debug_enabled, KeyLogEntry, TvKey, TV_KEYS,
};
use family_calendar::client::components::tv::model::{
    current_focus, focus_order, FocusId, TvLayout, TvModel, TvOverlay, TvPanel, TvState, TvZone,
};
use family_calendar::client::components::tv::nav::{on_key_for, presses_to_reach, TvAction};
use family_calendar::client::components::tv::staleness::{
    badge_is_lit, TvStaleness, STALENESS_THRESHOLD_MS,
};
use family_calendar::client::components::tv::style::{
    TV_FOCUSABLE_CLASS, TV_FOCUS_RING_ACTIVE, TV_MIN_BODY_PX, TV_MIN_HEADING_PX, TV_OVERSCAN_CLASS,
    TV_TYPE_SCALE,
};
use family_calendar::client::components::tv::surface::TvSurface;
use family_calendar::server::db::SHEFFIELD_MORNING_ROUTINE;
use family_calendar::server::health::STALENESS_THRESHOLD;
use family_calendar::shared::types::{CalendarEvent, MaximizedView, ServerMessage};

// ---------------------------------------------------------------------------
// A very small HTML tag scanner
// ---------------------------------------------------------------------------
//
// No HTML crate is added for this: the input is markup this crate produced
// two lines earlier, and a dependency added here would have to go through
// Boss (PURPLE §P4 — `Cargo.toml` is not T2.1's).

#[derive(Debug, Clone)]
struct Tag {
    name: String,
    attrs: Vec<(String, String)>,
}

impl Tag {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    fn classes(&self) -> Vec<&str> {
        self.attr("class")
            .map(|class| class.split_whitespace().collect())
            .unwrap_or_default()
    }
}

/// Every opening tag in `html`, in document order.
fn tags(html: &str) -> Vec<Tag> {
    let bytes: Vec<char> = html.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] != '<' {
            i += 1;
            continue;
        }
        // Skip closing tags, comments and doctypes.
        if !matches!(bytes.get(i + 1), Some(c) if c.is_ascii_alphabetic()) {
            i += 1;
            continue;
        }
        let start = i + 1;
        let mut j = start;
        let mut quote: Option<char> = None;
        while j < bytes.len() {
            let c = bytes[j];
            match quote {
                Some(q) if c == q => quote = None,
                Some(_) => {}
                None if c == '"' || c == '\'' => quote = Some(c),
                None if c == '>' => break,
                None => {}
            }
            j += 1;
        }
        out.push(parse_tag(&bytes[start..j.min(bytes.len())]));
        i = j + 1;
    }
    out
}

fn parse_tag(inner: &[char]) -> Tag {
    let text: String = inner.iter().collect();
    let mut chars = text.chars().peekable();

    let mut name = String::new();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() || c == '/' {
            break;
        }
        name.push(c);
        chars.next();
    }

    let mut attrs = Vec::new();
    loop {
        while chars.peek().is_some_and(|c| c.is_whitespace() || *c == '/') {
            chars.next();
        }
        let mut key = String::new();
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() || c == '=' {
                break;
            }
            key.push(c);
            chars.next();
        }
        if key.is_empty() {
            break;
        }
        let mut value = String::new();
        if chars.peek() == Some(&'=') {
            chars.next();
            match chars.peek().copied() {
                Some(q @ ('"' | '\'')) => {
                    chars.next();
                    for c in chars.by_ref() {
                        if c == q {
                            break;
                        }
                        value.push(c);
                    }
                }
                _ => {
                    while let Some(&c) = chars.peek() {
                        if c.is_whitespace() {
                            break;
                        }
                        value.push(c);
                        chars.next();
                    }
                }
            }
        }
        attrs.push((key, value));
    }

    Tag {
        name: name.to_ascii_lowercase(),
        attrs,
    }
}

/// Render the pure kiosk surface for `model` into HTML.
fn render(model: &TvModel) -> String {
    let model = model.clone();
    dioxus::ssr::render_element(rsx! { TvSurface { model } })
}

/// Every focusable element in the rendered markup, in document order.
fn focusable_tags(html: &str) -> Vec<Tag> {
    tags(html)
        .into_iter()
        .filter(|tag| tag.attr("data-tv-focus").is_some())
        .collect()
}

fn rendered_focus_ids(html: &str) -> Vec<String> {
    focusable_tags(html)
        .iter()
        .map(|tag| {
            tag.attr("id")
                .unwrap_or_else(|| panic!("a focusable <{}> with no id", tag.name))
                .to_string()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Golden files
// ---------------------------------------------------------------------------

fn golden(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("reading {path:?}: {err}"))
}

/// Parse `tv_focus_order.txt` into `(section, ids)` pairs.
fn golden_focus_order() -> Vec<(String, Vec<String>)> {
    let mut sections: Vec<(String, Vec<String>)> = Vec::new();
    for line in golden("tv_focus_order.txt").lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(header) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            sections.push((header.to_string(), Vec::new()));
        } else {
            sections
                .last_mut()
                .expect("an id outside any [section] header")
                .1
                .push(line.to_string());
        }
    }
    sections
}

/// The allowlisted font sizes, as `(class, px)`.
fn golden_type_scale() -> Vec<(String, u32)> {
    golden("tv_type_scale.txt")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let (class, px) = line.split_once(' ').expect("`<class> <px>`");
            (class.to_string(), px.trim().parse::<u32>().expect("px"))
        })
        .collect()
}

/// The models the golden file's four sections describe.
fn golden_models() -> Vec<(String, TvModel)> {
    let mut out = Vec::new();
    for panel in TvPanel::ALL {
        let mut model = canonical_model();
        model.state.panel = panel;
        out.push((format!("panel:{}", panel.slug()), model));
    }
    let mut overlay = canonical_model();
    overlay.state.overlay = TvOverlay::JoinQr;
    out.push(("overlay:join-qr".to_string(), overlay));
    out
}

// ---------------------------------------------------------------------------
// (a) deterministic focus order + a ring on every focusable
// ---------------------------------------------------------------------------

#[test]
fn t2_1_a_the_rendered_focus_order_matches_the_golden_file() {
    let golden = golden_focus_order();
    let models = golden_models();
    assert_eq!(
        golden.len(),
        models.len(),
        "the golden file has {} sections for {} rendered states",
        golden.len(),
        models.len()
    );

    for ((section, expected), (name, model)) in golden.iter().zip(models.iter()) {
        assert_eq!(section, name, "golden sections are out of order");

        // The order the remote walks...
        let model_order: Vec<String> = focus_order(model)
            .iter()
            .map(|focus| focus.dom_id())
            .collect();
        assert_eq!(&model_order, expected, "[{section}] focus_order()");

        // ...and the order the DOM presents, which must be the same list.
        let html = render(model);
        assert_eq!(&rendered_focus_ids(&html), expected, "[{section}] rendered");
    }
}

#[test]
fn t2_1_a_every_focusable_element_carries_a_visible_focus_ring() {
    for (section, model) in golden_models() {
        let html = render(&model);
        let focusables = focusable_tags(&html);
        assert!(!focusables.is_empty(), "[{section}] nothing is focusable");

        for tag in &focusables {
            let id = tag.attr("id").unwrap_or_default();
            let classes = tag.classes();
            for required in TV_FOCUSABLE_CLASS.split_whitespace() {
                assert!(
                    classes.contains(&required),
                    "[{section}] #{id} is missing the focus-ring class `{required}`"
                );
            }
        }
    }
}

#[test]
fn t2_1_a_exactly_one_element_wears_the_live_ring_and_it_is_the_focused_one() {
    for (section, model) in golden_models() {
        let html = render(&model);
        let lit: Vec<String> = focusable_tags(&html)
            .iter()
            .filter(|tag| tag.classes().contains(&TV_FOCUS_RING_ACTIVE))
            .map(|tag| tag.attr("id").unwrap_or_default().to_string())
            .collect();

        assert_eq!(lit.len(), 1, "[{section}] {lit:?} elements lit");
        let focused = current_focus(&model).expect("something is focused");
        assert_eq!(lit[0], focused.dom_id(), "[{section}]");
    }
}

#[test]
fn t2_1_a_the_focus_order_does_not_depend_on_how_often_it_is_rendered() {
    // "Deterministic" is the word in D8. Rendering the same model twice, and
    // walking the model twice, must produce byte-identical lists.
    let model = canonical_model();
    assert_eq!(render(&model), render(&model));
    assert_eq!(focus_order(&model), focus_order(&model));
}

// ---------------------------------------------------------------------------
// (b) / (c) a phone steers the television
// ---------------------------------------------------------------------------

fn panel_attr(html: &str) -> String {
    tags(html)
        .iter()
        .find_map(|tag| tag.attr("data-tv-panel").map(str::to_string))
        .expect("the surface stamps data-tv-panel")
}

fn profile_attr(html: &str) -> String {
    tags(html)
        .iter()
        .find_map(|tag| tag.attr("data-tv-profile").map(str::to_string))
        .expect("the surface stamps data-tv-profile")
}

#[test]
fn t2_1_b_an_injected_set_view_changes_the_rendered_view() {
    let mut model = canonical_model();
    let before = render(&model);
    assert_eq!(panel_attr(&before), "routine");
    assert!(before.contains("Morning Routine"));

    let changed = model.apply_server_message(&ServerMessage::SetView {
        view: MaximizedView::Whiteboard,
    });
    assert!(changed, "SetView must move the kiosk");

    let after = render(&model);
    assert_eq!(panel_attr(&after), "whiteboard");
    assert!(
        after.contains("Drawing happens on a phone"),
        "the whiteboard panel did not render"
    );
    assert_ne!(before, after);
}

#[test]
fn t2_1_b_set_view_reaches_every_panel_including_a_phones_restore() {
    for (view, expected) in [
        (MaximizedView::Calendar, "calendar"),
        (MaximizedView::Whiteboard, "whiteboard"),
        (MaximizedView::Routine, "routine"),
        // "Restore" on the phone puts the television back on the routine.
        (MaximizedView::None, "routine"),
    ] {
        let mut model = canonical_model();
        model.state.panel = TvPanel::Whiteboard;
        model.apply_server_message(&ServerMessage::SetView { view });
        assert_eq!(panel_attr(&render(&model)), expected, "{view:?}");
    }
}

#[test]
fn t2_1_c_an_injected_set_active_profile_changes_the_rendered_profile() {
    let mut model = canonical_model();
    let before = render(&model);
    assert_eq!(profile_attr(&before), "1");

    let changed = model.apply_server_message(&ServerMessage::SetActiveProfile { user_id: 3 });
    assert!(changed, "SetActiveProfile must move the kiosk");

    let after = render(&model);
    assert_eq!(profile_attr(&after), "3");

    // ...and the rail agrees: profile 3 is both current and focused.
    let tag = focusable_tags(&after)
        .into_iter()
        .find(|tag| tag.attr("id") == Some("tv-profile-3"))
        .expect("profile 3 is on the rail");
    assert_eq!(tag.attr("aria-current"), Some("true"));
    assert!(tag.classes().contains(&TV_FOCUS_RING_ACTIVE));
}

// ---------------------------------------------------------------------------
// (d) one transition per key in the D8 map — and no Escape
// ---------------------------------------------------------------------------

#[test]
fn t2_1_d_every_key_in_the_d8_map_has_a_defined_transition() {
    let model = canonical_model();

    // ArrowUp / ArrowDown on the rail: switch profile.
    let down = on_key_for(&model, TvKey::Down);
    assert_eq!(down.action, TvAction::SelectProfile(2));
    assert_eq!(down.state.rail_index, 1);

    let up = on_key_for(&model, TvKey::Up);
    assert_eq!(up.state.rail_index, model.profiles.len(), "wraps to the QR");

    // ArrowLeft / ArrowRight: cycle panels.
    assert_eq!(
        on_key_for(&model, TvKey::Right).state.panel,
        TvPanel::Calendar
    );
    assert_eq!(
        on_key_for(&model, TvKey::Left).state.panel,
        TvPanel::Whiteboard
    );

    // Enter: into the focused profile's list.
    let enter = on_key_for(&model, TvKey::Enter);
    assert_eq!(enter.state.zone, TvZone::PanelBody);
    assert_eq!(enter.action, TvAction::SelectProfile(1));

    // Enter again, now in the list: toggle.
    let mut in_list = model.clone();
    in_list.state = enter.state;
    assert_eq!(
        on_key_for(&in_list, TvKey::Enter).action,
        TvAction::Activate(FocusId::RoutineItem(1))
    );

    // Backspace: out of the list, then home.
    let back = on_key_for(&in_list, TvKey::Back);
    assert_eq!(back.state.zone, TvZone::ProfileRail);

    let mut on_calendar = model.clone();
    on_calendar.state.panel = TvPanel::Calendar;
    assert_eq!(
        on_key_for(&on_calendar, TvKey::Back).state.panel,
        TvPanel::Routine
    );

    // MediaPlayPause: the phone-join QR.
    let play = on_key_for(&model, TvKey::PlayPause);
    assert_eq!(play.state.overlay, TvOverlay::JoinQr);
    assert_eq!(play.action, TvAction::OpenOverlay(TvOverlay::JoinQr));

    // ...and every one of the seven is reachable from a browser key name.
    for key in TV_KEYS {
        assert_eq!(TvKey::from_key(key.canonical_key_name()), Some(key));
    }
}

#[test]
fn t2_1_d_there_is_no_escape_key_anywhere_in_the_kiosk() {
    // R-11: a Fire TV remote has no Escape. A code path only reachable with
    // one is a code path the family cannot use.
    assert_eq!(TvKey::from_key("Escape"), None);

    let logged = KeyLogEntry::new("Escape", "Escape");
    assert_eq!(logged.mapped, None);
    assert_eq!(logged.action(), "ignored");

    for (path, source) in tv_sources() {
        assert!(
            !source.contains("\"Escape\"") || path.ends_with("keymap.rs"),
            "{path} matches on Escape; only keymap.rs may name it, to refuse it"
        );
    }
}

// ---------------------------------------------------------------------------
// (e) every routine item within twelve presses
// ---------------------------------------------------------------------------

#[test]
fn t2_1_e_every_routine_item_is_within_twelve_presses_of_the_profile_selector() {
    let model = canonical_model();
    assert_eq!(
        model.routine.len(),
        SHEFFIELD_MORNING_ROUTINE.len(),
        "the fixture has drifted from the seeded morning routine — regenerate \
         tests/golden/tv_focus_order.txt"
    );
    assert_eq!(CANONICAL_ROUTINE.len(), SHEFFIELD_MORNING_ROUTINE.len());

    let start = TvState::initial();
    assert_eq!(
        start.zone,
        TvZone::ProfileRail,
        "the search starts on the rail"
    );

    let mut worst = 0;
    for item in &model.routine {
        let target = FocusId::RoutineItem(item.template_id);
        let presses = presses_to_reach(&model, start, &target)
            .unwrap_or_else(|| panic!("{} is unreachable by remote", target.dom_id()));
        assert!(presses <= 12, "{} took {presses} presses", target.dom_id());
        worst = worst.max(presses);
    }
    println!("worst-case routine item: {worst} key presses (budget 12)");
}

#[test]
fn t2_1_e_a_child_completes_the_whole_routine_with_the_remote_alone() {
    // R-12 / D1. Walk the eight items pressing Enter on each and assert the
    // shell was asked to toggle every one, using nothing but the seven keys.
    let mut model = canonical_model();
    let expected: Vec<FocusId> = model
        .routine
        .iter()
        .map(|item| FocusId::RoutineItem(item.template_id))
        .collect();

    let mut toggled = Vec::new();
    let mut presses = 0;

    let outcome = on_key_for(&model, TvKey::Enter);
    model.state = outcome.state;
    presses += 1;

    for _ in 0..expected.len() {
        let outcome = on_key_for(&model, TvKey::Enter);
        model.state = outcome.state;
        presses += 1;
        if let TvAction::Activate(focus) = outcome.action {
            toggled.push(focus);
        }
        let outcome = on_key_for(&model, TvKey::Down);
        model.state = outcome.state;
        presses += 1;
    }

    assert_eq!(toggled, expected);
    println!("full routine completed in {presses} key presses");
}

// ---------------------------------------------------------------------------
// (f) typography, overscan, and no pointer-only affordances
// ---------------------------------------------------------------------------

/// Is `class` a Tailwind font-size utility? (`text-slate-500` and
/// `text-center` are colour and alignment and are not.)
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
fn t2_1_f_every_rendered_font_size_is_on_the_committed_allowlist() {
    let allowlist = golden_type_scale();
    assert!(
        allowlist.len() <= 6,
        "the type scale has grown to {} sizes; D8/T3.4 cap it at six",
        allowlist.len()
    );
    for (class, px) in &allowlist {
        assert!(
            *px >= TV_MIN_BODY_PX,
            "{class} is {px}px, under the {TV_MIN_BODY_PX}px body minimum"
        );
    }
    // The constants the components actually use are the same table.
    let from_code: Vec<(String, u32)> = TV_TYPE_SCALE
        .iter()
        .map(|(class, px)| ((*class).to_string(), *px))
        .collect();
    assert_eq!(from_code, allowlist, "tv::style drifted from the allowlist");

    let mut seen: Vec<String> = Vec::new();
    for (section, mut model) in golden_models() {
        model.keys_debug = true;
        model.key_log = vec![KeyLogEntry::new("ArrowDown", "ArrowDown")];
        let html = render(&model);
        for tag in tags(&html) {
            for class in tag.classes() {
                let Some(size) = font_size_class(class) else {
                    continue;
                };
                assert!(
                    allowlist.iter().any(|(allowed, _)| allowed == size),
                    "[{section}] <{}> uses `{size}`, which is not on the allowlist",
                    tag.name
                );
                if !seen.iter().any(|s| s == size) {
                    seen.push(size.to_string());
                }
            }
        }
    }
    assert!(!seen.is_empty(), "no font sizes were rendered at all");
}

#[test]
fn t2_1_f_every_heading_clears_forty_four_pixels() {
    let allowlist = golden_type_scale();
    for (section, model) in golden_models() {
        let html = render(&model);
        let mut headings = 0;
        for tag in tags(&html) {
            if !matches!(tag.name.as_str(), "h1" | "h2" | "h3") {
                continue;
            }
            headings += 1;
            let size = tag
                .classes()
                .into_iter()
                .find_map(font_size_class)
                .unwrap_or_else(|| panic!("[{section}] <{}> has no font size", tag.name));
            let px = allowlist
                .iter()
                .find(|(class, _)| class == size)
                .map(|(_, px)| *px)
                .expect("an allowlisted size");
            assert!(
                px >= TV_MIN_HEADING_PX,
                "[{section}] <{}> is {px}px, under the {TV_MIN_HEADING_PX}px heading minimum",
                tag.name
            );
        }
        assert!(headings > 0, "[{section}] rendered no heading at all");
    }
}

#[test]
fn t2_1_f_every_full_screen_container_carries_the_five_percent_overscan() {
    for (section, model) in golden_models() {
        let html = render(&model);
        let root = tags(&html)
            .into_iter()
            .find(|tag| tag.attr("data-tv-surface").is_some())
            .unwrap_or_else(|| panic!("[{section}] no surface root"));
        assert!(
            root.classes().contains(&TV_OVERSCAN_CLASS),
            "[{section}] the surface root is missing `{TV_OVERSCAN_CLASS}`"
        );
    }
}

#[test]
fn t2_1_f_the_kiosk_has_no_hover_only_affordance() {
    // A D-pad has no pointer: anything that only reveals itself on hover is
    // invisible on the television (D8, and T3.4 asserts the same thing).
    for (section, mut model) in golden_models() {
        model.keys_debug = true;
        let html = render(&model);
        assert!(
            !html.contains("hover:"),
            "[{section}] rendered a `hover:` class"
        );
    }
    for (path, source) in tv_sources() {
        assert!(!source.contains("hover:"), "{path} uses a `hover:` class");
    }
}

// ---------------------------------------------------------------------------
// The rest of D8: the QR overlay, `?keys=1`, the permanent status line
// ---------------------------------------------------------------------------

#[test]
fn the_join_qr_overlay_shows_the_https_phone_url_and_a_scannable_code() {
    let mut model = canonical_model();
    model.state.overlay = TvOverlay::JoinQr;
    let html = render(&model);

    assert!(html.contains("https://10.0.0.42:8443/m"), "{html}");
    assert!(html.contains("<svg"), "the QR did not render");
    assert!(html.contains("data-tv-overlay=\"join-qr\""));

    // The overlay owns the focus order: one target, and Back closes it.
    assert_eq!(rendered_focus_ids(&html), vec!["tv-overlay-close"]);
    let closed = on_key_for(&model, TvKey::Back);
    assert_eq!(closed.state.overlay, TvOverlay::None);
    assert_eq!(closed.action, TvAction::CloseOverlay);
}

#[test]
fn the_join_qr_overlay_still_renders_before_the_hub_knows_its_own_address() {
    let mut model = canonical_model();
    model.state.overlay = TvOverlay::JoinQr;
    model.join_url = None;
    let html = render(&model);
    assert!(html.contains("Waiting for the hub"), "{html}");
    assert_eq!(rendered_focus_ids(&html), vec!["tv-overlay-close"]);
}

#[test]
fn the_key_code_debug_overlay_is_off_unless_keys_equals_one() {
    let mut model = canonical_model();
    assert!(!render(&model).contains("tv-keys-overlay"));

    model.keys_debug = keys_debug_enabled("?keys=1");
    model.key_log = vec![
        KeyLogEntry::new("ArrowDown", "ArrowDown"),
        KeyLogEntry::new("Escape", "Escape"),
    ];
    let html = render(&model);

    assert!(html.contains("tv-keys-overlay"));
    assert!(html.contains("ArrowDown"), "the real key name is shown");
    assert!(html.contains("Escape"), "an ignored key is still reported");
    assert!(html.contains("ignored"));

    // A debug HUD that captured the D-pad would defeat its own purpose.
    assert!(
        !rendered_focus_ids(&html)
            .iter()
            .any(|id| id.contains("keys")),
        "the debug overlay must not be focusable"
    );
}

#[test]
fn the_updated_line_is_permanent_and_the_badge_is_not() {
    let mut model = canonical_model();
    model.connected = true;
    model.stale = false;
    let healthy = render(&model);
    assert!(healthy.contains("updated 07:42"));
    assert!(!healthy.contains("tv-disconnected-badge"));

    model.connected = false;
    let dropped = render(&model);
    assert!(dropped.contains("updated 07:42"), "the line is permanent");
    assert!(dropped.contains("tv-disconnected-badge"));

    model.connected = true;
    model.stale = true;
    assert!(render(&model).contains("tv-disconnected-badge"));

    // ...and before the hub has ever answered, the line still exists.
    model.updated_at = None;
    assert!(render(&model).contains("updated"));
}

#[test]
fn the_kiosk_badge_keeps_the_servers_ninety_second_semantics() {
    // The client port of T1.7's tracker must not drift from the server's.
    assert_eq!(
        STALENESS_THRESHOLD_MS,
        STALENESS_THRESHOLD.as_millis() as u64
    );

    let mut tracker = TvStaleness::new(0);
    assert!(!badge_is_lit(true, &tracker, 90_000));
    assert!(badge_is_lit(true, &tracker, 90_001));
    tracker.record_message(90_001);
    assert!(!badge_is_lit(true, &tracker, 91_000));
    assert!(
        badge_is_lit(false, &tracker, 91_000),
        "a dropped socket lights it"
    );
}

// ---------------------------------------------------------------------------
// Source-level guards
// ---------------------------------------------------------------------------

/// Every `.rs` file T2.1 owns, as `(path, source)`, **plus**
/// `src/client/components/screensaver.rs` (QA round 1, Q1-14): `KioskDashboard`
/// (`src/client/app.rs`) layers `Screensaver {}` full-screen over `TvShell {}`
/// on `/tv` only — idle-triggered or schedule-forced — so it is exactly as
/// pointer-free-or-bust as everything under `tv/`, even though the component
/// itself lives one directory up (it is T2.7's file, not T2.1's).
fn tv_sources() -> Vec<(String, String)> {
    let components_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("client")
        .join("components");
    let dir = components_dir.join("tv");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap_or_else(|err| panic!("reading {dir:?}: {err}")) {
        let path = entry.expect("a dir entry").path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            let source = std::fs::read_to_string(&path).expect("readable");
            out.push((path.display().to_string(), source));
        }
    }
    assert!(out.len() >= 8, "expected the whole tv module, got {out:?}");

    let screensaver_path = components_dir.join("screensaver.rs");
    let screensaver_source = std::fs::read_to_string(&screensaver_path)
        .unwrap_or_else(|err| panic!("reading {screensaver_path:?}: {err}"));
    out.push((screensaver_path.display().to_string(), screensaver_source));

    out
}

#[test]
fn the_kiosk_never_reaches_for_a_pointer_event() {
    // The scope line (PURPLE §P5.5 default 35): the television is driven by a
    // remote. A `onclick`-only control on `/tv` is a control a child cannot
    // press.
    for (path, source) in tv_sources() {
        for pointer in ["onclick:", "onpointerdown:", "onmouseover:", "ondblclick:"] {
            assert!(
                !source.contains(pointer),
                "{path} wires up `{pointer}` — the television has no pointer"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// T2.4 (e) on the television — a failed fetch is not an empty day (W3, Q1-12)
// ---------------------------------------------------------------------------

/// The kiosk is the primary display, so the W3 states have to reach it too.
///
/// Before this, `TvModel.events` was a bare `Vec` and `tv/shell.rs` folded
/// `Some(Err(_))` and `None` into `Vec::new()`: a hub that could not be
/// reached rendered "Nothing on the calendar today." — the family reads a
/// broken hub as a free morning. Each of the four states must now render as
/// itself, and only `Ready` may contribute focusable rows.
#[test]
fn t2_4_e_a_failed_calendar_fetch_is_not_rendered_as_an_empty_day() {
    const EMPTY_SENTENCE: &str = "Nothing on the calendar today.";

    fn calendar_model(events: CalendarState<Vec<CalendarEvent>>) -> TvModel {
        let mut model = canonical_model();
        model.state.panel = TvPanel::Calendar;
        model.events = events;
        model
    }

    // The state the bug produced: the hub answered with an error.
    let failed = calendar_model(CalendarState::Error("pool closed".to_string()));
    let html = render(&failed);
    assert!(
        !html.contains(EMPTY_SENTENCE),
        "an unreachable hub still renders the empty day's words:\n{html}"
    );
    assert!(
        html.contains("reach the hub"),
        "the error state says nothing about the hub:\n{html}"
    );
    assert_eq!(
        TvLayout::of(&failed).body_len(TvPanel::Calendar),
        0,
        "the error sentence must not be focusable"
    );
    assert!(
        rendered_focus_ids(&html)
            .iter()
            .all(|id| !id.starts_with("tv-event-")),
        "an error state rendered event rows: {html}"
    );

    // The first paint, before the hub has answered at all.
    let loading = calendar_model(CalendarState::Loading);
    let html = render(&loading);
    assert!(
        !html.contains(EMPTY_SENTENCE),
        "the first paint borrows the empty day's words:\n{html}"
    );
    assert!(
        html.contains("Loading the calendar"),
        "the loading state does not say it is loading:\n{html}"
    );
    assert_eq!(TvLayout::of(&loading).body_len(TvPanel::Calendar), 0);

    // A genuinely empty day keeps the sentence it has always had.
    let empty = calendar_model(CalendarState::Empty);
    let html = render(&empty);
    assert!(
        html.contains(EMPTY_SENTENCE),
        "a real empty day lost its sentence:\n{html}"
    );
    assert_eq!(TvLayout::of(&empty).body_len(TvPanel::Calendar), 0);

    // ...and a day with events still lists them, focusably.
    let fixture = canonical_model();
    let events = fixture
        .events
        .ready()
        .expect("the fixture calendar is Ready")
        .clone();
    assert!(!events.is_empty(), "the fixture has no events to list");
    let ready = calendar_model(CalendarState::Ready(events.clone()));
    let html = render(&ready);
    assert!(!html.contains(EMPTY_SENTENCE));
    assert_eq!(
        TvLayout::of(&ready).body_len(TvPanel::Calendar),
        events.len()
    );
    for event in &events {
        assert!(
            html.contains(&event.summary),
            "`{}` is missing from the Ready calendar:\n{html}",
            event.summary
        );
    }

    // All four states name themselves, which is what the `data-calendar-state`
    // diagnostics rely on and what makes the three failures distinguishable.
    let names: Vec<&str> = [failed.events, loading.events, empty.events, ready.events]
        .iter()
        .map(CalendarState::name)
        .collect();
    assert_eq!(names, vec!["error", "loading", "empty", "ready"]);
}

// ---------------------------------------------------------------------------
// D4.3 — the kiosk becomes the poster
// (`docs/design/DESIGN_DIRECTION.md` §3.1, §3.2, §3.4; acceptance (a)–(g))
// ---------------------------------------------------------------------------
//
// Seven assertions, lettered as §4 letters them. (d), (e) and (f) are the
// *unchanged* T2.1/T3.4 assertions above and in `tests/palette_tests.rs` —
// the poster had to land green against them, not around them — so the test
// here re-states them as D4.3's own contract rather than duplicating their
// machinery: the golden focus-order file must still describe the same 29
// ids, the type-scale golden must still be the four D8 sizes, and neither
// the poster's source nor its markup may name a pointer-only variant.

/// The first `<h1 ...>...</h1>` in `html`, opening tag included.
fn first_h1(html: &str) -> String {
    let start = html.find("<h1").expect("the surface renders an <h1>");
    let end = html[start..].find("</h1>").expect("an unclosed <h1>") + start;
    html[start..end].to_string()
}

/// The canonical kiosk with the eight *seeded* icons rather than the
/// fixture's eight suns, so (b) is a real test of the mapping.
fn seeded_icon_model() -> TvModel {
    let mut model = canonical_model();
    assert_eq!(
        model.routine.len(),
        SHEFFIELD_MORNING_ROUTINE.len(),
        "the fixture routine and the seeded routine are different lengths"
    );
    for (item, (title, description, icon)) in
        model.routine.iter_mut().zip(SHEFFIELD_MORNING_ROUTINE)
    {
        item.title = title.to_string();
        item.description = description.to_string();
        item.icon_name = icon.to_string();
    }
    model
}

/// (a) The wordmark: the poster's stacked headline, sun first.
#[test]
fn d4_3_a_the_routine_panel_wears_the_poster_wordmark() {
    let html = render(&canonical_model());

    // The tracked eyebrow, in capitals in the markup — not a CSS transform,
    // so what a reader greps is what the television shows.
    let eyebrow = tags(&html)
        .into_iter()
        .find(|tag| tag.classes().contains(&"tracking-[0.35em]"))
        .expect("no element carries `tracking-[0.35em]`");
    assert!(
        html.contains("SHEFFIELD"),
        "the wordmark eyebrow does not say SHEFFIELD:\n{html}"
    );
    assert!(
        eyebrow.classes().contains(&"text-3xl"),
        "the eyebrow is off the type scale: {:?}",
        eyebrow.classes()
    );

    // The sun comes before the h1's text, exactly as it does on the poster.
    let h1 = first_h1(&html);
    let sun = h1.find('\u{2600}').expect("no sun in the wordmark's <h1>");
    let morning = h1.find("Morning").expect("no `Morning` in the wordmark");
    assert!(
        sun < morning,
        "the sun renders after the wordmark's text:\n{h1}"
    );
    assert_eq!(
        h1.matches('\u{2600}').count(),
        2,
        "the wordmark must be flanked by exactly two suns (§2.8):\n{h1}"
    );

    // "Morning" is the outlined display red; "Routine" is quiet ink. The
    // pair `text-sheffield-accent` on `bg-white` is declared `Large` in
    // `palette::PALETTE_PAIRS` and only legal at >= 44px/800, which is why
    // the <h1> itself must carry the 60px size.
    assert!(h1.contains("text-sheffield-accent"), "{h1}");
    assert!(h1.contains("poster-outline"), "{h1}");
    assert!(h1.contains("font-poster"), "{h1}");
    assert!(h1.contains("text-6xl"), "{h1}");
    assert!(h1.contains("Routine"), "{h1}");

    // ...and the lockup belongs to the routine alone (§2.6).
    let mut calendar = canonical_model();
    calendar.state.panel = TvPanel::Calendar;
    assert!(
        !first_h1(&render(&calendar)).contains("poster-outline"),
        "the Today panel borrowed the routine's wordmark"
    );
}

/// (b) Row anatomy: every routine row leads with its own poster glyph.
#[test]
fn d4_3_b_every_routine_row_renders_its_icon_glyph() {
    let model = seeded_icon_model();
    let html = render(&model);

    for item in &model.routine {
        let glyph = icon_glyph(&item.icon_name);
        assert!(
            html.contains(glyph),
            "the `{}` row is missing its glyph {glyph:?}:\n{html}",
            item.icon_name
        );
        assert_ne!(
            glyph, "\u{2705}",
            "`{}` fell back to the unknown-icon check",
            item.icon_name
        );
        // ...and the *why* is in parentheses after the instruction (§1.3).
        // The seeded whys carry apostrophes ("God's provision"), which the
        // renderer escapes, so the needle is escaped the same way.
        let why = item
            .description
            .replace('&', "&amp;")
            .replace('\'', "&#39;");
        assert!(
            html.contains(&format!("({why})")),
            "the `{}` row does not put its why in parentheses:\n{html}",
            item.icon_name
        );
    }

    // A custom task with no photo still reads as a poster row.
    assert!(
        html.contains("\u{2705}"),
        "the photo-less custom task lost its fallback glyph"
    );
}

/// (c) The frame and the card.
#[test]
fn d4_3_c_the_frame_is_the_overscan_band_and_the_card_is_the_only_one() {
    for (section, model) in golden_models() {
        let html = render(&model);
        let all = tags(&html);

        let root = all
            .iter()
            .find(|tag| tag.attr("data-tv-surface").is_some())
            .unwrap_or_else(|| panic!("[{section}] no surface root"));
        let classes = root.classes();
        assert!(
            classes.contains(&"bg-sheffield-light"),
            "[{section}] the root is not the poster's blue frame: {classes:?}"
        );
        assert!(
            classes.contains(&TV_OVERSCAN_CLASS),
            "[{section}] the frame is not the 5% overscan band: {classes:?}"
        );
        // The frame carries no ink: §2.2 makes `sheffield-light` decorative
        // only, so a text colour on the root would be a pair that cannot be
        // declared.
        assert!(
            !classes.iter().any(|c| c.starts_with("text-slate")
                || c.starts_with("text-sheffield")
                || *c == "text-white"),
            "[{section}] the frame carries ink: {classes:?}"
        );

        let cards: Vec<&Tag> = all
            .iter()
            .filter(|tag| {
                let classes = tag.classes();
                classes.contains(&"border-slate-800") && classes.contains(&"bg-white")
            })
            .collect();
        assert_eq!(
            cards.len(),
            1,
            "[{section}] expected exactly one poster card, found {}",
            cards.len()
        );
        assert!(
            cards[0].classes().contains(&"border-4"),
            "[{section}] the poster card's border is not the poster's: {:?}",
            cards[0].classes()
        );
    }
}

/// (d)–(f) The three contracts the poster had to land green *against*.
#[test]
fn d4_3_d_e_f_the_poster_did_not_move_the_focus_order_or_the_type_scale() {
    // (d) the focus-order golden file. D4.3 added glyphs, a frame and a card
    // — no focusables — so this file may not have moved. Its shape is pinned
    // here so a future edit fails twice: once in `t2_1_a_...`, and once as a
    // deliberate D4.3 contract.
    let ids: Vec<String> = golden_focus_order()
        .into_iter()
        .flat_map(|(_, ids)| ids)
        .collect();
    assert_eq!(
        ids.len(),
        15 + 8 + 5 + 1,
        "the golden focus order changed length: {ids:?}"
    );
    for (section, model) in golden_models() {
        let rendered = rendered_focus_ids(&render(&model));
        let expected: Vec<String> = golden_focus_order()
            .into_iter()
            .find(|(name, _)| *name == section)
            .expect("a golden section")
            .1;
        assert_eq!(rendered, expected, "[{section}] the poster moved the focus");
    }

    // (e) the type-scale golden: still the four D8 sizes, and the <h1> the
    // wordmark now builds is still 60px.
    assert_eq!(
        golden_type_scale(),
        vec![
            ("text-3xl".to_string(), 30),
            ("text-4xl".to_string(), 36),
            ("text-5xl".to_string(), 48),
            ("text-6xl".to_string(), 60),
        ],
        "the type-scale golden moved"
    );
    assert_eq!(TV_TYPE_SCALE.len(), 4);

    // (f) no pointer-only affordance, in the poster's own new markup —
    // including the 8/8 state, which no golden model renders.
    for (section, mut model) in golden_models() {
        model.routine.iter_mut().for_each(|i| i.completed = true);
        let html = render(&model);
        assert!(
            !html.contains(concat!("hover", ":")),
            "[{section}] the poster rendered a pointer-only variant"
        );
    }
}

/// (g) The 8/8 celebration: the count chip flips accent -> sun.
#[test]
fn d4_3_g_the_count_chip_turns_sun_yellow_at_eight_of_eight() {
    let chip_ground = |model: &TvModel| -> String {
        let html = render(model);
        let tag = tags(&html)
            .into_iter()
            .find(|tag| tag.attr("id") == Some("tv-routine-count"))
            .expect("the routine panel renders a count chip");
        tag.classes()
            .into_iter()
            .find(|class| class.starts_with("bg-"))
            .expect("the chip has a ground")
            .to_string()
    };

    // 1 / 8 — the working morning.
    let mut one = canonical_model();
    one.routine
        .iter_mut()
        .enumerate()
        .for_each(|(index, item)| item.completed = index == 0);
    assert_eq!(chip_ground(&one), "bg-sheffield-accent");
    assert!(render(&one).contains("1 / 8"));

    // 8 / 8 — the poster's own exuberance, on a ground the chip's
    // `text-slate-800` clears at 10.4:1 (a declared pair).
    let mut all = canonical_model();
    all.routine
        .iter_mut()
        .for_each(|item| item.completed = true);
    let html = render(&all);
    assert_eq!(chip_ground(&all), "bg-sheffield-sun");
    assert!(html.contains("8 / 8"));
    assert!(
        html.contains('\u{2600}'),
        "the finished routine lost its celebration sun"
    );
    // ...and the wordmark's suns turn, for anyone who has not asked for less
    // motion (§2.4).
    assert!(
        first_h1(&html).contains("motion-safe:animate-spin"),
        "the 8/8 wordmark suns do not turn"
    );
    assert!(
        !first_h1(&render(&one)).contains("animate-spin"),
        "the suns turn before the routine is finished"
    );
}
