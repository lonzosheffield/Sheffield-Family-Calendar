//! The remote's state machine.
//!
//! One pure function — [`on_key`] — takes the cursor's position, a key, and
//! the shape of the lists, and returns the new position plus at most one
//! side effect for the shell to perform. No DOM, no signals, no clock: the
//! whole of PURPLE §P3 T2.1 (d) ("for each of `ArrowUp/Down/Left/Right/
//! Enter/Backspace/MediaPlayPause`, assert the resulting focus/view
//! transition") and (e) ("every routine item is reachable from the profile
//! selector in ≤ 12 key presses") are `assert_eq!`s over this function.
//!
//! ## The rules, in full
//!
//! | Key | On the profile rail | In the panel body | With the QR overlay open |
//! | --- | --- | --- | --- |
//! | Up / Down | move the rail cursor, wrapping — landing on a profile selects it | move the list cursor, wrapping | — |
//! | Left / Right | previous / next panel | previous / next panel | — |
//! | Enter | on a profile: enter its list · on the QR entry: open the overlay | toggle the focused item | close |
//! | Backspace | return to the default panel | return to the rail | close |
//! | Play/Pause | toggle the QR overlay | toggle the QR overlay | close |
//!
//! Wrapping is not decoration: it is what bounds (e). With a wrapping list of
//! *n* items the furthest item is ⌊n/2⌋ presses away, not *n* − 1.

use super::keymap::TvKey;
use super::model::{
    body_order, rail_order, FocusId, TvLayout, TvModel, TvOverlay, TvPanel, TvState, TvZone,
};

/// The one side effect a key press may ask the shell to perform.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub enum TvAction {
    #[default]
    None,
    /// The rail cursor landed on a profile: show that profile's routine and
    /// tell the rest of the app (`AppState::active_user_id`).
    SelectProfile(i64),
    /// `Enter` on a body item: toggle it, follow it, whatever it is.
    Activate(FocusId),
    /// Show the phone-join QR.
    OpenOverlay(TvOverlay),
    /// Dismiss whatever overlay is up.
    CloseOverlay,
}

/// The result of one key press.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct KeyOutcome {
    pub state: TvState,
    pub action: TvAction,
}

impl KeyOutcome {
    fn quiet(state: TvState) -> Self {
        Self {
            state,
            action: TvAction::None,
        }
    }
}

/// Move `index` by one step through `len` entries, wrapping. A zero-length
/// list stays at 0.
fn step(index: usize, len: usize, forward: bool) -> usize {
    if len == 0 {
        return 0;
    }
    let index = index.min(len - 1);
    if forward {
        (index + 1) % len
    } else {
        (index + len - 1) % len
    }
}

