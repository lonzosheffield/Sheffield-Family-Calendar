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

// ===========================================================================
// HS5 acceptance (a)–(k) — the phone's School tab
// (`docs/homeschool/PLAN_HOMESCHOOL.md` §3 row HS5)
// ===========================================================================
//
// The whole School surface is presentational: every pane takes plain props and
// an action sink, and only `homeschool::School` itself knows a server exists.
// So the assertions below render the real components with
// `dioxus::ssr::render_element` — no server, no database, no browser — which is
// the same shape `RoutineRow` above is rendered in, extended with the one piece
// of context these panes read: the parent session signal `MobileShell`
// provides.
//
// The fixture is the synthetic curriculum of §2 H5
// (`tests/fixtures/curricula/sample-year.toml`), rebuilt in Rust here for the
// same reason `src/shared/homeschool.rs`'s own tests rebuild it: this is a
// client-side render test with no TOML parser and no pool. Its shape is pinned
// by review finding P-15, so the two agree by construction. Every book and
// subject name in it is invented (§0 N1).

use family_calendar::client::components::homeschool::day_sheet::{DaySheet, EXTRA_CATEGORIES};
use family_calendar::client::components::homeschool::month::MonthPanel;
use family_calendar::client::components::homeschool::row::CATCH_UP_CHIP_CLASS;
use family_calendar::client::components::homeschool::today::TodayPanel;
use family_calendar::client::components::homeschool::year::YearPanel;
use family_calendar::client::components::homeschool::{month_label, SchoolAction};
use family_calendar::client::components::mobile::queue::{OfflineQueue, QueuedMutation};
use family_calendar::client::components::mobile::remote::VIEWS;
use family_calendar::client::components::mobile::session::SessionState;
use family_calendar::client::components::mobile::{
    mobile_tab_budget_px, mobile_tab_column_px, MobileTab, MobileTabBar,
};
use family_calendar::shared::homeschool::{
    self as sched, AssignmentRow, Category, Enrollment, LogRow, LogStatus, SubjectPlan, TermNote,
    TermNoteKind, WeekPlan, Weekday,
};
use family_calendar::shared::types::{
    BoyToday, ClientMessage, DayItem, ExtraTask, HomeschoolTodayView, MonthView, TogetherGroup,
    WeekGrid,
};

/// `2026-09-07` is a Monday; `2026-09-08` the Tuesday this fixture renders.
const FIXTURE_MONDAY: &str = "2026-09-07";
const FIXTURE_TUESDAY: &str = "2026-09-08";

fn fixture_days(letters: &str) -> Vec<Weekday> {
    sched::parse_days(letters).expect("fixture day letters parse")
}

fn fixture_row(assignment_id: i64, ordinal: i64, text: &str) -> AssignmentRow {
    AssignmentRow {
        assignment_id,
        ordinal,
        text: text.to_string(),
        detail: None,
        days: None,
    }
}

fn fixture_subject(
    subject_id: i64,
    name: &str,
    category: Category,
    letters: &str,
    rows: Vec<AssignmentRow>,
) -> SubjectPlan {
    SubjectPlan {
        subject_id,
        name: name.to_string(),
        category,
        source: None,
        icon_name: None,
        sort_order: subject_id,
        days: fixture_days(letters),
        shared: category.shared_by_default(),
        rows,
    }
}

/// One week of the synthetic curriculum (P-15): 3 weeks, `term_weeks = 1`,
/// seven subjects covering every branch of the H3 occurrence rule.
fn sample_week(week: i64) -> WeekPlan {
    let (fables, twice_told): (Vec<AssignmentRow>, Vec<AssignmentRow>) = match week {
        1 => (
            vec![
                fixture_row(41, 1, "The Kite and the Kettle"),
                fixture_row(42, 2, "The Patient Heron"),
            ],
            Vec::new(),
        ),
        2 => (
            Vec::new(),
            vec![
                fixture_row(51, 1, "The Tin Whistle"),
                fixture_row(52, 2, "The Second Telling"),
            ],
        ),
        _ => (
            vec![
                fixture_row(43, 1, "The Miller's Cat"),
                fixture_row(44, 2, "Two Crows and a Crumb"),
            ],
            Vec::new(),
        ),
    };
    WeekPlan {
        curriculum_id: 1,
        week,
        weeks: 3,
        term: sched::term_of(week, 1),
        subjects: vec![
            fixture_subject(1, "Sums", Category::Daily, "MTWRF", Vec::new()),
            fixture_subject(2, "Copywork", Category::Daily, "MTWRF", Vec::new()),
            fixture_subject(
                3,
                "Old Tales",
                Category::Reading,
                "MW",
                vec![fixture_row(30 + week, 1, "ch. 2 'The Long Road'")],
            ),
            fixture_subject(4, "Fables", Category::Reading, "TF", fables),
            fixture_subject(5, "Twice Told", Category::Reading, "T", twice_told),
            fixture_subject(
                6,
                "Painting",
                Category::Weekly,
                "F",
                vec![fixture_row(61, 1, "Study the picture")],
            ),
            fixture_subject(7, "Reading Basket", Category::FreeRead, "MTWRF", Vec::new()),
        ],
        term_notes: vec![TermNote {
            id: 1,
            term: sched::term_of(week, 1),
            kind: TermNoteKind::Poetry,
            text: "Rhymes for a Rainy Window".into(),
            sort_order: 0,
        }],
    }
}

fn sample_enrollment(profile_id: i64, week: i64, week_started_on: &str) -> Enrollment {
    Enrollment {
        profile_id,
        curriculum_id: 1,
        current_week: week,
        weeks: 3,
        term_weeks: 1,
        week_started_on: week_started_on.to_string(),
        school_days: fixture_days("MTWRF"),
        paused: false,
    }
}

