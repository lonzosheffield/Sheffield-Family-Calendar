//! What the kiosk is showing, and in what order the remote reaches it.
//!
//! Everything here is plain data and pure functions. That is deliberate: the
//! T2.1 acceptance contract (PURPLE §P3) asks for a **deterministic** focus
//! order pinned to a golden file and for **pure-function** key-handler
//! transition tests, and neither is provable if the focus order is an
//! emergent property of the DOM.
//!
//! The single source of truth is [`focus_order`]: `TvSurface` renders the
//! list it returns, in the order it returns it, so "the order the test
//! asserts" and "the order the remote walks" cannot drift apart.
//!
//! # The two zones
//!
//! ```text
//!  ┌──────────────┬─────────────────────────────────────────┐
//!  │ ProfileRail  │  PanelBody                              │
//!  │              │                                         │
//!  │  Reuben      │   ☐ Wake up and thank God for the day!  │
//!  │  Silas    ◄──┼── Up/Down here switches profile         │
//!  │  Judah       │   ☐ Make your bed                       │
//!  │  Asher       │   ☑ Brush your teeth                    │
//!  │  Phone QR    │   ...   Up/Down here walks the list     │
//!  └──────────────┴─────────────────────────────────────────┘
//!         Left/Right cycles the panel, from either zone.
//! ```
//!
//! Up/Down is *one* key doing *one* thing — "move the cursor down the column
//! you are in" — which is what makes D8's "Up/Down switches profile" and a
//! reachable eight-item routine list coexist (PURPLE §P3 T2.1 resolves D8's
//! Left/Right-vs-profile-selector clash this way). `Enter` crosses from the
//! rail into the list; `Backspace` crosses back.

use crate::client::components::calendar::CalendarState;
use crate::shared::types::{
    CalendarEvent, CustomTaskView, DayItem, HomeschoolTodayView, LessonOccurrence, MaximizedView,
    RoutineItemView, ServerMessage,
};

use super::keymap::KeyLogEntry;

// ---------------------------------------------------------------------------
// Panels
// ---------------------------------------------------------------------------

/// The four full-screen panels Left/Right cycles between.
///
/// **HS6** appended `Homeschool` (`docs/homeschool/PLAN_HOMESCHOOL.md` H6:
/// "TV `/tv`, panel 4 of 4 — `Routine · Calendar · Whiteboard · School`,
/// Left/Right wraps"), so School is exactly one `Left` from the routine the
/// kiosk boots on.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub enum TvPanel {
    #[default]
    Routine,
    Calendar,
    Whiteboard,
    Homeschool,
}

impl TvPanel {
    /// Cycle order. `Routine` is first because it is what the kiosk exists
    /// for and where `Backspace` always lands.
    pub const ALL: [TvPanel; 4] = [
        TvPanel::Routine,
        TvPanel::Calendar,
        TvPanel::Whiteboard,
        TvPanel::Homeschool,
    ];

    pub fn index(self) -> usize {
        match self {
            TvPanel::Routine => 0,
            TvPanel::Calendar => 1,
            TvPanel::Whiteboard => 2,
            TvPanel::Homeschool => 3,
        }
    }

    /// Right: next panel, wrapping.
    pub fn next(self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    /// Left: previous panel, wrapping.
    pub fn previous(self) -> Self {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }

    /// Stable machine name, stamped on the surface as `data-tv-panel` so a
    /// test can assert which panel rendered without matching prose.
    pub fn slug(self) -> &'static str {
        match self {
            TvPanel::Routine => "routine",
            TvPanel::Calendar => "calendar",
            TvPanel::Whiteboard => "whiteboard",
            TvPanel::Homeschool => "homeschool",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            TvPanel::Routine => "Morning Routine",
            TvPanel::Calendar => "Today",
            TvPanel::Whiteboard => "Whiteboard",
            TvPanel::Homeschool => "School",
        }
    }

