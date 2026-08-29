use base64::Engine;
use dioxus::html::FileData;
use dioxus::prelude::*;

use crate::client::app::use_app_state;
use crate::client::realtime::use_realtime;
use crate::server::api::{
    create_photo_task, get_custom_tasks, get_daily_routine, today, toggle_custom_task,
    toggle_routine_task,
};
use crate::shared::types::{
    profile_name, routine_progress, CustomTaskView, RoutineItemView, FAMILY_PROFILE_COUNT,
};

// ---------------------------------------------------------------------------
// Date state machine (T1.5 / R-24a)
// ---------------------------------------------------------------------------

/// What the panel knows about "today" for the purpose of dating a mutation.
///
/// Replaces the v1 pattern of `today().await.unwrap_or_default()`, which
/// silently turned a failed fetch into `""` and let the kiosk render "nothing
/// done today" as if that were the truth rather than a missing answer
/// (R-24a). [`Self::resolve`] is the pure decision the component renders
/// from, so the transition into [`RoutineDateState::Error`] is unit-testable
/// without a browser or a live server.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum RoutineDateState {
    /// No answer yet — from either the realtime bus or a direct fetch.
    Loading,
    /// A trustworthy `YYYY-MM-DD`, safe to send back to a mutation.
    Ready(String),
    /// The fetch failed and the bus has not pushed one either. The panel must
    /// show this explicitly rather than guess.
    Error,
}

impl RoutineDateState {
    /// Resolve the date state from the realtime bus's server-pushed value
    /// (set from `Hello`/`DayRolled`, PLAN v2 D5) and, only while that is
    /// still unset, the outcome of a direct `today()` fetch.
    ///
    /// The bus wins whenever it has an answer: it is kept current by the
    /// server pushing `DayRolled` at midnight, whereas a one-shot `today()`
    /// fetch is a snapshot from whenever the component first mounted. Once
    /// the bus knows, a live socket makes a stale local fetch irrelevant.
    pub fn resolve(
        bus_today: Option<String>,
        fetch_outcome: Option<Result<String, String>>,
    ) -> Self {
        if let Some(date) = bus_today {
            return RoutineDateState::Ready(date);
        }
        match fetch_outcome {
            None => RoutineDateState::Loading,
            Some(Ok(date)) => RoutineDateState::Ready(date),
            Some(Err(_)) => RoutineDateState::Error,
        }
    }

    /// The date to stamp on a mutation, if one is known yet.
    pub fn date(&self) -> Option<&str> {
        match self {
            RoutineDateState::Ready(date) => Some(date.as_str()),
            _ => None,
        }
    }
}

/// A fresh idempotency key for one mutation (PLAN v2 T1.5 / R-15).
///
/// No client-side UUID crate is pulled in for this: `web-sys`'s `Performance`
/// clock plus a per-process monotonic counter already gives every call on
/// this client a value no other call on this client will ever repeat, and a
/// key only has to be unique among the retries of *one* mutation, not
/// globally — the server dedupes by the key's bytes, not by trusting it as an
/// identity. Mirrors [`crate::client::realtime::entropy_seed`]'s reasoning
/// for staying dependency-free on wasm.
pub fn new_idempotency_key() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}-{n}", crate::client::realtime::now_millis())
}