fn named_boy(
    plan: &WeekPlan,
    enrollment: &Enrollment,
    logs: &[LogRow],
    name: &str,
    date: &str,
) -> BoyToday {
    let mut boy = sched::today_view(plan, enrollment, logs, date);
    boy.name = name.to_string();
    boy
}

fn fixture_group(
    plan: &WeekPlan,
    members: &[(Enrollment, Vec<LogRow>, &str)],
    date: &str,
    can_finish_week: bool,
) -> TogetherGroup {
    let with_logs: Vec<(Enrollment, Vec<LogRow>)> = members
        .iter()
        .map(|(enrollment, logs, _)| (enrollment.clone(), logs.clone()))
        .collect();
    let representative = with_logs[0].0.clone();
    TogetherGroup {
        curriculum_id: plan.curriculum_id,
        curriculum_name: "Sample Year".into(),
        week: plan.week,
        weeks: plan.weeks,
        term: plan.term,
        week_started_on: representative.week_started_on.clone(),
        paused: false,
        year_complete: false,
        can_finish_week,
        days_on_week: sched::days_on_week(&representative.week_started_on, date),
        together: sched::together_view(&with_logs, plan, date),
        boys: members
            .iter()
            .map(|(enrollment, logs, name)| named_boy(plan, enrollment, logs, name, date))
            .collect(),
        term_notes: plan.term_notes.clone(),
    }
}

/// P-15's pinned HS5 fixture: **two boys on week 2, one on week 1**, rendered
/// on Tuesday `2026-09-08`.
///
/// The boy on week 1 is anchored on the Tuesday rather than the Monday so that
/// his group's own `Old Tales` split falls outside the day entirely — the
/// fixture is built so exactly one `Old Tales` row, carrying `part 1 of 2`, is
/// on screen, which is what makes accept (b) a count rather than a hope.
fn fixture_today_view() -> HomeschoolTodayView {
    let week_two = sample_week(2);
    let week_one = sample_week(1);
    let group_a = fixture_group(
        &week_two,
        &[
            (
                sample_enrollment(1, 2, FIXTURE_MONDAY),
                Vec::new(),
                "Isaiah",
            ),
            (
                sample_enrollment(2, 2, FIXTURE_MONDAY),
                Vec::new(),
                "Nathaniel",
            ),
        ],
        FIXTURE_TUESDAY,
        true,
    );
    let group_b = fixture_group(
        &week_one,
        &[(
            sample_enrollment(3, 1, FIXTURE_TUESDAY),
            Vec::new(),
            "Simeon",
        )],
        FIXTURE_TUESDAY,
        false,
    );
    HomeschoolTodayView {
        date: FIXTURE_TUESDAY.to_string(),
        is_school_day: true,
        anyone_enrolled: true,
        groups: vec![group_a, group_b],
    }
}

/// The one piece of context a School pane reads: the session signal
/// `MobileShell` provides, so `session::is_parent()` answers something
/// deterministic instead of "there is no runtime".
#[component]
fn TodayHarness(view: HomeschoolTodayView, parent: bool) -> Element {
    use_context_provider(|| {
        Signal::new(Some(if parent {
            SessionState::Parent
        } else {
            SessionState::SignedOut
        }))
    });
    rsx! {
        TodayPanel {
            view,
            boy_filter: None,
            on_boy_filter: move |_: Option<i64>| {},
            on_action: move |_: SchoolAction| {},
        }
    }
}

fn render_today(view: HomeschoolTodayView, parent: bool) -> String {
    dioxus::ssr::render_element(rsx! {
        TodayHarness { view, parent }
    })
}

fn count(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

/// The slice of rendered markup belonging to one `data-*` marked element:
/// from its own marker up to the next element carrying the same marker (or the
/// end of the page). Every marked element in this module puts its marker
/// first, so its own classes and children fall inside its own slice.
fn slice_at<'a>(html: &'a str, marker: &str, value: &str) -> &'a str {
    let needle = format!("{marker}=\"{value}\"");
    let start = html
        .find(&needle)
        .unwrap_or_else(|| panic!("the page must render {needle}: {html}"));
    let rest = &html[start + needle.len()..];
    match rest.find(&format!("{marker}=\"")) {
        Some(end) => &rest[..end],
        None => rest,
    }
}

// ---------------------------------------------------------------------------
// (a) the six-tab bar and its pixel budget
// ---------------------------------------------------------------------------

/// `EventHandler` props must be built inside a running component scope (see
/// [`RoutineRowHarness`] above for the whole reason), so even a
/// context-free component gets a one-line wrapper.
#[component]
fn TabBarHarness(active: MobileTab) -> Element {
    rsx! {
        MobileTabBar { active, on_select: move |_: MobileTab| {} }
    }
}

#[test]
fn hs5_a_the_bar_renders_six_buttons_in_the_order_h6_names() {
    let html = dioxus::ssr::render_element(rsx! {
        TabBarHarness { active: MobileTab::Homeschool }
    });

    assert_eq!(
        count(&html, "<button"),
        6,
        "the bar must render exactly six tab buttons: {html}"
    );

    let positions: Vec<usize> = MobileTab::ALL
        .iter()
        .map(|tab| {
            html.find(&format!("data-mobile-tab=\"{}\"", tab.slug()))
                .unwrap_or_else(|| panic!("the bar must render the {} tab: {html}", tab.slug()))
        })
        .collect();
    let mut sorted = positions.clone();
    sorted.sort_unstable();
    assert_eq!(
        positions, sorted,
        "the tabs must render in `MobileTab::ALL` order: Routine · School · Calendar · Board · Remote · Settings"
    );

    assert!(
        html.contains("grid-cols-6"),
        "six columns, not five: {html}"
    );
    assert!(
        html.contains(">School<"),
        "the School tab is labelled: {html}"
    );
    assert!(
        html.contains(">Remote<"),
        "the remote tab is relabelled `Remote`: {html}"
    );
    assert!(
        !html.contains("TV Remote"),
        "the two-word label is the one that would not fit: {html}"
    );

    let glyph_at = html
        .find('\u{1F3E0}')
        .expect("the School tab must wear the house glyph");
    let span_start = html[..glyph_at]
        .rfind("<span")
        .expect("the glyph sits in a span");
    assert!(
        html[span_start..glyph_at].contains("aria-hidden=\"true\""),
        "the house glyph must be aria-hidden: {}",
        &html[span_start..glyph_at]
    );
    println!("tab bar: 6 buttons, School second, Remote relabelled");
}