    /// The protocol's `View` for this panel ([`ServerMessage::SetView`]).
    pub fn to_view(self) -> MaximizedView {
        match self {
            TvPanel::Routine => MaximizedView::Routine,
            TvPanel::Calendar => MaximizedView::Calendar,
            TvPanel::Whiteboard => MaximizedView::Whiteboard,
            TvPanel::Homeschool => MaximizedView::Homeschool,
        }
    }

    /// A phone's `SetView`, resolved onto a panel.
    ///
    /// `MaximizedView::None` means "nothing is maximised" on the multi-panel
    /// dashboard, but the kiosk is *always* showing exactly one panel at 10
    /// feet, so `None` resolves to the default panel — which is also what a
    /// parent pressing "Restore" on the phone means: put the television back
    /// on the routine.
    pub fn from_view(view: MaximizedView) -> Self {
        match view {
            // T2.7: `Screensaver` is not a panel — the overlay it names is
            // drawn independently by `client::components::screensaver`, on
            // top of whatever panel is showing — so it resolves to the same
            // default as `None` here (`docs/HANDOFF.md` "T2.7 → T2.1").
            MaximizedView::Routine | MaximizedView::None | MaximizedView::Screensaver => {
                TvPanel::Routine
            }
            MaximizedView::Calendar => TvPanel::Calendar,
            MaximizedView::Whiteboard => TvPanel::Whiteboard,
            // HS6: the real arm, replacing the Boss's post-A placeholder
            // (`docs/homeschool/PLAN_HOMESCHOOL.md` §3 "Boss micro-commits").
            MaximizedView::Homeschool => TvPanel::Homeschool,
        }
    }
}

// ---------------------------------------------------------------------------
// Focus identities
// ---------------------------------------------------------------------------

/// The zone the remote's cursor is in.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub enum TvZone {
    #[default]
    ProfileRail,
    PanelBody,
}

/// A modal that captures the whole screen and the whole focus order.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub enum TvOverlay {
    #[default]
    None,
    /// The phone-join QR (D3′ / D8).
    JoinQr,
}

/// One focusable element, identified by *what it is* rather than by where it
/// happens to sit in the DOM.
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
pub enum FocusId {
    /// A family profile on the rail.
    Profile(i64),
    /// The rail's last entry: open the phone-join QR.
    JoinQr,
    /// One of the eight morning-routine items.
    RoutineItem(u32),
    /// One of the profile's extra (photo) tasks.
    CustomTask(u32),
    /// A calendar entry. Read-only on the television: calendar editing is
    /// phone-only (PURPLE §P5.5 default 35), but it is still focusable so a
    /// child can walk a long day without a pointer.
    Event(String),
    /// **HS6** — one of the active boy's own lesson occurrences on the School
    /// panel, keyed by [`lesson_key`] (`s{subject}-a{assignment|0}-{date}`)
    /// rather than by its index in the list.
    ///
    /// The key has to be the occurrence's *identity* because the list is
    /// refetched on every `homeschool_version` bump and a tick reorders
    /// nothing but changes every row's state (D-2): an index would move the
    /// ring onto whatever slid into that slot.
    Lesson(String),
    /// **HS6** — one of the boy's parent-added tasks (`lesson_extras`), keyed
    /// by its row id for the same reason.
    Extra(i64),
    /// The overlay's dismiss target.
    OverlayClose,
}

impl FocusId {
    /// The element's DOM `id`. This is the string the golden file pins.
    pub fn dom_id(&self) -> String {
        match self {
            FocusId::Profile(id) => format!("tv-profile-{id}"),
            FocusId::JoinQr => "tv-join-qr".to_string(),
            FocusId::RoutineItem(id) => format!("tv-routine-{id}"),
            FocusId::CustomTask(id) => format!("tv-task-{id}"),
            FocusId::Event(id) => format!("tv-event-{}", slugify(id)),
            FocusId::Lesson(key) => format!("tv-lesson-{key}"),
            FocusId::Extra(id) => format!("tv-extra-{id}"),
            FocusId::OverlayClose => "tv-overlay-close".to_string(),
        }
    }
}