#[component]
pub fn Routine(compact: bool) -> Element {
    let state = use_app_state();
    let (bus, _sender) = use_realtime();
    let mut show_dialog = use_signal(|| false);

    let mut today_fetch =
        use_resource(move || async move { today().await.map_err(|err| err.to_string()) });

    let date_state =
        RoutineDateState::resolve((bus.today)(), (*today_fetch.read_unchecked()).clone());

    let mut routine = use_resource(move || async move {
        // Re-runs whenever another client toggles something.
        let _version = (bus.routine_version)();
        let user_id = (state.active_user_id)();
        let date =
            RoutineDateState::resolve((bus.today)(), (*today_fetch.read_unchecked()).clone());
        match date {
            RoutineDateState::Ready(date) => get_daily_routine(user_id, date).await.map(Some),
            _ => Ok(None),
        }
    });

    let mut tasks = use_resource(move || async move {
        let _version = (bus.routine_version)();
        get_custom_tasks((state.active_user_id)()).await
    });

    let items: Vec<RoutineItemView> = match &*routine.read_unchecked() {
        Some(Ok(Some(items))) => items.clone(),
        _ => Vec::new(),
    };
    let custom: Vec<CustomTaskView> = match &*tasks.read_unchecked() {
        Some(Ok(tasks)) => tasks.clone(),
        _ => Vec::new(),
    };
    let progress = routine_progress(&items);
    // Computed once, outside the per-row closures below: `RoutineDateState`
    // is not `Copy` and each closure only needs the resolved date, not the
    // whole state machine.
    let mutation_date: Option<String> = date_state.date().map(str::to_string);

    if date_state == RoutineDateState::Error {
        return rsx! {
            div { class: "flex h-full flex-col items-center justify-center gap-3 text-center",
                ProfileSelector {}
                div { class: "rounded-2xl bg-red-50 p-4 text-red-700 ring-1 ring-red-200",
                    p { class: "font-bold", "Can't reach the hub" }
                    p { class: "text-sm", "Today's routine can't be shown right now. Check the connection and try again." }
                    button {
                        class: "mt-3 rounded-xl bg-red-600 px-4 py-2 font-semibold text-white",
                        onclick: move |_| { today_fetch.restart(); },
                        "Retry"
                    }
                }
            }
        };
    }

    rsx! {
        div { class: "flex h-full flex-col gap-4",
            ProfileSelector {}

            div {
                class: "space-y-1",
                div { class: "flex items-baseline justify-between",
                    span { class: "text-sm font-semibold text-sheffield-dark",
                        "{profile_name((state.active_user_id)())}'s progress"
                    }
                    span { class: "text-sm font-bold text-sheffield-accent", "{progress:.0}%" }
                }
                div { class: "h-4 w-full overflow-hidden rounded-full bg-sheffield-light/30",
                    div {
                        class: "h-full rounded-full bg-sheffield-dark transition-all duration-500",
                        style: "width: {progress}%",
                    }
                }
            }

            ul { class: if compact { "space-y-3" } else { "space-y-3 overflow-auto" },
                for (item, date_for_row) in items.iter().cloned().map(|item| {
                    // Cloned fresh per item, then *moved* into the `move`
                    // closure below — capturing `mutation_date` itself
                    // there would try to move the one outer
                    // `Option<String>` again on every iteration.
                    let date = mutation_date.clone();
                    (item, date)
                }) {
                    li { key: "{item.template_id}",
                        RoutineRow {
                            item: item.clone(),
                            on_toggle: move |completed: bool| {
                                let template_id = item.template_id;
                                let user_id = (state.active_user_id)();
                                let date = date_for_row.clone();
                                async move {
                                    let Some(date) = date else { return };
                                    let key = new_idempotency_key();
                                    let _ = toggle_routine_task(user_id, template_id, completed, date, key).await;
                                    routine.restart();
                                }
                            },
                        }
                    }
                }
            }

            if !custom.is_empty() {
                div { class: "space-y-2",
                    h3 { class: "text-lg font-bold text-sheffield-dark", "Extra tasks" }
                    ul { class: "space-y-2",
                        for (task, date_for_task) in custom.iter().cloned().map(|task| {
                            // Same reasoning as `date_for_row` above: a fresh
                            // per-item clone, moved into this item's `move`
                            // closure.
                            let date = mutation_date.clone();
                            (task, date)
                        }) {
                            li { key: "{task.id}",
                                CustomTaskRow {
                                    task: task.clone(),
                                    on_toggle: move |completed: bool| {
                                        let id = task.id;
                                        let owner = task.user_id;
                                        let date = date_for_task.clone();
                                        async move {
                                            let Some(date) = date else { return };
                                            let key = new_idempotency_key();
                                            let _ = toggle_custom_task(owner, id, completed, date, key).await;
                                            tasks.restart();
                                        }
                                    },
                                }
                            }
                        }
                    }
                }
            }

            button {
                class: "mt-auto rounded-2xl bg-sheffield-accent px-4 py-3 text-lg font-bold text-white shadow hover:brightness-110",
                onclick: move |_| show_dialog.set(true),
                "Add photo task"
            }

            if show_dialog() {
                PhotoTaskDialog {
                    on_close: move |_| show_dialog.set(false),
                    on_created: move |_| {
                        show_dialog.set(false);
                        tasks.restart();
                    },
                }
            }
        }
    }
}