/// Apply one remote key press.
///
/// `rail` and `body` are the focus lists for the *current* state — pass
/// [`rail_order`] and [`body_order`] of the same model the layout came from.
/// They are taken as slices rather than re-derived so this function stays
/// free of the model's data types and is trivial to drive from a test.
pub fn on_key(
    state: TvState,
    key: TvKey,
    layout: &TvLayout,
    rail: &[FocusId],
    body: &[FocusId],
) -> KeyOutcome {
    // An overlay captures everything. Only the three "dismiss" keys do
    // anything, and there is deliberately no `Escape` among them (R-11).
    if state.overlay != TvOverlay::None {
        return match key {
            TvKey::Enter | TvKey::Back | TvKey::PlayPause => {
                let mut next = state;
                next.overlay = TvOverlay::None;
                KeyOutcome {
                    state: next,
                    action: TvAction::CloseOverlay,
                }
            }
            _ => KeyOutcome::quiet(state),
        };
    }

    let mut next = state;

    match key {
        TvKey::Up | TvKey::Down => {
            let forward = key == TvKey::Down;
            match state.zone {
                TvZone::ProfileRail => {
                    next.rail_index = step(state.rail_index, layout.rail_len, forward);
                    // Landing on a profile *is* selecting it — that is D8's
                    // "Up/Down switches profile".
                    if let Some(FocusId::Profile(id)) = rail.get(next.rail_index) {
                        next.active_profile = Some(*id);
                        return KeyOutcome {
                            state: next,
                            action: TvAction::SelectProfile(*id),
                        };
                    }
                }
                TvZone::PanelBody => {
                    next.body_index = step(state.body_index, layout.body_len(state.panel), forward);
                }
            }
            KeyOutcome::quiet(next)
        }

        TvKey::Left | TvKey::Right => {
            let panel = if key == TvKey::Right {
                state.panel.next()
            } else {
                state.panel.previous()
            };
            next.set_panel(panel, layout);
            KeyOutcome::quiet(next)
        }

        TvKey::Enter => match state.zone {
            TvZone::ProfileRail => match rail.get(state.rail_index) {
                Some(FocusId::Profile(id)) => {
                    next.active_profile = Some(*id);
                    // Step into the list, but only if there is one — pressing
                    // Enter on the whiteboard panel must not strand the
                    // cursor in an empty zone.
                    if layout.body_len(state.panel) > 0 {
                        next.zone = TvZone::PanelBody;
                        next.body_index = 0;
                    }
                    KeyOutcome {
                        state: next,
                        action: TvAction::SelectProfile(*id),
                    }
                }
                Some(FocusId::JoinQr) => {
                    next.overlay = TvOverlay::JoinQr;
                    KeyOutcome {
                        state: next,
                        action: TvAction::OpenOverlay(TvOverlay::JoinQr),
                    }
                }
                _ => KeyOutcome::quiet(next),
            },
            TvZone::PanelBody => match body.get(state.body_index) {
                Some(focus) => KeyOutcome {
                    state: next,
                    action: TvAction::Activate(focus.clone()),
                },
                None => KeyOutcome::quiet(next),
            },
        },

        TvKey::Back => match state.zone {
            // Out of the list, back to the profile rail.
            TvZone::PanelBody => {
                next.zone = TvZone::ProfileRail;
                KeyOutcome::quiet(next)
            }
            // Already on the rail: "back" means back to what the kiosk is
            // for. From the routine panel it is a no-op, which is correct —
            // there is nowhere above home on a wall display.
            TvZone::ProfileRail => {
                next.set_panel(TvPanel::Routine, layout);
                KeyOutcome::quiet(next)
            }
        },

        TvKey::PlayPause => {
            next.overlay = TvOverlay::JoinQr;
            KeyOutcome {
                state: next,
                action: TvAction::OpenOverlay(TvOverlay::JoinQr),
            }
        }
    }
}

/// [`on_key`] driven straight off a model — the form the shell uses, and the
/// form the reachability search uses.
pub fn on_key_for(model: &TvModel, key: TvKey) -> KeyOutcome {
    let layout = TvLayout::of(model);
    let rail = rail_order(model);
    let body = body_order(model);
    on_key(model.state, key, &layout, &rail, &body)
}