/// The stable key of one lesson occurrence: `s{subject}-a{assignment|0}-{date}`
/// (HS6).
///
/// It is exactly the tuple the `lesson_log_occurrence` unique index keys on —
/// `(subject_id, IFNULL(assignment_id, 0), scheduled_date)` for one boy in one
/// week (H1/H3 rule 9) — so two rows the server considers the same occurrence
/// can never be two focus targets, and a row the server considers distinct can
/// never collide with another.
pub fn lesson_key(occurrence: &LessonOccurrence) -> String {
    format!(
        "s{}-a{}-{}",
        occurrence.subject_id,
        occurrence.assignment_id.unwrap_or(0),
        occurrence.scheduled_date
    )
}

/// The focus identity of one row of the School panel.
pub fn day_item_focus(item: &DayItem) -> FocusId {
    match item {
        DayItem::Lesson(lesson) => FocusId::Lesson(lesson_key(lesson)),
        DayItem::Extra(extra) => FocusId::Extra(extra.id),
    }
}

/// Reduce an arbitrary identifier (a Google event id, say) to something safe
/// to put in a DOM `id` and in a golden file.
fn slugify(raw: &str) -> String {
    let mut out: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    if out.is_empty() {
        out.push_str("unknown");
    }
    out
}

// ---------------------------------------------------------------------------
// The model
// ---------------------------------------------------------------------------

/// One family profile as the rail shows it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TvProfile {
    pub id: i64,
    pub name: String,
    /// Hex colour from the `profiles` table (T1.4), used as an inline style —
    /// it is per-row data, so it cannot be a Tailwind class.
    pub color: String,
}

/// Everything `TvSurface` needs to draw a frame. A plain struct with no
/// signals, no context and no server calls, so a test can build one, render
/// it, and assert on the result.
#[derive(Clone, PartialEq, Debug)]
pub struct TvModel {
    pub profiles: Vec<TvProfile>,
    pub routine: Vec<RoutineItemView>,
    pub tasks: Vec<CustomTaskView>,
    /// Today's calendar as a **state**, not a bare list (W3): a failed
    /// `get_today_events` must not reach the television looking like a day
    /// with nothing on it. The four arms are rendered separately by
    /// [`super::surface`].
    pub events: CalendarState<Vec<CalendarEvent>>,
    /// **HS6** — the whole Today view the hub answered with, or `None` before
    /// `get_homeschool_today` has answered at all.
    ///
    /// The *whole* view rather than one boy's slice: the rail can move the
    /// active profile between two frames without a refetch, and the panel has
    /// to be able to say "No school plan for Simeon" the instant the cursor
    /// lands on him. [`school`] does that slicing, purely.
    pub homeschool: Option<HomeschoolTodayView>,
    pub state: TvState,
    /// Is the WebSocket up? Drives half of the disconnected badge (D8).
    pub connected: bool,
    /// Has the hub been silent past the 90 s threshold? The other half.
    pub stale: bool,
    /// Server-local `HH:MM` of the last thing the hub said, or `None` before
    /// the first answer.
    pub updated_at: Option<String>,
    /// `https://<ip>:8443/m` — what the QR encodes and what the overlay
    /// prints underneath it.
    pub join_url: Option<String>,
    /// `?keys=1` (see [`super::keymap::keys_debug_enabled`]).
    pub keys_debug: bool,
    pub key_log: Vec<KeyLogEntry>,
}

impl TvModel {
    /// An empty kiosk: what the surface renders before any fetch answers.
    pub fn empty() -> Self {
        Self {
            profiles: Vec::new(),
            routine: Vec::new(),
            tasks: Vec::new(),
            events: CalendarState::Loading,
            homeschool: None,
            state: TvState::default(),
            connected: false,
            stale: false,
            updated_at: None,
            join_url: None,
            keys_debug: false,
            key_log: Vec::new(),
        }
    }

    /// The profile the rail currently has under the cursor, if the cursor is
    /// on a profile at all (it may be on the QR entry).
    pub fn focused_profile(&self) -> Option<&TvProfile> {
        self.profiles.get(self.state.rail_index)
    }