#[component]
fn ProfileSelector() -> Element {
    let mut state = use_app_state();
    let active = (state.active_user_id)();

    rsx! {
        div { class: "flex items-center justify-center gap-3",
            for user_id in 1..=FAMILY_PROFILE_COUNT {
                button {
                    key: "{user_id}",
                    class: if user_id == active {
                        "flex h-16 w-16 items-center justify-center rounded-full bg-sheffield-dark text-xl font-bold text-white ring-4 ring-sheffield-sun"
                    } else {
                        "flex h-16 w-16 items-center justify-center rounded-full bg-sheffield-light text-xl font-bold text-white opacity-70 hover:opacity-100"
                    },
                    aria_label: "{profile_name(user_id)}",
                    onclick: move |_| state.active_user_id.set(user_id),
                    "{user_id}"
                }
            }
        }
    }
}

#[component]
fn RoutineRow(item: RoutineItemView, on_toggle: EventHandler<bool>) -> Element {
    let completed = item.completed;

    rsx! {
        button {
            class: if completed {
                "flex w-full items-start gap-4 rounded-2xl bg-sheffield-light/25 p-4 text-left transition"
            } else {
                "flex w-full items-start gap-4 rounded-2xl bg-white p-4 text-left shadow-sm ring-1 ring-slate-100 transition hover:ring-sheffield-light"
            },
            onclick: move |_| on_toggle.call(!completed),
            span {
                class: if completed {
                    "mt-1 flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-sheffield-dark text-white"
                } else {
                    "mt-1 flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border-2 border-sheffield-light"
                },
                if completed { "✓" }
            }
            span {
                span {
                    class: if completed {
                        "block text-lg font-semibold text-slate-400 line-through"
                    } else {
                        "block text-lg font-semibold"
                    },
                    "{item.title}"
                }
                span { class: "block text-sm text-slate-500", "{item.description}" }
            }
            span { class: "ml-auto text-xs uppercase tracking-wide text-sheffield-light",
                "{item.icon_name}"
            }
        }
    }
}

#[component]
fn CustomTaskRow(task: CustomTaskView, on_toggle: EventHandler<bool>) -> Element {
    let completed = task.is_completed;

    rsx! {
        button {
            class: "flex w-full items-center gap-3 rounded-2xl bg-white p-3 text-left shadow-sm ring-1 ring-slate-100",
            onclick: move |_| on_toggle.call(!completed),
            if let Some(path) = task.photo_path.clone() {
                img { class: "h-12 w-12 rounded-lg object-cover", src: "{path}", alt: "{task.title}" }
            }
            span {
                class: if completed { "font-semibold text-slate-400 line-through" } else { "font-semibold" },
                "{task.title}"
            }
        }
    }
}