#[test]
fn hs5_a_the_widest_tab_label_fits_a_column_of_the_narrowest_phone() {
    let budget = mobile_tab_budget_px();
    assert!(
        budget <= 60,
        "the widest tab label needs {budget}px; a sixth of a 360px phone is 60px"
    );
    assert_eq!(mobile_tab_column_px(), 60);
    println!(
        "mobile_tab_budget_px() = {budget}px of a {}px column",
        mobile_tab_column_px()
    );
}

// ---------------------------------------------------------------------------
// (b) Today, signed out and as a parent
// ---------------------------------------------------------------------------

#[test]
fn hs5_b_today_renders_the_fixture_the_way_h6_lays_it_out() {
    let html = render_today(fixture_today_view(), false);

    assert!(
        html.contains(">Together<"),
        "the shared read-alouds get their own heading: {html}"
    );
    assert_eq!(
        count(&html, "Old Tales"),
        1,
        "the split reading renders once, under Together, not once per boy: {html}"
    );
    assert_eq!(
        count(&html, "part 1 of 2"),
        1,
        "the Monday half of the split says which part it is: {html}"
    );
    assert!(
        html.contains("(then tell it back)"),
        "every reading row prompts the narration: {html}"
    );
    assert!(
        html.contains("Week 2 of 3"),
        "the header chip names the week: {html}"
    );

    for boy in ["Isaiah", "Nathaniel", "Simeon"] {
        assert!(html.contains(boy), "{boy} must have his own block: {html}");
    }
    assert_eq!(
        count(&html, "data-school-boy="),
        3,
        "one block per enrolled boy: {html}"
    );

    for parent_only in ["Finish week", "Mark all done", "Edit text", "Back a week"] {
        assert!(
            !html.contains(parent_only),
            "a signed-out phone must not offer `{parent_only}`: {html}"
        );
    }
}

#[test]
fn hs5_b_the_identical_render_as_a_parent_gains_exactly_the_parent_affordances() {
    let view = fixture_today_view();
    let complete_groups = view
        .groups
        .iter()
        .filter(|group| group.can_finish_week)
        .count();
    let boys: usize = view.groups.iter().map(|group| group.boys.len()).sum();
    assert_eq!(complete_groups, 1, "the fixture pins one finishable group");
    assert_eq!(boys, 3, "the fixture pins three enrolled boys");

    let html = render_today(view, true);
    assert_eq!(
        count(&html, "Finish week"),
        complete_groups,
        "exactly one `Finish week` per complete group: {html}"
    );
    assert_eq!(
        count(&html, "Mark all done"),
        boys,
        "exactly one `Mark all done` per boy: {html}"
    );
    assert!(
        html.contains("Edit text"),
        "a parent gets the inline edit affordance: {html}"
    );
}

#[test]
fn hs5_b_the_catch_up_chip_names_the_day_it_slipped_from() {
    let html = render_today(fixture_today_view(), false);
    assert!(
        html.contains("from Mon"),
        "Monday's unfinished work is tagged with its day: {html}"
    );
    assert!(
        html.contains(CATCH_UP_CHIP_CLASS),
        "and wears the declared catch-up chip: {html}"
    );
}

// ---------------------------------------------------------------------------
// (c) the three whole-surface states
// ---------------------------------------------------------------------------

#[test]
fn hs5_c_nobody_enrolled_offers_the_way_in_rather_than_a_blank_tab() {
    let empty = HomeschoolTodayView {
        date: FIXTURE_TUESDAY.into(),
        is_school_day: true,
        anyone_enrolled: false,
        groups: Vec::new(),
    };
    let html = render_today(empty, false);
    assert!(html.contains("No school plan yet"), "{html}");
    assert!(html.contains("Enroll a boy"), "{html}");
}

#[test]
fn hs5_c_a_paused_group_says_school_is_out_and_a_finished_year_celebrates() {
    let mut paused = fixture_today_view();
    paused.groups[0].paused = true;
    let html = render_today(paused, true);
    assert!(
        html.contains("No school today"),
        "a paused group says so in as many words: {html}"
    );
    assert_eq!(
        count(&html, "Mark all done"),
        1,
        "a paused group renders no boy blocks of its own, so only week 1's boy keeps his: {html}"
    );

    let mut complete = fixture_today_view();
    complete.groups[0].year_complete = true;
    let html = render_today(complete, true);
    assert!(html.contains("Year complete"), "{html}");
}

use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex};

use family_calendar::client::components::mobile::queue::QueuedMutationEntry;

// ---------------------------------------------------------------------------
// (d) the offline queue carries both School toggles
// ---------------------------------------------------------------------------

/// A hub that counts deliveries and applies each idempotency key once — the
/// same shape `tests/pwa_tests.rs` uses for the routine toggles.
#[derive(Clone, Default)]
struct FakeHub {
    calls: Arc<Mutex<Vec<QueuedMutationEntry>>>,
    applied: Arc<Mutex<HashSet<String>>>,
}

impl FakeHub {
    fn deliver(&self, entry: QueuedMutationEntry) -> Result<(), String> {
        self.calls.lock().expect("lock").push(entry.clone());
        self.applied.lock().expect("lock").insert(entry.key);
        Ok(())
    }