    /// The profile whose routine the body is showing. The rail cursor *is*
    /// the selection (D8: Up/Down switches profile), so this is the last
    /// profile the cursor sat on — kept as [`TvState::active_profile`] so
    /// moving the cursor onto the QR entry does not blank the panel.
    pub fn active_profile(&self) -> Option<&TvProfile> {
        self.state
            .active_profile
            .and_then(|id| self.profiles.iter().find(|p| p.id == id))
            .or_else(|| self.profiles.first())
    }

    /// Apply an inbound [`ServerMessage`] that steers the kiosk.
    ///
    /// Only `SetView` and `SetActiveProfile` move the television (D1: phones
    /// remote-control the TV); everything else on the bus is data the
    /// resources refetch and is ignored here. Returns `true` if the model
    /// changed, which is what the acceptance test for (b) and (c) asserts
    /// before re-rendering.
    pub fn apply_server_message(&mut self, message: &ServerMessage) -> bool {
        let before = self.state;
        match message {
            ServerMessage::SetView { view } => {
                self.state
                    .set_panel(TvPanel::from_view(*view), &TvLayout::of(self));
            }
            ServerMessage::SetActiveProfile { user_id } => {
                if let Some(index) = self.profiles.iter().position(|p| p.id == *user_id) {
                    self.state.rail_index = index;
                    self.state.active_profile = Some(*user_id);
                    self.state.zone = TvZone::ProfileRail;
                    self.state.body_index = 0;
                }
            }
            _ => {}
        }
        before != self.state
    }
}

/// Where the remote's cursor is. Small, `Copy`, and comparable, so a
/// transition test is a one-line `assert_eq!`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct TvState {
    pub panel: TvPanel,
    pub zone: TvZone,
    /// Index into the rail: `0..profiles.len()` are profiles, the entry after
    /// the last profile is [`FocusId::JoinQr`].
    pub rail_index: usize,
    /// Index into the current panel's body list.
    pub body_index: usize,
    pub overlay: TvOverlay,
    /// Profile id the body is showing. Separate from `rail_index` so the
    /// panel does not blank when the cursor moves onto the QR entry.
    pub active_profile: Option<i64>,
}

impl Default for TvState {
    fn default() -> Self {
        Self {
            panel: TvPanel::Routine,
            zone: TvZone::ProfileRail,
            rail_index: 0,
            body_index: 0,
            overlay: TvOverlay::None,
            active_profile: None,
        }
    }
}

impl TvState {
    /// The state a freshly booted kiosk is in: routine panel, cursor on the
    /// first profile. This is where the "≤ 12 key presses" search starts.
    pub fn initial() -> Self {
        Self::default()
    }

    /// Move to `panel`, clamping the body cursor into the new panel's list.
    pub fn set_panel(&mut self, panel: TvPanel, layout: &TvLayout) {
        self.panel = panel;
        let len = layout.body_len(panel);
        if len == 0 {
            self.zone = TvZone::ProfileRail;
            self.body_index = 0;
        } else if self.body_index >= len {
            self.body_index = len - 1;
        }
    }
}

/// The shape of the focusable lists, extracted from a [`TvModel`] so the key
/// handler is a pure function of "how many things are where" rather than of
/// the whole model.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TvLayout {
    /// Profiles **plus** the trailing QR entry.
    pub rail_len: usize,
    /// Body length per panel, indexed by [`TvPanel::index`].
    pub body_lens: [usize; 4],
}

impl TvLayout {
    pub fn of(model: &TvModel) -> Self {
        Self {
            rail_len: model.profiles.len() + 1,
            body_lens: [
                model.routine.len() + model.tasks.len(),
                // Only a `Ready` calendar has rows to focus: Loading, Error
                // and Empty each render one unfocusable sentence (W3).
                model.events.ready().map_or(0, Vec::len),
                // Drawing is phone-only (PURPLE §P5.5 default 35): the
                // television shows the board, it does not edit it, so the
                // panel has nothing focusable.
                0,
                // HS6: the active boy's own rows. Each of the four state
                // cards ("No school plan for Simeon", "No school today",
                // "Year complete", the all-done celebration) is one
                // unfocusable card, exactly like the calendar's three
                // non-`Ready` arms.
                school_rows(model).len(),
            ],
        }
    }

