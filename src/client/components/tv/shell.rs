//! The live kiosk: everything [`super::surface::TvSurface`] deliberately is
//! not.
//!
//! `TvShell` owns the impure half of `/tv` — the resources, the realtime bus,
//! the clock poll, the key listener — assembles a [`TvModel`] out of them and
//! hands it to the pure surface. Splitting it this way is what makes the T2.1
//! acceptance contract testable without a browser: the surface can be
//! rendered from a fixture, and the key handler is [`super::nav::on_key`],
//! which never sees a DOM.
//!
//! # Where focus lives
//!
//! DOM focus stays on **one** element — the root `div`, which is
//! `tabindex="0"`, autofocused, and focused again on mount. The remote's
//! cursor is application state ([`TvState`]), not browser focus. On a kiosk
//! that is the right way round: a Fire TV WebView's own focus model differs
//! between Fire OS versions, which would make the focus order depend on the
//! device — exactly what PURPLE §P3 T2.1 (a)'s golden file forbids. One
//! listener, one deterministic cursor, on every branch of D2′.
//!
//! # What a phone may do to the television
//!
//! `ServerMessage::SetView` and `ServerMessage::SetActiveProfile` arrive on
//! the bus as `requested_view` / `requested_profile`; T1.2 has already
//! authorised them server-side, so an unauthenticated phone's `SetView` is
//! never fanned out and never reaches here. The two effects below feed them
//! into [`TvModel::apply_server_message`], which is the same code path the
//! acceptance tests for (b) and (c) drive.

use dioxus::prelude::*;

use crate::client::app::use_app_state;
use crate::client::components::calendar::CalendarState;
use crate::client::components::qr::{phone_join_url, DEFAULT_PHONE_PORT};
use crate::client::components::routine::{new_idempotency_key, RoutineDateState};
use crate::client::components::whiteboard::Whiteboard;
use crate::client::realtime::{now_millis, use_realtime};
use crate::server::api::{
    get_custom_tasks, get_daily_routine, get_today_events, list_profiles, toggle_custom_task,
    toggle_routine_task,
};
use crate::shared::types::{profile_name, ServerMessage, FAMILY_PROFILE_COUNT};

use super::clock::{tv_clock, TvClock, CLOCK_POLL_SECS};
use super::keymap::{push_key_log, KeyLogEntry};
use super::model::{current_focus, FocusId, TvModel, TvProfile, TvState};
use super::nav::{on_key_for, scroll_target, TvAction};
use super::staleness::{TvStaleness, BADGE_TICK_MS};
use super::surface::TvSurface;

/// Rail shown until `list_profiles()` answers — or if it never does.
///
/// A kiosk rendering an empty rail is a kiosk a child cannot use, and these
/// are the same four rows `migrations/0003_profiles.sql` seeds. Showing them
/// early costs nothing; the real roster replaces them the moment it arrives.
fn fallback_profiles() -> Vec<TvProfile> {
    const COLORS: [&str; 4] = ["#2672B3", "#8BB5DA", "#E86A58", "#F4D03F"];
    (1..=FAMILY_PROFILE_COUNT)
        .map(|user_id| TvProfile {
            id: user_id as i64,
            name: profile_name(user_id).to_string(),
            color: COLORS[(user_id as usize - 1) % COLORS.len()].to_string(),
        })
        .collect()
}

/// The date a mutation from the television is stamped with.
///
/// Same state machine as the phone's routine panel (T1.5 / R-24a): the bus's
/// server-pushed `today` wins, the clock poll is the fallback, and "we do not
/// know yet" is never silently turned into a default.
fn mutation_date(bus_today: Option<String>, clock: Option<TvClock>) -> Option<String> {
    RoutineDateState::resolve(bus_today, clock.map(|clock| Ok(clock.date)))
        .date()
        .map(str::to_string)
}

/// Apply a phone's steering message to the kiosk (D1: phones remote-control
/// the television) and mirror the result into [`AppState`], so any other
/// component on the page agrees about which profile and panel are live.
///
/// A free function rather than a closure because `use_effect` stores its
/// closure on the first render: anything captured by value would be a
/// first-frame snapshot forever.
fn steer(
    mut state: Signal<TvState>,
    mut app: crate::client::app::AppState,
    profiles: &[TvProfile],
    message: ServerMessage,
) {
    let mut model = TvModel {
        profiles: profiles.to_vec(),
        state: state.peek().to_owned(),
        ..TvModel::empty()
    };
    if !model.apply_server_message(&message) {
        return;
    }
    state.set(model.state);
    match message {
        ServerMessage::SetView { view } => app.current_view.set(view),
        ServerMessage::SetActiveProfile { user_id } => {
            if let Ok(user_id) = u32::try_from(user_id) {
                app.active_user_id.set(user_id);
            }
        }
        _ => {}
    }
}