    fn calls(&self) -> Vec<QueuedMutationEntry> {
        self.calls.lock().expect("lock").clone()
    }

    fn effects(&self) -> usize {
        self.applied.lock().expect("lock").len()
    }
}

/// One offline School tick, replayed: it must reach the hub exactly once, keep
/// the key it was minted with, and produce one effect however many times it is
/// delivered.
async fn assert_queued_school_tick_replays_once(mutation: QueuedMutation, label: &str) {
    let now = 1_700_000_000_000i64;
    let mut queue = OfflineQueue::new();
    // The mutation is made on the 12th; the lesson it ticks was due on the 8th
    // — four days earlier, the catch-up case R-14 exists for.
    let entry = queue.enqueue(mutation.clone(), "2026-09-12", now);
    let minted_key = entry.key.clone();
    assert_eq!(queue.len(), 1);
    assert_eq!(mutation.label(), label);

    // A queue that survives being written to and read back from storage — the
    // phone is closed between the failure and the reconnect.
    let mut queue = OfflineQueue::from_json(&queue.to_json());

    let hub = FakeHub::default();
    let report = {
        let hub = hub.clone();
        queue
            .replay(now, move |entry| {
                let hub = hub.clone();
                async move { hub.deliver(entry) }
            })
            .await
    };
    assert_eq!(report.sent, 1, "{label} must replay on reconnect");
    assert!(queue.is_empty(), "a delivered entry leaves the queue");

    let calls = hub.calls();
    assert_eq!(calls.len(), 1, "{label} reached the hub once");
    assert_eq!(calls[0].date, "2026-09-12", "the day it was made");
    assert_eq!(
        calls[0].key, minted_key,
        "the key is minted once and never regenerated"
    );
    assert_eq!(calls[0].mutation, mutation);

    // Delivered twice (a retried request, two tabs, an iOS relaunch): one
    // effect, because the key is unchanged.
    hub.deliver(calls[0].clone()).expect("second delivery");
    assert_eq!(hub.calls().len(), 2, "the second delivery really happened");
    assert_eq!(hub.effects(), 1, "{label} is idempotent under its own key");

    // Nothing is left to replay a second time.
    let second = queue.replay(now, |_entry| async { Ok(()) }).await;
    assert_eq!(second.sent, 0, "{label} does not replay twice");
}

#[tokio::test]
async fn hs5_d_a_failed_catch_up_lesson_tick_is_queued_and_replayed_once() {
    assert_queued_school_tick_replays_once(
        QueuedMutation::ToggleLesson {
            user_id: 1,
            subject_id: 3,
            assignment_id: Some(32),
            week: 2,
            scheduled_date: FIXTURE_TUESDAY.to_string(),
            completed: true,
        },
        "School lesson",
    )
    .await;
}

#[tokio::test]
async fn hs5_d_a_failed_extra_tick_is_queued_and_replayed_once() {
    assert_queued_school_tick_replays_once(
        QueuedMutation::ToggleExtra {
            user_id: 1,
            extra_id: 77,
            completed: true,
        },
        "School task",
    )
    .await;
}

#[test]
fn hs5_d_both_school_mutations_name_their_boy_and_their_label() {
    let lesson = QueuedMutation::ToggleLesson {
        user_id: 2,
        subject_id: 3,
        assignment_id: None,
        week: 2,
        scheduled_date: FIXTURE_TUESDAY.to_string(),
        completed: false,
    };
    let extra = QueuedMutation::ToggleExtra {
        user_id: 3,
        extra_id: 9,
        completed: true,
    };
    assert_eq!(lesson.user_id(), 2);
    assert_eq!(extra.user_id(), 3);
    assert_eq!(lesson.label(), "School lesson");
    assert_eq!(extra.label(), "School task");

    // The two dates R-14 is about: the entry's own `date` and the
    // occurrence's `scheduled_date`, four days apart on a catch-up tick.
    let restored = OfflineQueue::from_json(
        &{
            let mut queue = OfflineQueue::new();
            queue.enqueue(lesson.clone(), "2026-09-12", 1_000);
            queue
        }
        .to_json(),
    );
    let entry = &restored.entries()[0];
    assert_eq!(entry.date, "2026-09-12");
    assert_eq!(entry.mutation, lesson, "the due date rides in the mutation");
}

// ---------------------------------------------------------------------------
// (e) the remote can put School on the television
// ---------------------------------------------------------------------------

#[test]
fn hs5_e_the_remote_offers_school_and_sends_the_view_the_kiosk_expects() {
    assert_eq!(
        VIEWS.len(),
        5,
        "Dashboard · Routine · Calendar · Whiteboard · School"
    );
    let (view, label) = VIEWS
        .iter()
        .copied()
        .find(|(view, _)| *view == MaximizedView::Homeschool)
        .expect("the remote must offer the School panel");
    assert_eq!(label, "School");

    // The message the button builds — the phone never mutates the TV locally.
    let message = ClientMessage::SetView { view, auth: None };
    assert!(
        matches!(
            message,
            ClientMessage::SetView {
                view: MaximizedView::Homeschool,
                auth: None
            }
        ),
        "sending the School entry must yield `SetView {{ view: Homeschool }}`"
    );
}

// ---------------------------------------------------------------------------
// (g) the module reuses declared pairs and never paints the hue as ink
// ---------------------------------------------------------------------------