    pub fn body_len(&self, panel: TvPanel) -> usize {
        self.body_lens[panel.index()]
    }
}

// ---------------------------------------------------------------------------
// HS6 — the School panel, sliced for the active boy
// ---------------------------------------------------------------------------

/// What the School panel has to draw for the boy the rail has selected
/// (`docs/homeschool/PLAN_HOMESCHOOL.md` H6, "TV `/tv`, panel 4 of 4").
#[derive(Clone, PartialEq, Debug)]
pub struct TvSchool {
    /// The week number for the header (`🏠 School · Week 3`), when there is
    /// an enrollment to take it from.
    pub week: Option<i64>,
    pub state: TvSchoolState,
}

/// The five things the School panel can be: the working day, and the four
/// state cards H6 names.
#[derive(Clone, PartialEq, Debug)]
pub enum TvSchoolState {
    /// `get_homeschool_today` has not answered yet — the same shape as the
    /// routine panel's "Loading today's routine…".
    Loading,
    /// This boy has no enrollment (H6: "No school plan for Simeon").
    NotEnrolled(String),
    /// `paused`, or a day outside `school_days` (H2's "School's out ⚽").
    NoSchoolToday,
    /// `current_week > weeks` (H2, §4 default 9).
    YearComplete,
    /// Nothing left to do: the routine's 8/8-style celebration.
    AllDone { done: u32, total: u32 },
    /// His own work, in the two sections the remote walks.
    Day {
        due_today: Vec<DayItem>,
        catch_up: Vec<DayItem>,
        done: u32,
        total: u32,
    },
}

/// Only the boy's **own** rows reach the television.
///
/// W-16: a shared read-aloud is one book and one reader, so it is ticked on
/// the phone by whoever is holding it — it renders under **Together** there
/// and never on the kiosk, where two brothers would each tick it. An extra is
/// per boy and per date by construction (§4 default 16), so it always stays.
fn his_own(items: &[DayItem]) -> Vec<DayItem> {
    items
        .iter()
        .filter(|item| match item {
            DayItem::Lesson(lesson) => !lesson.shared,
            DayItem::Extra(_) => true,
        })
        .cloned()
        .collect()
}

/// Slice [`TvModel::homeschool`] down to the active boy (pure).
pub fn school(model: &TvModel) -> TvSchool {
    let loading = TvSchool {
        week: None,
        state: TvSchoolState::Loading,
    };
    let (Some(view), Some(profile)) = (model.homeschool.as_ref(), model.active_profile()) else {
        return loading;
    };

    let Some((group, boy)) = view.groups.iter().find_map(|group| {
        group
            .boys
            .iter()
            .find(|boy| boy.user_id == profile.id)
            .map(|boy| (group, boy))
    }) else {
        return TvSchool {
            week: None,
            state: TvSchoolState::NotEnrolled(profile.name.clone()),
        };
    };

    let week = Some(group.week);
    // Year complete first: on a Wednesday in June both this and "no school
    // today" would be true of a finished year, and 🎉 is the truer sentence.
    if group.year_complete {
        return TvSchool {
            week,
            state: TvSchoolState::YearComplete,
        };
    }
    if group.paused || !view.is_school_day {
        return TvSchool {
            week,
            state: TvSchoolState::NoSchoolToday,
        };
    }

    let due_today = his_own(&boy.due_today);
    let catch_up = his_own(&boy.catch_up);
    if due_today.is_empty() && catch_up.is_empty() {
        return TvSchool {
            week,
            state: TvSchoolState::AllDone {
                done: boy.done_count,
                total: boy.total_count,
            },
        };
    }

    TvSchool {
        week,
        state: TvSchoolState::Day {
            due_today,
            catch_up,
            done: boy.done_count,
            total: boy.total_count,
        },
    }
}