#[component]
fn PhotoTaskDialog(on_close: EventHandler<()>, on_created: EventHandler<()>) -> Element {
    let state = use_app_state();
    let mut title = use_signal(String::new);
    let mut photo = use_signal(|| None::<String>);
    let mut saving = use_signal(|| false);

    rsx! {
        div { class: "fixed inset-0 z-40 flex items-end justify-center bg-black/50 p-4 sm:items-center",
            div { class: "w-full max-w-md space-y-4 rounded-3xl bg-white p-5 shadow-2xl",
                h3 { class: "text-xl font-bold text-sheffield-dark", "New task" }

                input {
                    class: "w-full rounded-xl border border-slate-200 p-3 text-lg",
                    r#type: "text",
                    placeholder: "What needs doing?",
                    value: "{title}",
                    oninput: move |event| title.set(event.value()),
                }

                input {
                    class: "w-full text-sm",
                    r#type: "file",
                    accept: "image/*",
                    capture: "environment",
                    onchange: move |event| async move {
                        if let Some(encoded) = encode_first_photo(event.files()).await {
                            photo.set(Some(encoded));
                        }
                    },
                }

                if photo().is_some() {
                    p { class: "text-sm text-sheffield-dark", "Photo attached" }
                }

                div { class: "flex justify-end gap-2",
                    button {
                        class: "rounded-xl px-4 py-2 font-semibold text-slate-500",
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                    button {
                        class: "rounded-xl bg-sheffield-dark px-5 py-2 font-bold text-white disabled:opacity-50",
                        disabled: saving() || title().trim().is_empty(),
                        onclick: move |_| async move {
                            saving.set(true);
                            let user_id = (state.active_user_id)();
                            let result = create_photo_task(user_id, title().trim().to_string(), photo()).await;
                            saving.set(false);
                            if result.is_ok() {
                                on_created.call(());
                            }
                        },
                        "Save"
                    }
                }
            }
        }
    }
}

/// Read the first file out of a file-input change event and base64 encode it
/// for [`create_photo_task`].
///
/// Dioxus 0.7 changed `FormData::files()` from `Option<Arc<dyn FileEngine>>`
/// to a plain `Vec<FileData>`, so this takes the 0.7 shape directly. Keeping
/// it out of the `rsx!` closure is what makes the new signature unit testable
/// without a browser.
pub async fn encode_first_photo(files: Vec<FileData>) -> Option<String> {
    let file = files.into_iter().next()?;
    let bytes = file.read_bytes().await.ok()?;
    Some(base64::engine::general_purpose::STANDARD.encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------
    // RoutineDateState (T1.5 / R-24a)
    // -----------------------------------------------------------------

    #[test]
    fn bus_today_wins_even_over_a_successful_fetch() {
        let state = RoutineDateState::resolve(
            Some("2026-08-29".to_string()),
            Some(Ok("2026-08-28".to_string())),
        );
        assert_eq!(state, RoutineDateState::Ready("2026-08-29".to_string()));
    }

    #[test]
    fn no_bus_value_falls_back_to_the_fetch_outcome() {
        assert_eq!(
            RoutineDateState::resolve(None, Some(Ok("2026-08-29".to_string()))),
            RoutineDateState::Ready("2026-08-29".to_string())
        );
    }

    #[test]
    fn a_failed_fetch_with_no_bus_value_is_an_explicit_error_not_a_default() {
        // R-24a: the v1 code's `today().await.unwrap_or_default()` turned
        // this exact situation into an empty string that silently rendered
        // as "nothing done today". The panel must be able to tell the
        // difference between "it is midnight" and "the fetch failed".
        let state = RoutineDateState::resolve(None, Some(Err("network error".to_string())));
        assert_eq!(state, RoutineDateState::Error);
        assert_eq!(state.date(), None);
    }

    #[test]
    fn nothing_has_answered_yet_is_loading_not_error() {
        let state = RoutineDateState::resolve(None, None);
        assert_eq!(state, RoutineDateState::Loading);
        assert_eq!(state.date(), None);
    }

    #[test]
    fn ready_state_exposes_its_date_for_a_mutation() {
        let state = RoutineDateState::resolve(Some("2026-08-29".to_string()), None);
        assert_eq!(state.date(), Some("2026-08-29"));
    }

    // -----------------------------------------------------------------
    // Idempotency keys
    // -----------------------------------------------------------------

    #[test]
    fn idempotency_keys_are_never_repeated_on_this_client() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..500 {
            assert!(
                seen.insert(new_idempotency_key()),
                "new_idempotency_key produced a duplicate"
            );
        }
    }
}