#[test]
fn hs5_g_the_school_module_names_no_colour_the_palette_has_not_declared() {
    // R-12: `palette.rs` belongs to no HS task, so School may only reuse pairs
    // that are already in the table. The whole-tree scan in
    // `tests/palette_tests.rs` enforces the token allowlist; this asserts the
    // one pair HS5 is named on, and the one class it must never contain.
    assert!(CATCH_UP_CHIP_CLASS.contains("bg-sheffield-accent"));
    assert!(CATCH_UP_CHIP_CLASS.contains("text-slate-800"));

    let banned = concat!("text-sheffield-", "accent");
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("client")
        .join("components")
        .join("homeschool");
    let mut files = 0usize;
    for entry in std::fs::read_dir(&dir).expect("the School module directory exists") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        files += 1;
        let source = std::fs::read_to_string(&path).expect("reading a School source file");
        assert!(
            !source.contains(banned),
            "{path:?} names `{banned}`, which is 3.11:1 on paper and legal only \
             as the wordmark's display red"
        );
    }
    assert_eq!(files, 8, "the eight files HS5 owns were scanned");

    // And the chip really is painted, not merely declared.
    let html = render_today(fixture_today_view(), false);
    assert!(html.contains("bg-sheffield-accent"), "{html}");
    println!("{files} School sources scanned; the warm hue is a ground, never ink");
}

// ---------------------------------------------------------------------------
// (h) and (k) the Year view
// ---------------------------------------------------------------------------

#[component]
fn YearHarness(
    grid: WeekGrid,
    current_week: i64,
    anchor: String,
    parent: bool,
    boys: Vec<(i64, String)>,
) -> Element {
    use_context_provider(|| {
        Signal::new(Some(if parent {
            SessionState::Parent
        } else {
            SessionState::SignedOut
        }))
    });
    rsx! {
        YearPanel {
            grid,
            user_id: 1,
            current_week,
            anchor,
            boys,
            on_boy_filter: move |_: Option<i64>| {},
            on_select_week: move |_: i64| {},
            on_action: move |_: SchoolAction| {},
        }
    }
}

/// The two boys the QH1-05 chip assertions are built on: the pane is drawn for
/// `user_id: 1`, so Isaiah is the selected chip and Nathaniel the offer.
fn fixture_boys() -> Vec<(i64, String)> {
    vec![(1, "Isaiah".to_string()), (2, "Nathaniel".to_string())]
}

/// The markup of the boy-chip strip: from its container marker to the marker
/// of the pane content that always follows it. Scoping matters because
/// `LessonRow` carries an `aria-pressed` of its own, and "exactly one chip is
/// pressed" is an assertion about the chips.
fn chip_strip<'a>(html: &'a str, until: &str) -> &'a str {
    let start = html
        .find("data-boy-chips=\"true\"")
        .unwrap_or_else(|| panic!("the pane must render a chip strip: {html}"));
    let end = html[start..]
        .find(until)
        .unwrap_or_else(|| panic!("the chip strip must sit above {until}: {html}"))
        + start;
    &html[start..end]
}

/// The chip assertions QH1-05 pins, shared by Year and Month: two boys, two
/// chips, exactly one of them pressed, and no `Everyone` — Year is filtered by
/// the chip and Month *requires* it (H6, §4 default 17 / D-4).
fn assert_chip_strip(html: &str, until: &str) {
    let strip = chip_strip(html, until);
    assert_eq!(
        count(strip, "data-boy-chip="),
        2,
        "one chip per enrolled boy: {strip}"
    );
    assert_eq!(
        count(strip, "aria-pressed=\"true\""),
        1,
        "exactly one chip is the boy on screen: {strip}"
    );
    assert!(
        strip.contains("data-boy-chip=\"1\"") && strip.contains("data-boy-chip=\"2\""),
        "both enrolled boys are reachable without going back to Today: {strip}"
    );
    assert!(
        !html.contains("Everyone"),
        "Year and Month are about exactly one boy, so there is no Everyone: {html}"
    );
}

/// The fixture's week `week` as a grid, anchored the way HS4 anchors it:
/// the live `week_started_on` for the current week, a synthetic
/// `add_days(week_started_on, (week − current_week) × 7)` for any other.
fn fixture_grid(week: i64, current_week: i64) -> (WeekGrid, String) {
    let plan = sample_week(week);
    let enrollment = sample_enrollment(1, current_week, FIXTURE_MONDAY);
    let anchor = sched::add_days(FIXTURE_MONDAY, ((week - current_week) * 7) as i32)
        .expect("the synthetic anchor is a real date");
    let grid = sched::week_grid(&plan, &enrollment, &[], &anchor, week == current_week);
    (grid, anchor)
}