/// Every focusable row of the School panel, in the order the remote walks
/// them: today's work, then the week's catch-up.
pub fn school_rows(model: &TvModel) -> Vec<DayItem> {
    match school(model).state {
        TvSchoolState::Day {
            due_today,
            catch_up,
            ..
        } => due_today.into_iter().chain(catch_up).collect(),
        _ => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Focus order — the golden-file contract
// ---------------------------------------------------------------------------

/// Every focusable element on the kiosk, in the order the DOM renders them
/// and the order Up/Down walks them.
///
/// An open overlay owns the screen, so it also owns the whole focus order:
/// there is exactly one thing to press when the QR is up, and `Backspace`
/// does the same thing (R-11 — no Escape).
pub fn focus_order(model: &TvModel) -> Vec<FocusId> {
    if model.state.overlay != TvOverlay::None {
        return vec![FocusId::OverlayClose];
    }

    let mut order = rail_order(model);
    order.extend(body_order(model));
    order
}

/// The left rail: one entry per profile, then the phone-join QR.
pub fn rail_order(model: &TvModel) -> Vec<FocusId> {
    let mut order: Vec<FocusId> = model
        .profiles
        .iter()
        .map(|profile| FocusId::Profile(profile.id))
        .collect();
    order.push(FocusId::JoinQr);
    order
}

/// The current panel's list.
pub fn body_order(model: &TvModel) -> Vec<FocusId> {
    match model.state.panel {
        TvPanel::Routine => model
            .routine
            .iter()
            .map(|item| FocusId::RoutineItem(item.template_id))
            .chain(model.tasks.iter().map(|task| FocusId::CustomTask(task.id)))
            .collect(),
        TvPanel::Calendar => model
            .events
            .ready()
            .map(|events| {
                events
                    .iter()
                    .map(|event| FocusId::Event(event.id.clone()))
                    .collect()
            })
            .unwrap_or_default(),
        TvPanel::Whiteboard => Vec::new(),
        TvPanel::Homeschool => school_rows(model).iter().map(day_item_focus).collect(),
    }
}

/// The one element the remote's cursor is on, if any.
pub fn current_focus(model: &TvModel) -> Option<FocusId> {
    if model.state.overlay != TvOverlay::None {
        return Some(FocusId::OverlayClose);
    }
    match model.state.zone {
        TvZone::ProfileRail => rail_order(model).get(model.state.rail_index).cloned(),
        TvZone::PanelBody => body_order(model).get(model.state.body_index).cloned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::components::tv::fixture::canonical_model;

    #[test]
    fn panels_cycle_in_both_directions_and_wrap() {
        assert_eq!(TvPanel::Routine.next(), TvPanel::Calendar);
        assert_eq!(TvPanel::Calendar.next(), TvPanel::Whiteboard);
        assert_eq!(TvPanel::Whiteboard.next(), TvPanel::Homeschool);
        assert_eq!(TvPanel::Homeschool.next(), TvPanel::Routine);
        // HS6: School is one `Left` from the routine the kiosk boots on.
        assert_eq!(TvPanel::Routine.previous(), TvPanel::Homeschool);
        assert_eq!(TvPanel::Homeschool.previous(), TvPanel::Whiteboard);
    }

    #[test]
    fn a_phones_restore_puts_the_television_back_on_the_routine() {
        assert_eq!(TvPanel::from_view(MaximizedView::None), TvPanel::Routine);
        assert_eq!(
            TvPanel::from_view(MaximizedView::Whiteboard),
            TvPanel::Whiteboard
        );
        for panel in TvPanel::ALL {
            assert_eq!(TvPanel::from_view(panel.to_view()), panel);
        }
    }

    #[test]
    fn dom_ids_are_stable_and_slugged() {
        assert_eq!(FocusId::Profile(3).dom_id(), "tv-profile-3");
        assert_eq!(FocusId::RoutineItem(7).dom_id(), "tv-routine-7");
        assert_eq!(FocusId::CustomTask(12).dom_id(), "tv-task-12");
        assert_eq!(FocusId::JoinQr.dom_id(), "tv-join-qr");
        assert_eq!(FocusId::OverlayClose.dom_id(), "tv-overlay-close");
        assert_eq!(
            FocusId::Event("google:AbC_12".into()).dom_id(),
            "tv-event-google-abc-12"
        );
        assert_eq!(
            FocusId::Lesson("s7-a0-2026-09-02".into()).dom_id(),
            "tv-lesson-s7-a0-2026-09-02"
        );
        assert_eq!(FocusId::Extra(501).dom_id(), "tv-extra-501");
    }

    #[test]
    fn a_lesson_key_is_the_unique_indexs_own_tuple() {
        let mut occurrence = crate::client::components::tv::fixture::canonical_school()
            .groups
            .remove(0)
            .boys
            .remove(0)
            .due_today
            .into_iter()
            .find_map(|item| match item {
                DayItem::Lesson(lesson) => Some(lesson),
                DayItem::Extra(_) => None,
            })
            .expect("the fixture day opens with a lesson");

        occurrence.subject_id = 7;
        occurrence.assignment_id = None;
        occurrence.scheduled_date = "2026-09-02".to_string();
        assert_eq!(lesson_key(&occurrence), "s7-a0-2026-09-02");

        // A daily subject with no per-week row keys on 0, exactly as
        // `IFNULL(assignment_id, 0)` does; a row with one keys on its id.
        occurrence.assignment_id = Some(31);
        assert_eq!(lesson_key(&occurrence), "s7-a31-2026-09-02");
    }

    #[test]
    fn the_rail_always_ends_with_the_phone_qr() {
        let model = canonical_model();
        let rail = rail_order(&model);
        assert_eq!(rail.last(), Some(&FocusId::JoinQr));
        assert_eq!(rail.len(), model.profiles.len() + 1);
    }

    #[test]
    fn an_open_overlay_owns_the_entire_focus_order() {
        let mut model = canonical_model();
        model.state.overlay = TvOverlay::JoinQr;
        assert_eq!(focus_order(&model), vec![FocusId::OverlayClose]);
        assert_eq!(current_focus(&model), Some(FocusId::OverlayClose));
    }

    #[test]
    fn the_whiteboard_panel_has_nothing_to_focus_because_drawing_is_phone_only() {
        let mut model = canonical_model();
        model.state.panel = TvPanel::Whiteboard;
        assert!(body_order(&model).is_empty());
        assert_eq!(TvLayout::of(&model).body_len(TvPanel::Whiteboard), 0);
    }

    #[test]
    fn set_active_profile_moves_the_rail_cursor_onto_that_profile() {
        let mut model = canonical_model();
        let target = model.profiles[2].id;
        let changed =
            model.apply_server_message(&ServerMessage::SetActiveProfile { user_id: target });
        assert!(changed);
        assert_eq!(model.state.rail_index, 2);
        assert_eq!(model.state.active_profile, Some(target));
        assert_eq!(current_focus(&model), Some(FocusId::Profile(target)));
    }

    #[test]
    fn set_active_profile_for_an_unknown_id_changes_nothing() {
        let mut model = canonical_model();
        let before = model.state;
        assert!(!model.apply_server_message(&ServerMessage::SetActiveProfile { user_id: 999 }));
        assert_eq!(model.state, before);
    }

    #[test]
    fn switching_to_a_shorter_panel_clamps_the_body_cursor() {
        let mut model = canonical_model();
        model.state.zone = TvZone::PanelBody;
        model.state.body_index = 7;
        let layout = TvLayout::of(&model);
        model.state.set_panel(TvPanel::Calendar, &layout);
        assert!(model.state.body_index < layout.body_len(TvPanel::Calendar));
    }

    #[test]
    fn switching_to_an_empty_panel_returns_the_cursor_to_the_rail() {
        let mut model = canonical_model();
        model.state.zone = TvZone::PanelBody;
        model.state.body_index = 3;
        let layout = TvLayout::of(&model);
        model.state.set_panel(TvPanel::Whiteboard, &layout);
        assert_eq!(model.state.zone, TvZone::ProfileRail);
        assert_eq!(model.state.body_index, 0);
    }
}