#[component]
pub fn TvShell() -> Element {
    let mut app = use_app_state();
    let (bus, _sender) = use_realtime();

    let mut state = use_signal(TvState::initial);
    let mut key_log = use_signal(Vec::<KeyLogEntry>::new);
    let keys_debug = use_signal(platform::keys_debug_from_location);
    let join_host = use_signal(platform::host_from_location);

    // ---------------------------------------------------------------
    // The hub's clock — and, on the same round trip, its pulse.
    // ---------------------------------------------------------------
    let mut clock = use_signal(|| None::<TvClock>);
    let mut tracker = use_signal(|| TvStaleness::new(now_millis()));
    let mut now_ms = use_signal(now_millis);

    use_future(move || async move {
        loop {
            if let Ok(answer) = tv_clock().await {
                clock.set(Some(answer));
                tracker.write().record_message(now_millis());
            }
            platform::sleep_ms(CLOCK_POLL_SECS * 1_000).await;
        }
    });

    // The badge is recomputed, never latched, so it clears on the first tick
    // after the hub answers — well inside D8's "off within 2 s".
    use_future(move || async move {
        loop {
            platform::sleep_ms(BADGE_TICK_MS).await;
            now_ms.set(now_millis());
        }
    });

    // Anything the bus learns is also proof the hub is alive. Reading the
    // signals here is what makes them dependencies of this effect.
    use_effect(move || {
        let _ = (bus.today)();
        let _ = (bus.routine_version)();
        let _ = (bus.tasks_version)();
        let _ = (bus.profiles_version)();
        let _ = (bus.calendar_version)();
        let _ = (bus.resync_version)();
        // QA round 1 Q1-13: the hub now publishes `ServerMessage::Health`
        // every 25 s (`realtime::HEALTH_HEARTBEAT_INTERVAL`), which
        // `RealtimeBus::apply` lands in `stale`. Reading it here is what makes
        // the heartbeat a dependency of this effect, so an idle-but-connected
        // socket is proof of life on its own and the badge no longer depends
        // on the clock poll alone.
        let _ = (bus.stale)();
        if (bus.connected)() {
            tracker.write().record_message(now_millis());
        }
    });

    // ---------------------------------------------------------------
    // Data
    // ---------------------------------------------------------------
    let profiles_resource = use_resource(move || async move {
        let _version = (bus.profiles_version)();
        list_profiles().await
    });

    let mut routine_resource = use_resource(move || async move {
        let _version = (bus.routine_version)();
        let user_id = (app.active_user_id)();
        let date = mutation_date((bus.today)(), clock.read().as_ref().cloned());
        match date {
            Some(date) => get_daily_routine(user_id, date).await.map(Some),
            None => Ok(None),
        }
    });

    let mut tasks_resource = use_resource(move || async move {
        let _version = (bus.tasks_version)();
        get_custom_tasks((app.active_user_id)()).await
    });

    let events_resource = use_resource(move || async move {
        let _version = (bus.calendar_version)();
        get_today_events().await
    });

    // Held in a signal rather than recomputed per render so the two effects
    // below see the *current* roster: `use_effect` stores its closure once,
    // so a captured `Vec` would be frozen at the first frame's fallback.
    let mut profiles = use_signal(fallback_profiles);
    use_effect(move || {
        if let Some(Ok(rows)) = &*profiles_resource.read() {
            let children: Vec<TvProfile> = rows
                .iter()
                .filter(|profile| !profile.is_parent)
                .map(|profile| TvProfile {
                    id: profile.id,
                    name: profile.name.clone(),
                    color: profile.color.clone(),
                })
                .collect();
            if !children.is_empty() {
                profiles.set(children);
            }
        }
    });

    // ---------------------------------------------------------------
    // What a phone asked the television to do
    // ---------------------------------------------------------------
    use_effect(move || {
        if let Some(view) = (bus.requested_view)() {
            steer(
                state,
                app,
                &profiles.read(),
                ServerMessage::SetView { view },
            );
        }
    });

    use_effect(move || {
        if let Some(user_id) = (bus.requested_profile)() {
            steer(
                state,
                app,
                &profiles.read(),
                ServerMessage::SetActiveProfile { user_id },
            );
        }
    });

    // ---------------------------------------------------------------
    // The model handed to the pure surface
    // ---------------------------------------------------------------
    let routine = match &*routine_resource.read_unchecked() {
        Some(Ok(Some(items))) => items.clone(),
        _ => Vec::new(),
    };
    let tasks = match &*tasks_resource.read_unchecked() {
        Some(Ok(tasks)) => tasks.clone(),
        _ => Vec::new(),
    };
    // W3: the television must be able to tell "the hub did not answer" from
    // "there is nothing on today", so the resource is folded into the same
    // four-state machine the phone uses rather than into a bare `Vec`.
    let events = CalendarState::resolve(
        events_resource
            .read_unchecked()
            .clone()
            .map(|result| result.map_err(|error| error.to_string())),
        Vec::is_empty,
    );

    let rail: Vec<TvProfile> = profiles.read().clone();
    let mut current = state.read().to_owned();
    if current.active_profile.is_none() {
        current.active_profile = rail.first().map(|profile| profile.id);
    }

    let today = mutation_date((bus.today)(), clock.read().as_ref().cloned());

    let model = TvModel {
        profiles: rail,
        routine,
        tasks,
        events,
        state: current,
        connected: (bus.connected)(),
        stale: tracker.read().is_stale(now_ms()),
        updated_at: clock.read().as_ref().map(|clock| clock.hhmm.clone()),
        join_url: join_host()
            .as_deref()
            .map(|host| phone_join_url(host, DEFAULT_PHONE_PORT)),
        keys_debug: keys_debug(),
        key_log: key_log.read().clone(),
    };

    // ---------------------------------------------------------------
    // The remote
    // ---------------------------------------------------------------
    // QD-02 / QD-08: the element the cursor last landed on, as a DOM id.
    //
    // Held in a signal rather than scrolled from inside the key handler so
    // the scroll happens *after* Dioxus has rendered the frame the press
    // produced — the row the cursor just moved to may not exist yet when the
    // press is handled (Left/Right swaps the whole panel).
    let mut scroll_to = use_signal(|| None::<String>);
    use_effect(move || {
        if let Some(dom_id) = scroll_to() {
            platform::scroll_into_view(&dom_id);
        }
    });

    let rendered = model.clone();
    let handle_key = move |event: Event<KeyboardData>| {
        let entry = KeyLogEntry::new(event.key().to_string(), event.code().to_string());
        let mapped = entry.mapped;
        push_key_log(&mut key_log.write(), entry);

        // Every press is logged for `?keys=1`, including the ones the kiosk
        // ignores — that is what makes the overlay worth having (A5).
        let Some(remote_key) = mapped else { return };
        // Arrows scroll and Backspace navigates back in a plain WebView;
        // neither is what a D-pad press means here.
        event.prevent_default();

        let mut model = rendered.clone();
        model.state = state.peek().to_owned();
        let was = current_focus(&model);
        let outcome = on_key_for(&model, remote_key);
        state.set(outcome.state);
        model.state = outcome.state;

        // Move the viewport with the ring, in the rail and in the routine
        // list alike (QD-02, QD-08).
        if let Some(target) = scroll_target(was.as_ref(), current_focus(&model).as_ref()) {
            scroll_to.set(Some(target.dom_id()));
        }

        match outcome.action {
            TvAction::None | TvAction::OpenOverlay(_) | TvAction::CloseOverlay => {}
            TvAction::SelectProfile(id) => {
                if let Ok(user_id) = u32::try_from(id) {
                    app.active_user_id.set(user_id);
                }
            }
            TvAction::Activate(focus) => {
                let (Some(date), Some(profile)) = (today.clone(), model.active_profile()) else {
                    return;
                };
                let Ok(user_id) = u32::try_from(profile.id) else {
                    return;
                };
                match focus {
                    FocusId::RoutineItem(template_id) => {
                        let completed = !model
                            .routine
                            .iter()
                            .find(|item| item.template_id == template_id)
                            .map(|item| item.completed)
                            .unwrap_or(false);
                        spawn(async move {
                            let _ = toggle_routine_task(
                                user_id,
                                template_id,
                                completed,
                                date,
                                new_idempotency_key(),
                            )
                            .await;
                            routine_resource.restart();
                        });
                    }
                    FocusId::CustomTask(task_id) => {
                        let Some(task) = model.tasks.iter().find(|task| task.id == task_id) else {
                            return;
                        };
                        let completed = !task.is_completed;
                        let owner = task.user_id;
                        spawn(async move {
                            let _ = toggle_custom_task(
                                owner,
                                task_id,
                                completed,
                                date,
                                new_idempotency_key(),
                            )
                            .await;
                            tasks_resource.restart();
                        });
                    }
                    // Calendar rows are read-only on the television and the
                    // rail's QR entry is handled by the overlay actions:
                    // editing is phone-only (PURPLE §P5.5 default 35).
                    FocusId::Profile(_)
                    | FocusId::JoinQr
                    | FocusId::Event(_)
                    | FocusId::OverlayClose => {}
                }
            }
        }
    };

    rsx! {
        div {
            id: "tv-keyboard-host",
            class: "h-full w-full outline-none",
            tabindex: "0",
            autofocus: true,
            onkeydown: handle_key,
            onmounted: move |event| async move {
                // A kiosk has nobody to click the page first.
                let _ = event.set_focus(true).await;
            },
            TvSurface { model,
                Whiteboard {}
            }
        }
    }
}