#[test]
fn hs5_h_the_year_view_lays_the_fixture_week_out_as_a_subject_by_day_grid() {
    let (grid, anchor) = fixture_grid(2, 2);
    assert!(grid.dated, "week 2 is the current week, so it is dated");
    let html = dioxus::ssr::render_element(rsx! {
        YearHarness {
            grid: grid.clone(),
            current_week: 2,
            anchor,
            parent: false,
            boys: Vec::new(),
        }
    });

    // The week picker: one entry per week of the curriculum, the current week
    // marked as such.
    assert_eq!(
        count(&html, "data-year-week="),
        grid.weeks as usize,
        "the picker offers every week 1…{}: {html}",
        grid.weeks
    );
    assert_eq!(grid.weeks, 3, "the fixture is a three-week curriculum");
    assert!(
        slice_at(&html, "data-year-week", "2").contains("aria-current=\"true\""),
        "week 2 is marked as the current week: {html}"
    );

    // The grid itself. `free_read` subjects have no row (H3 rule 6), so the
    // fixture's seven subjects give **six** subject rows — the plan's HS3 (g)
    // was corrected to say exactly that (review finding D-5). HS5 (h)'s "7
    // subject rows" counts the weekday header row, which the grid also has, so
    // both readings are asserted here rather than either being softened.
    assert_eq!(
        count(&html, "data-year-subject="),
        6,
        "six subject rows: the free-reading list is never an occurrence: {html}"
    );
    assert_eq!(grid.rows.len(), 6);
    assert_eq!(
        count(&html, "data-year-row="),
        7,
        "six subject rows plus the weekday header row: {html}"
    );
    assert_eq!(
        count(&html, "data-year-col="),
        5,
        "five school-day columns, Monday to Friday: {html}"
    );
    assert_eq!(
        count(&html, "data-year-cell="),
        6 * 5,
        "every subject row has a cell per day: {html}"
    );

    // `Twice Told` is the both-readings-on-one-day case: two rows, one day.
    let twice_told = grid
        .rows
        .iter()
        .find(|row| row.title == "Twice Told")
        .expect("the fixture has a Twice Told subject");
    let tuesday = grid
        .days
        .iter()
        .position(|day| *day == Weekday::Tue)
        .expect("Tuesday is a school day");
    assert_eq!(twice_told.cells[tuesday].len(), 2);
    let cell = slice_at(
        &html,
        "data-year-cell",
        &format!("{}-{tuesday}", twice_told.subject_id),
    );
    assert_eq!(
        count(cell, "data-year-entry="),
        2,
        "the Tuesday cell holds both of the week's readings: {cell}"
    );

    // W-9's answer: a thumb-sized row, and the grid scrolls in its own box
    // rather than the page scrolling sideways.
    assert_eq!(
        count(&html, "min-h-[44px]"),
        7,
        "every grid row, header included, is at least 44px tall: {html}"
    );
    assert!(
        html.contains("overflow-x-auto"),
        "the grid scrolls inside its own container: {html}"
    );
    println!("year grid: 6 subject rows + 1 header × 5 day columns, Twice Told doubled on Tuesday");
}

#[test]
fn hs5_h_two_enrolled_boys_each_get_a_chip_on_the_year_pane() {
    // QH1-05: H6 says "a boy chip filters Year exactly as it filters Today",
    // but the chips lived inside `TodayPanel` alone, so the second boy's Year
    // was reachable only by going back to Today, tapping his chip and toggling
    // Year again. Dormant while only Isaiah is enrolled; live the day
    // Nathaniel is.
    let (grid, anchor) = fixture_grid(2, 2);
    let html = dioxus::ssr::render_element(rsx! {
        YearHarness {
            grid,
            current_week: 2,
            anchor,
            parent: false,
            boys: fixture_boys(),
        }
    });
    assert_chip_strip(&html, "data-year-picker");
    assert!(
        slice_at(&html, "data-boy-chip", "1").contains("aria-pressed=\"true\""),
        "the pane is drawn for boy 1, so his is the pressed chip: {html}"
    );
    println!("year: two chips, Isaiah pressed, no Everyone");
}

#[test]
fn hs5_h_a_single_enrolled_boy_is_offered_no_chip_to_press() {
    let (grid, anchor) = fixture_grid(2, 2);
    let html = dioxus::ssr::render_element(rsx! {
        YearHarness {
            grid,
            current_week: 2,
            anchor,
            parent: false,
            boys: vec![(1, "Isaiah".to_string())],
        }
    });
    assert!(
        !html.contains("data-boy-chips="),
        "one boy is not a choice: {html}"
    );
}

#[test]
fn hs5_k_a_week_that_has_not_been_dealt_out_is_neither_dated_nor_tickable() {
    let (grid, anchor) = fixture_grid(3, 2);
    assert!(!grid.dated, "week 3 is not the current week (D-5)");
    let html = dioxus::ssr::render_element(rsx! {
        YearHarness {
            grid,
            current_week: 2,
            anchor,
            parent: true,
            boys: Vec::new(),
        }
    });

    assert!(
        !html.contains("data-lesson-check="),
        "an undealt week renders no checkbox — the server would reject the tick: {html}"
    );
    assert!(
        !html.contains("2026-"),
        "and no date in the column headers, because there is not a true one: {html}"
    );
    // The weekday names are still there — the grid is readable, just undated.
    assert!(html.contains(">Mon<") && html.contains(">Fri<"), "{html}");

    // The current week, by contrast, is both dated and tickable.
    let (dated_grid, dated_anchor) = fixture_grid(2, 2);
    let dated_html = dioxus::ssr::render_element(rsx! {
        YearHarness {
            grid: dated_grid,
            current_week: 2,
            anchor: dated_anchor,
            parent: true,
            boys: Vec::new(),
        }
    });
    assert!(dated_html.contains("data-lesson-check="), "{dated_html}");
    assert!(dated_html.contains(FIXTURE_MONDAY), "{dated_html}");
}

// ---------------------------------------------------------------------------
// (i) the Month view
// ---------------------------------------------------------------------------

/// The text of a month cell's count badge — `2/5` inside the current week's
/// span, a bare `1` outside it, `None` when the cell shows no count at all.
fn count_badge(cell: &str) -> Option<String> {
    const BADGE: &str = "class=\"text-xs font-semibold text-slate-600\">";
    let start = cell.find(BADGE)? + BADGE.len();
    let end = cell[start..].find('<')? + start;
    Some(cell[start..end].to_string())
}

fn fixture_extra(scheduled_date: &str) -> ExtraTask {
    ExtraTask {
        id: 91,
        user_id: 1,
        scheduled_date: scheduled_date.to_string(),
        title: "Finish the model bridge".into(),
        category: Category::Daily,
        text: None,
        sort_order: 1,
        status: None,
        note: None,
    }
}

/// September 2026 for the boy on week 2, anchored on Monday `2026-09-07`, with
/// one log row on the 1st (a bare count) and one extra on the 10th (a pin).
fn fixture_month() -> MonthView {
    let plan = sample_week(2);
    let enrollment = sample_enrollment(1, 2, FIXTURE_MONDAY);
    let logs = vec![LogRow {
        subject_id: 1,
        assignment_id: None,
        scheduled_date: "2026-09-01".into(),
        status: LogStatus::Done,
        note: None,
    }];
    let extras = vec![fixture_extra("2026-09-10")];
    sched::month_view(
        Some(&enrollment),
        Some(&plan),
        &logs,
        &extras,
        2026,
        9,
        FIXTURE_TUESDAY,
    )
}

