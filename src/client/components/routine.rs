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

#[component]
pub fn Routine(compact: bool) -> Element {
    let state = use_app_state();
    let (bus, _sender) = use_realtime();
    let mut show_dialog = use_signal(|| false);

    let mut routine = use_resource(move || async move {
        // Re-runs whenever another client toggles something.
        let _version = (bus.routine_version)();
        let user_id = (state.active_user_id)();
        let date = today().await.unwrap_or_default();
        get_daily_routine(user_id, date).await
    });

    let mut tasks = use_resource(move || async move {
        let _version = (bus.routine_version)();
        get_custom_tasks((state.active_user_id)()).await
    });

    let items: Vec<RoutineItemView> = match &*routine.read_unchecked() {
        Some(Ok(items)) => items.clone(),
        _ => Vec::new(),
    };
    let custom: Vec<CustomTaskView> = match &*tasks.read_unchecked() {
        Some(Ok(tasks)) => tasks.clone(),
        _ => Vec::new(),
    };
    let progress = routine_progress(&items);

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
                for item in items.iter().cloned() {
                    li { key: "{item.template_id}",
                        RoutineRow {
                            item: item.clone(),
                            on_toggle: move |completed: bool| {
                                let template_id = item.template_id;
                                let user_id = (state.active_user_id)();
                                async move {
                                    let _ = toggle_routine_task(user_id, template_id, completed).await;
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
                        for task in custom.iter().cloned() {
                            li { key: "{task.id}",
                                CustomTaskRow {
                                    task: task.clone(),
                                    on_toggle: move |completed: bool| {
                                        let id = task.id;
                                        async move {
                                            let _ = toggle_custom_task(id, completed).await;
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