#[cfg(all(feature = "web", target_arch = "wasm32"))]
mod platform {
    use super::super::keymap::keys_debug_enabled;

    pub async fn sleep_ms(millis: u64) {
        gloo_timers::future::TimeoutFuture::new(millis as u32).await;
    }

    /// The host the television reached the hub on — which is exactly the
    /// address a phone on the same LAN should use, so the QR needs no
    /// configuration and survives a DHCP change (RR-7).
    pub fn host_from_location() -> Option<String> {
        let location = web_sys::window()?.location();
        location.hostname().ok().filter(|host| !host.is_empty())
    }

    pub fn keys_debug_from_location() -> bool {
        web_sys::window()
            .map(|window| window.location())
            .and_then(|location| location.search().ok())
            .map(|query| keys_debug_enabled(&query))
            .unwrap_or(false)
    }

    /// Bring the element the remote's cursor just landed on into view
    /// (QD-02 / QD-08).
    ///
    /// `ScrollLogicalPosition::Nearest` in both axes is the only choice that
    /// behaves on a kiosk: `Start`/`Center` would yank an already-visible
    /// row to the top of its list on every press, and the horizontal
    /// `Nearest` keeps the rail's scroll container from sliding sideways
    /// when a focus ring pokes out of it. The default `auto` behaviour is
    /// instant — a smooth scroll would still be gliding when the next press
    /// arrives, and nobody presses Down slowly.
    pub fn scroll_into_view(dom_id: &str) {
        let Some(element) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id(dom_id))
        else {
            return;
        };
        let options = web_sys::ScrollIntoViewOptions::new();
        options.set_block(web_sys::ScrollLogicalPosition::Nearest);
        options.set_inline(web_sys::ScrollLogicalPosition::Nearest);
        element.scroll_into_view_with_scroll_into_view_options(&options);
    }
}