#[component]
fn MonthHarness(month: MonthView, boys: Vec<(i64, String)>) -> Element {
    rsx! {
        MonthPanel {
            month,
            label: month_label(2026, 9),
            boys,
            on_boy_filter: move |_: Option<i64>| {},
            on_open_day: move |_: String| {},
            on_step: move |_: i32| {},
        }
    }
}

#[test]
fn hs5_i_the_month_view_counts_only_what_it_can_honestly_count() {
    let month = fixture_month();
    let html = dioxus::ssr::render_element(rsx! {
        MonthHarness { month: month.clone(), boys: Vec::new() }
    });

    assert_eq!(
        count(&html, "data-month-day="),
        30,
        "September has thirty days and every one of them gets a cell: {html}"
    );
    assert!(
        html.contains("data-month-weekend="),
        "the weekend is a thin strip, not five more columns: {html}"
    );
    assert!(html.contains("September 2026"), "{html}");

    // The 10th: a parent-added task, so a pin — whatever the pointer is doing.
    let tenth = month
        .days
        .iter()
        .find(|day| day.date == "2026-09-10")
        .expect("the 10th is in September");
    assert_eq!(tenth.extras, 1);
    assert!(
        slice_at(&html, "data-month-day", "2026-09-10").contains('\u{1F4CC}'),
        "the 10th carries the pin: {html}"
    );

    // The 8th: inside the current week's span, so the plan is dealt out and
    // the cell can show a denominator.
    let eighth = month
        .days
        .iter()
        .find(|day| day.date == FIXTURE_TUESDAY)
        .expect("the 8th is in September");
    assert!(eighth.in_current_week);
    let total = eighth.total.expect("a day in the span has a denominator");
    assert!(
        slice_at(&html, "data-month-day", FIXTURE_TUESDAY)
            .contains(&format!("{}/{total}", eighth.done)),
        "the 8th shows done/total: {html}"
    );

    // The 1st: before the span, so a bare count — a past week's plan is not
    // reconstructed.
    let first = month
        .days
        .iter()
        .find(|day| day.date == "2026-09-01")
        .expect("the 1st is in September");
    assert!(!first.in_current_week);
    assert_eq!(first.total, None);
    assert_eq!(first.done, 1);
    let first_cell = slice_at(&html, "data-month-day", "2026-09-01");
    assert_eq!(
        count_badge(first_cell),
        Some("1".to_string()),
        "a past day shows a bare count, never a denominator it cannot know: {first_cell}"
    );
    assert_eq!(
        count_badge(slice_at(&html, "data-month-day", FIXTURE_TUESDAY)),
        Some(format!("{}/{total}", eighth.done)),
        "…where a day in the span shows both halves: {html}"
    );
    println!(
        "month grid: 30 cells, pin on the 10th, {}/{total} on the 8th",
        eighth.done
    );
}

#[test]
fn hs5_i_the_month_chip_is_a_required_selector_never_an_everyone() {
    // QH1-05 / D-4: "Month view always shows exactly one boy … the chip is a
    // required selector". Two boys enrolled, two chips, one of them pressed,
    // and no way to ask for both at once.
    let month = fixture_month();
    let html = dioxus::ssr::render_element(rsx! {
        MonthHarness { month: month.clone(), boys: fixture_boys() }
    });
    assert_eq!(month.user_id, 1, "the fixture month is boy 1's");
    assert_chip_strip(&html, "aria-label=\"Previous month\"");
    assert!(
        slice_at(&html, "data-boy-chip", "1").contains("aria-pressed=\"true\""),
        "the month is boy 1's, so his is the pressed chip: {html}"
    );

    let alone = dioxus::ssr::render_element(rsx! {
        MonthHarness { month, boys: vec![(1, "Isaiah".to_string())] }
    });
    assert!(
        !alone.contains("data-boy-chips="),
        "with one boy enrolled there is nothing to select: {alone}"
    );
    println!("month: two chips, one pressed, no Everyone");
}

// ---------------------------------------------------------------------------
// (j) the day sheet
// ---------------------------------------------------------------------------

#[component]
fn DaySheetHarness(
    date: String,
    week: i64,
    in_current_week: bool,
    before_span: bool,
    items: Vec<DayItem>,
    parent: bool,
) -> Element {
    use_context_provider(|| {
        Signal::new(Some(if parent {
            SessionState::Parent
        } else {
            SessionState::SignedOut
        }))
    });
    rsx! {
        DaySheet {
            date,
            week,
            in_current_week,
            before_span,
            user_id: 1,
            items,
            on_action: move |_: SchoolAction| {},
            on_close: move |()| {},
        }
    }
}

/// The 8th's items for the boy on week 2: his own work plus one extra.
fn fixture_day_items() -> Vec<DayItem> {
    let plan = sample_week(2);
    let enrollment = sample_enrollment(1, 2, FIXTURE_MONDAY);
    let mut boy = sched::today_view(&plan, &enrollment, &[], FIXTURE_TUESDAY);
    let extras = vec![fixture_extra(FIXTURE_TUESDAY)];
    let span = sched::week_span(FIXTURE_MONDAY).expect("the span is a real week");
    sched::merge_extras(&mut boy, &extras, FIXTURE_TUESDAY, (&span.0, &span.1));
    boy.due_today
}