/// Fewest key presses from `start` to a state whose cursor is on `target`,
/// or `None` if it is unreachable.
///
/// A breadth-first search over the state machine itself: it cannot be fooled
/// by a path the code does not actually implement, which is the point of
/// PURPLE §P3 T2.1 (e).
pub fn presses_to_reach(model: &TvModel, start: TvState, target: &FocusId) -> Option<usize> {
    use std::collections::{HashSet, VecDeque};

    let mut probe = model.clone();
    let mut seen: HashSet<TvState> = HashSet::new();
    let mut queue: VecDeque<(TvState, usize)> = VecDeque::new();

    seen.insert(start);
    queue.push_back((start, 0));

    while let Some((state, depth)) = queue.pop_front() {
        probe.state = state;
        if super::model::current_focus(&probe).as_ref() == Some(target) {
            return Some(depth);
        }
        for key in super::keymap::TV_KEYS {
            let outcome = on_key_for(&probe, key);
            if seen.insert(outcome.state) {
                queue.push_back((outcome.state, depth + 1));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::components::tv::fixture::canonical_model;
    use crate::client::components::tv::model::current_focus;

    fn press(model: &mut TvModel, key: TvKey) -> TvAction {
        let outcome = on_key_for(model, key);
        model.state = outcome.state;
        outcome.action
    }

    // -------------------------------------------------------------------
    // (d) one transition test per key in the D8 map
    // -------------------------------------------------------------------

    #[test]
    fn down_on_the_rail_switches_to_the_next_profile() {
        let mut model = canonical_model();
        assert_eq!(current_focus(&model), Some(FocusId::Profile(1)));

        let action = press(&mut model, TvKey::Down);
        assert_eq!(action, TvAction::SelectProfile(2));
        assert_eq!(current_focus(&model), Some(FocusId::Profile(2)));
        assert_eq!(model.state.active_profile, Some(2));
    }

    #[test]
    fn up_on_the_rail_wraps_to_the_qr_entry_and_selects_no_profile() {
        let mut model = canonical_model();
        let action = press(&mut model, TvKey::Up);
        // Wrapping backwards off profile 1 lands on the rail's last entry.
        assert_eq!(action, TvAction::None);
        assert_eq!(current_focus(&model), Some(FocusId::JoinQr));

        // ...and one more Up is the last profile.
        let action = press(&mut model, TvKey::Up);
        let last = model.profiles.last().expect("a profile").id;
        assert_eq!(action, TvAction::SelectProfile(last));
    }

    #[test]
    fn right_and_left_cycle_the_panels() {
        let mut model = canonical_model();
        assert_eq!(model.state.panel, TvPanel::Routine);

        press(&mut model, TvKey::Right);
        assert_eq!(model.state.panel, TvPanel::Calendar);
        press(&mut model, TvKey::Right);
        assert_eq!(model.state.panel, TvPanel::Whiteboard);
        press(&mut model, TvKey::Right);
        assert_eq!(model.state.panel, TvPanel::Routine);

        press(&mut model, TvKey::Left);
        assert_eq!(model.state.panel, TvPanel::Whiteboard);
    }

    #[test]
    fn enter_on_a_profile_steps_into_that_profiles_routine_list() {
        let mut model = canonical_model();
        let action = press(&mut model, TvKey::Enter);
        assert_eq!(action, TvAction::SelectProfile(1));
        assert_eq!(model.state.zone, TvZone::PanelBody);
        assert_eq!(model.state.body_index, 0);
        assert_eq!(
            current_focus(&model),
            Some(FocusId::RoutineItem(model.routine[0].template_id))
        );
    }

    #[test]
    fn enter_in_the_list_toggles_the_focused_item() {
        let mut model = canonical_model();
        press(&mut model, TvKey::Enter); // rail -> list
        press(&mut model, TvKey::Down); // second item
        let expected = FocusId::RoutineItem(model.routine[1].template_id);
        let action = press(&mut model, TvKey::Enter);
        assert_eq!(action, TvAction::Activate(expected));
        // Toggling must not move the cursor: a child ticking three items in
        // a row should not have to hunt for their place again.
        assert_eq!(model.state.body_index, 1);
        assert_eq!(model.state.zone, TvZone::PanelBody);
    }

    #[test]
    fn down_in_the_list_walks_the_routine_and_wraps() {
        let mut model = canonical_model();
        press(&mut model, TvKey::Enter);
        let body_len = body_order(&model).len();
        for _ in 0..body_len {
            press(&mut model, TvKey::Down);
        }
        assert_eq!(model.state.body_index, 0, "a full lap returns to the top");
    }

    #[test]
    fn backspace_leaves_the_list_and_then_returns_to_the_routine_panel() {
        let mut model = canonical_model();
        press(&mut model, TvKey::Right); // calendar
        press(&mut model, TvKey::Enter); // into the event list
        assert_eq!(model.state.zone, TvZone::PanelBody);

        press(&mut model, TvKey::Back);
        assert_eq!(
            model.state.zone,
            TvZone::ProfileRail,
            "back leaves the list"
        );
        assert_eq!(model.state.panel, TvPanel::Calendar);

        press(&mut model, TvKey::Back);
        assert_eq!(model.state.panel, TvPanel::Routine, "back goes home");
    }

    #[test]
    fn play_pause_opens_the_phone_qr_and_any_dismiss_key_closes_it() {
        let mut model = canonical_model();
        let action = press(&mut model, TvKey::PlayPause);
        assert_eq!(action, TvAction::OpenOverlay(TvOverlay::JoinQr));
        assert_eq!(model.state.overlay, TvOverlay::JoinQr);
        assert_eq!(current_focus(&model), Some(FocusId::OverlayClose));

        let action = press(&mut model, TvKey::Back);
        assert_eq!(action, TvAction::CloseOverlay);
        assert_eq!(model.state.overlay, TvOverlay::None);
    }

    #[test]
    fn enter_on_the_rails_qr_entry_opens_the_overlay() {
        let mut model = canonical_model();
        model.state.rail_index = model.profiles.len(); // the QR entry
        assert_eq!(current_focus(&model), Some(FocusId::JoinQr));
        let action = press(&mut model, TvKey::Enter);
        assert_eq!(action, TvAction::OpenOverlay(TvOverlay::JoinQr));
    }

    #[test]
    fn an_open_overlay_swallows_navigation_keys() {
        let mut model = canonical_model();
        press(&mut model, TvKey::PlayPause);
        let before = model.state;
        for key in [TvKey::Up, TvKey::Down, TvKey::Left, TvKey::Right] {
            let action = press(&mut model, key);
            assert_eq!(action, TvAction::None);
            assert_eq!(
                model.state, before,
                "{key:?} moved the kiosk behind a modal"
            );
        }
    }

    #[test]
    fn enter_on_the_whiteboard_panel_cannot_strand_the_cursor_in_an_empty_list() {
        let mut model = canonical_model();
        press(&mut model, TvKey::Left); // whiteboard
        assert_eq!(model.state.panel, TvPanel::Whiteboard);
        press(&mut model, TvKey::Enter);
        assert_eq!(model.state.zone, TvZone::ProfileRail);
        assert!(current_focus(&model).is_some());
    }

    // -------------------------------------------------------------------
    // (e) reachability
    // -------------------------------------------------------------------

    #[test]
    fn every_routine_item_is_within_twelve_presses_of_a_booted_kiosk() {
        let model = canonical_model();
        for item in &model.routine {
            let target = FocusId::RoutineItem(item.template_id);
            let presses = presses_to_reach(&model, TvState::initial(), &target)
                .unwrap_or_else(|| panic!("{target:?} is unreachable by remote"));
            assert!(
                presses <= 12,
                "{target:?} took {presses} presses, over the 12-press budget"
            );
        }
    }

    #[test]
    fn every_routine_item_is_within_twelve_presses_from_any_panel() {
        let model = canonical_model();
        for panel in TvPanel::ALL {
            for rail_index in 0..(model.profiles.len() + 1) {
                let start = TvState {
                    panel,
                    zone: TvZone::ProfileRail,
                    rail_index,
                    ..TvState::initial()
                };
                for item in &model.routine {
                    let target = FocusId::RoutineItem(item.template_id);
                    let presses = presses_to_reach(&model, start, &target)
                        .unwrap_or_else(|| panic!("{target:?} unreachable from {start:?}"));
                    assert!(
                        presses <= 12,
                        "{target:?} took {presses} presses from {start:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_child_can_complete_the_whole_routine_with_the_remote_alone() {
        // R-12 / D1: the television is self-sufficient. Walk the full list
        // pressing Enter on each item and assert the shell was asked to
        // toggle every single one, using nothing but the seven keys.
        let mut model = canonical_model();
        let expected: Vec<FocusId> = model
            .routine
            .iter()
            .map(|item| FocusId::RoutineItem(item.template_id))
            .collect();

        let mut toggled = Vec::new();
        press(&mut model, TvKey::Enter); // rail -> list
        for _ in 0..expected.len() {
            if let TvAction::Activate(focus) = press(&mut model, TvKey::Enter) {
                toggled.push(focus);
            }
            press(&mut model, TvKey::Down);
        }
        assert_eq!(toggled, expected);
    }
}