#[cfg(not(all(feature = "web", target_arch = "wasm32")))]
mod platform {
    /// During SSR nothing ticks: the server renders one frame and stops.
    pub async fn sleep_ms(_millis: u64) {
        std::future::pending::<()>().await
    }

    pub fn host_from_location() -> Option<String> {
        None
    }

    pub fn keys_debug_from_location() -> bool {
        false
    }

    /// There is no viewport to scroll during SSR.
    pub fn scroll_into_view(_dom_id: &str) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fallback_rail_is_the_four_seeded_children() {
        let profiles = fallback_profiles();
        assert_eq!(profiles.len(), FAMILY_PROFILE_COUNT as usize);
        assert_eq!(profiles[0].id, 1);
        assert_eq!(profiles[3].name, profile_name(4));
        for profile in &profiles {
            assert!(profile.color.starts_with('#'), "{}", profile.color);
        }
    }

    #[test]
    fn the_bus_date_beats_the_clock_poll_and_absence_is_not_a_default() {
        let clock = TvClock {
            hhmm: "07:42".into(),
            date: "2026-08-28".into(),
        };
        assert_eq!(
            mutation_date(Some("2026-08-29".into()), Some(clock.clone())),
            Some("2026-08-29".to_string())
        );
        assert_eq!(
            mutation_date(None, Some(clock)),
            Some("2026-08-28".to_string())
        );
        assert_eq!(mutation_date(None, None), None);
    }
}