#[test]
fn hs5_j_only_a_parent_is_offered_the_add_task_form() {
    let items = fixture_day_items();
    let parent_html = dioxus::ssr::render_element(rsx! {
        DaySheetHarness {
            date: FIXTURE_TUESDAY.to_string(),
            week: 2,
            in_current_week: true,
            before_span: false,
            items: items.clone(),
            parent: true,
        }
    });
    assert!(parent_html.contains("Add task"), "{parent_html}");
    for (category, _) in EXTRA_CATEGORIES {
        assert!(
            parent_html.contains(&format!("data-extra-category=\"{}\"", category.as_str())),
            "the form offers the {} option: {parent_html}",
            category.as_str()
        );
    }
    assert_eq!(
        count(&parent_html, "data-extra-category="),
        3,
        "three kinds of task, and free reading is not one of them: {parent_html}"
    );

    let signed_out_html = dioxus::ssr::render_element(rsx! {
        DaySheetHarness {
            date: FIXTURE_TUESDAY.to_string(),
            week: 2,
            in_current_week: true,
            before_span: false,
            items,
            parent: false,
        }
    });
    assert!(
        !signed_out_html.contains("Add task"),
        "a signed-out phone is not offered a form the hub would refuse: {signed_out_html}"
    );
}

#[test]
fn hs5_j_a_future_day_says_it_has_not_been_dealt_out_and_shows_extras_only() {
    // A date past the current week's span: the curriculum is not dealt out
    // there until the parent finishes week 2, but a parent-added task pinned
    // to that date is real and renders anyway (H8).
    let items = vec![DayItem::Extra(fixture_extra("2026-09-24"))];
    let html = dioxus::ssr::render_element(rsx! {
        DaySheetHarness {
            date: "2026-09-24".to_string(),
            week: 2,
            in_current_week: false,
            before_span: false,
            items,
            parent: true,
        }
    });

    assert_eq!(
        count(&html, "Not dealt out yet \u{2014} finish week 2 first."),
        1,
        "exactly the line the plan writes, once: {html}"
    );
    assert!(
        !html.contains("data-lesson-row="),
        "no curriculum rows on a day that has not been dealt out: {html}"
    );
    assert!(
        html.contains("data-extra-row=\"91\""),
        "the parent's own task is still there: {html}"
    );
}

#[test]
fn hs5_j_a_past_day_says_it_is_behind_the_week_not_that_it_is_waiting_on_one() {
    // QH1-07: every date outside the span, past ones included, used to say
    // "Not dealt out yet — finish week 2 first.", so tapping last Tuesday told
    // the parent to finish a week that had already gone by. A past date is
    // behind the span, and what the sheet can honestly show there is the
    // tasks the parent added themselves (a past week's plan is deliberately
    // not reconstructed, H6).
    let items = vec![DayItem::Extra(fixture_extra("2026-09-01"))];
    let html = dioxus::ssr::render_element(rsx! {
        DaySheetHarness {
            date: "2026-09-01".to_string(),
            week: 2,
            in_current_week: false,
            before_span: true,
            items,
            parent: true,
        }
    });

    assert_eq!(
        count(
            &html,
            "Before this week \u{2014} only tasks you added are shown."
        ),
        1,
        "the past-date wording, once: {html}"
    );
    assert!(
        !html.contains("Not dealt out yet"),
        "a day that has already passed is not waiting on a week: {html}"
    );
    assert!(
        !html.contains("data-lesson-row="),
        "no curriculum rows on a day outside the span: {html}"
    );
    assert!(
        html.contains("data-extra-row=\"91\""),
        "the parent's own task is still there: {html}"
    );
    println!("day sheet: 2026-09-01 reads `Before this week`, no curriculum rows");
}

// ---------------------------------------------------------------------------
// QH1-02 — the resources really do read the signals they key on
// ---------------------------------------------------------------------------

/// The body of one `use_resource(` block in `School()`: from the `let mut
/// <name> = use_resource(` line to the `});` that closes it.
///
/// A source-shape guard in the style of `tv_tests::tv_sources`, and for the
/// same reason: the defect it pins is invisible to an SSR test. Every pane is
/// rendered with plain props, so a render assertion cannot see whether the
/// *tab* refetches when a signal changes — only that the props it was handed
/// came out right. Dioxus 0.7's `use_resource` subscribes to exactly the
/// signals read inside its own closure, so "the closure reads it" is the
/// property, and reading it here is the check.
fn resource_body<'a>(source: &'a str, name: &str) -> &'a str {
    let opener = format!("let mut {name} = use_resource(");
    let start = source
        .find(&opener)
        .unwrap_or_else(|| panic!("`School()` must still declare {name}"));
    let rest = &source[start..];
    let end = rest
        .find("\n    });")
        .unwrap_or_else(|| panic!("{name}'s resource must close with a `}});` line"));
    &rest[..end]
}

#[test]
fn hs5_qa1_the_year_and_month_resources_read_the_signals_they_are_keyed_on() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("client")
        .join("components")
        .join("homeschool")
        .join("mod.rs");
    let source = std::fs::read_to_string(&path).expect("the School tab's source is readable");

    let grid = resource_body(&source, "grid_res");
    for read in ["grid_week()", "focus()"] {
        assert!(
            grid.contains(read),
            "`grid_res` must read {read} inside its own closure, or tapping a \
             week in the Year picker re-renders without refetching: {grid}"
        );
    }

    let month = resource_body(&source, "month_res");
    for read in ["cursor()", "focus()"] {
        assert!(
            month.contains(read),
            "`month_res` must read {read} inside its own closure, or stepping \
             the month re-renders without refetching: {month}"
        );
    }

    // And the values themselves are memos, not plain locals computed above the
    // closures — a plain local is captured by copy and never re-read.
    for memo in [
        "let focus = use_memo(",
        "let grid_week = use_memo(",
        "let cursor = use_memo(",
    ] {
        assert!(
            source.contains(memo),
            "{memo}…) is what makes the read above reactive: {path:?}"
        );
    }
    println!("grid_res reads grid_week()/focus(); month_res reads cursor()/focus()");
}
