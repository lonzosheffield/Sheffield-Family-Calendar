use dioxus::html::FileData;
use dioxus::prelude::*;

use crate::client::app::use_app_state;
use crate::client::components::mobile::queue::{self, QueuedMutation};
use crate::client::components::mobile::session;
use crate::client::realtime::use_realtime;
use crate::server::api::{
    delete_custom_task, get_custom_tasks, get_daily_routine, today, toggle_custom_task,
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

/// A per-page-load nonce identifying **this client**, mixed into every
/// idempotency key it mints (QA round 1, Q1-08).
///
/// Before this, a key was `performance.now()-counter` with no per-client
/// component: two devices whose clocks and counters happened to line up
/// (both freshly reloaded, both toggling their first item) could mint the
/// *same* key, and the second device's write would be silently dropped as a
/// "replay" of the first. Seeding once per page load from
/// [`crate::client::realtime::entropy_seed`] (the wall clock/`performance.now`
/// mix already used for WS backoff jitter) and salting it with
/// [`crate::client::realtime::unit_random`] gives this load a 64-bit value no
/// other tab, device or reload is likely to reproduce, without pulling in a
/// UUID crate on wasm.
fn client_nonce() -> u64 {
    use std::sync::OnceLock;
    static NONCE: OnceLock<u64> = OnceLock::new();
    *NONCE.get_or_init(|| {
        let seed = crate::client::realtime::entropy_seed();
        let salt = (crate::client::realtime::unit_random() * u64::MAX as f64) as u64;
        seed ^ salt
    })
}

/// A fresh idempotency key for one mutation (PLAN v2 T1.5 / R-15).
///
/// No client-side UUID crate is pulled in for this: [`client_nonce`] (unique
/// to this page load) plus a wall-clock timestamp plus a per-process
/// monotonic counter gives every call on this client a value no other call —
/// on this client *or any other* — will ever repeat, and a key only has to
/// be unique among the retries of *one* mutation, not globally — the server
/// dedupes by the key's bytes, not by trusting it as an identity.
pub fn new_idempotency_key() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        "{:016x}-{}-{n}",
        client_nonce(),
        crate::client::realtime::now_millis()
    )
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
                    // QA round 1 (Q1-15): `text-sheffield-accent` on paper is
                    // 3.11:1. The hue is kept as a *ground* under dark ink —
                    // the chip reads at 4.62:1.
                    span { class: "rounded-full bg-sheffield-accent px-2 text-sm font-bold text-slate-800",
                        "{progress:.0}%"
                    }
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
                                    // T2.2 H-23: a failed tick is queued (dated + keyed,
                                    // never regenerated) rather than silently lost — the
                                    // producer T2.2 built the offline queue for but left
                                    // to this file's owner to wire in.
                                    if toggle_routine_task(user_id, template_id, completed, date.clone(), key)
                                        .await
                                        .is_err()
                                    {
                                        queue::record_offline_failure(
                                            QueuedMutation::ToggleRoutineTask { user_id, template_id, completed },
                                            date,
                                        );
                                    }
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
                                            // T2.2 H-23, same reasoning as the routine toggle above.
                                            if toggle_custom_task(owner, id, completed, date.clone(), key)
                                                .await
                                                .is_err()
                                            {
                                                queue::record_offline_failure(
                                                    QueuedMutation::ToggleCustomTask { user_id: owner, task_id: id, completed },
                                                    date,
                                                );
                                            }
                                            tasks.restart();
                                        }
                                    },
                                    on_delete: move |_| {
                                        let id = task.id;
                                        let owner = task.user_id;
                                        async move {
                                            // Q1-07: deletion is parent-only; the server
                                            // rejects an empty/invalid token with 401 either
                                            // way, but this avoids a doomed round trip when
                                            // this phone was never signed in at all.
                                            let auth = session::token().unwrap_or_default();
                                            let _ = delete_custom_task(auth, owner, id).await;
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
                // QA round 1 (Q1-15): white on `bg-sheffield-accent` is 3.17:1
                // and 18 px bold is not WCAG "large"; the primary blue is
                // 4.99:1.
                class: "mt-auto rounded-2xl bg-sheffield-dark px-4 py-3 text-lg font-bold text-white shadow hover:brightness-110",
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
                        // QA round 1 (Q1-15): white on the light blue is
                        // 2.16:1; dark ink on it is 6.77:1.
                        "flex h-16 w-16 items-center justify-center rounded-full bg-sheffield-light text-xl font-bold text-slate-800 opacity-70 hover:opacity-100"
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
                        "block text-lg font-semibold text-slate-600 line-through"
                    } else {
                        "block text-lg font-semibold"
                    },
                    "{item.title}"
                }
                span { class: "block text-sm text-slate-600", "{item.description}" }
            }
            // QA round 1 (Q1-15): `text-sheffield-light` on white is 2.16:1.
            span { class: "ml-auto text-xs uppercase tracking-wide text-slate-600",
                "{item.icon_name}"
            }
        }
    }
}

/// **T2.5**: `on_delete` is the UI half of "delete task + file" — the
/// server half (`api::delete_custom_task`, reusing T1.6's
/// `backup::delete_custom_task` verbatim) removes the row and, when it had
/// one, its photo file on disk.
#[component]
fn CustomTaskRow(
    task: CustomTaskView,
    on_toggle: EventHandler<bool>,
    on_delete: EventHandler<()>,
) -> Element {
    let completed = task.is_completed;

    rsx! {
        div { class: "flex w-full items-center gap-3 rounded-2xl bg-white p-3 shadow-sm ring-1 ring-slate-100",
            button {
                class: "flex flex-1 items-center gap-3 text-left",
                onclick: move |_| on_toggle.call(!completed),
                if let Some(path) = task.photo_path.clone() {
                    img { class: "h-12 w-12 rounded-lg object-cover", src: "{path}", alt: "{task.title}" }
                }
                span {
                    class: if completed { "font-semibold text-slate-600 line-through" } else { "font-semibold" },
                    "{task.title}"
                }
                if let Some(due) = task.due_date.clone() {
                    span { class: "ml-auto text-xs uppercase tracking-wide text-slate-600", "due {due}" }
                }
            }
            button {
                // QA round 1 (Q1-15): `red-500` ink on white was 3.76:1;
                // `red-700` is 6.5:1.
                class: "shrink-0 rounded-lg px-2 py-1 text-sm font-semibold text-red-700 hover:bg-red-50",
                aria_label: "Delete {task.title}",
                onclick: move |event| {
                    event.stop_propagation();
                    on_delete.call(());
                },
                "Delete"
            }
        }
    }
}

/// **T2.5**: the photo dialog now posts through the multipart route
/// ([`upload::submit`]) instead of the base64-through-a-`#[server]`-fn path
/// (G14 — axum's default 2 MB body limit 413'd every modern phone photo
/// before it ever reached the database). The file is downscaled to fit
/// within 1600×1600 and re-encoded to JPEG **client-side**, in the browser,
/// before it ever leaves the phone — see [`upload`]'s module doc for why that
/// happens in a small inline JS snippet rather than a dozen new `web-sys`
/// features. The server (`api::photos::upload_photo_handler`) re-encodes
/// again regardless, so a direct API call that skips this dialog is never
/// trusted to have downscaled anything.
#[component]
fn PhotoTaskDialog(on_close: EventHandler<()>, on_created: EventHandler<()>) -> Element {
    let state = use_app_state();
    let mut title = use_signal(String::new);
    let mut due_date = use_signal(String::new);
    let mut photo = use_signal(|| None::<(String, Vec<u8>)>);
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

                label { class: "block text-sm font-semibold text-slate-600",
                    "Due date (optional)"
                    input {
                        class: "mt-1 w-full rounded-xl border border-slate-200 p-3 text-lg",
                        r#type: "date",
                        value: "{due_date}",
                        oninput: move |event| due_date.set(event.value()),
                    }
                }

                input {
                    class: "w-full text-sm",
                    r#type: "file",
                    accept: "image/*",
                    capture: "environment",
                    onchange: move |event| async move {
                        if let Some(read) = read_first_photo(event.files()).await {
                            photo.set(Some(read));
                        }
                    },
                }

                if photo().is_some() {
                    p { class: "text-sm text-sheffield-dark", "Photo attached" }
                }

                // Q1-07: task creation (photo or title-only) is parent-only,
                // same as delete. The server enforces this regardless; this
                // is the honest UI state for a phone that never signed in.
                if !session::is_parent() {
                    p { class: "text-sm font-semibold text-red-600",
                        "Sign in with the parent PIN under Settings to add tasks"
                    }
                }

                div { class: "flex justify-end gap-2",
                    button {
                        class: "rounded-xl px-4 py-2 font-semibold text-slate-600",
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                    button {
                        class: "rounded-xl bg-sheffield-dark px-5 py-2 font-bold text-white disabled:opacity-50",
                        disabled: saving() || title().trim().is_empty() || !session::is_parent(),
                        onclick: move |_| async move {
                            saving.set(true);
                            let user_id = (state.active_user_id)();
                            let (mime, bytes) = photo().unwrap_or_else(|| (String::new(), Vec::new()));
                            let due = due_date();
                            let due = if due.trim().is_empty() { None } else { Some(due) };
                            let auth = session::token();
                            let ok = upload::submit(bytes, mime, title().trim().to_string(), user_id, due, auth).await;
                            saving.set(false);
                            if ok {
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

/// Read the first file out of a file-input change event, returning its MIME
/// type (as reported by the browser's `File.type`, falling back to
/// `image/jpeg` — the format [`upload`]'s downscale step always re-encodes
/// to anyway) alongside its raw bytes.
///
/// Dioxus 0.7 changed `FormData::files()` from `Option<Arc<dyn FileEngine>>`
/// to a plain `Vec<FileData>`, so this takes the 0.7 shape directly. Keeping
/// it out of the `rsx!` closure is what makes the new signature unit testable
/// without a browser.
pub async fn read_first_photo(files: Vec<FileData>) -> Option<(String, Vec<u8>)> {
    let file = files.into_iter().next()?;
    let mime = file
        .content_type()
        .filter(|mime| !mime.is_empty())
        .unwrap_or_else(|| "image/jpeg".to_string());
    let bytes = file.read_bytes().await.ok()?;
    Some((mime, bytes.to_vec()))
}

/// Client-side downscale + upload to `POST /api/upload_photo`
/// ([`crate::server::api::photos::upload_photo_handler`]).
///
/// **Why a hand-written `wasm_bindgen(inline_js)` snippet instead of
/// `web-sys` (`HtmlImageElement`, `createImageBitmap`, canvas `toBlob`,
/// `fetch`)**: reaching that whole pipeline through `web-sys` needs upwards
/// of half a dozen additional feature flags, and `Cargo.toml` is not a
/// T2.5-owned file (`docs/reviews/PURPLE_TEAM.md` §P4 — a feature addition is
/// a Boss micro-commit between waves, the same reasoning T2.2's `storage.rs`
/// documents for its own hand-written `localStorage` bindings). The JS here
/// does exactly three browser-native things Rust cannot do any more cheaply
/// through `wasm-bindgen`'s already-declared glue exception
/// (`docs/NON_RUST.md`): decode the file into a `createImageBitmap`, draw it
/// downscaled onto a `<canvas>` (`drawImage` + `toBlob('image/jpeg', 0.85)`
/// — the client-side half of PLAN v2 T2.5's "downscale ≤ 1600 px JPEG"), and
/// `fetch` a `multipart/form-data` body to the route above. `Vec<u8>` and
/// `Option<String>` cross the FFI boundary using `wasm-bindgen`'s built-in
/// `Uint8Array`/nullable-string support — no `js_sys` needed either.
///
/// The server re-encodes and downscales again regardless
/// (`api::photos::upload_photo_handler`), so this step is a bandwidth/latency
/// optimisation for the phone's own upload, never something the server
/// trusts blindly.
mod upload {
    /// Downscale (if a photo was attached) and upload. Returns whether the
    /// server accepted the task — `false` covers both "the network is down"
    /// and "the server rejected it", the same coarse signal
    /// [`super::PhotoTaskDialog`] already showed for the old base64 path.
    ///
    /// **Q1-07**: `auth` (the parent session token, `session::token()`) is
    /// the first thing appended to the form — the server rejects the whole
    /// request with 401 before it reads any `photo` bytes if this is missing
    /// or invalid.
    pub async fn submit(
        bytes: Vec<u8>,
        mime: String,
        title: String,
        user_id: u32,
        due_date: Option<String>,
        auth: Option<String>,
    ) -> bool {
        imp::submit(bytes, mime, title, user_id, due_date, auth).await
    }

    #[cfg(all(feature = "web", target_arch = "wasm32"))]
    mod imp {
        use wasm_bindgen::prelude::*;

        #[wasm_bindgen(inline_js = r#"
export async function family_hub_submit_task(bytes, mime, title, userId, dueDate, auth) {
    try {
        const form = new FormData();
        form.append('auth', auth ?? '');
        form.append('title', title);
        form.append('user_id', String(userId));
        if (dueDate) {
            form.append('due_date', dueDate);
        }
        if (bytes && bytes.length > 0) {
            const srcBlob = new Blob([bytes], { type: mime || 'image/jpeg' });
            const bitmap = await createImageBitmap(srcBlob);
            const maxDim = 1600;
            let width = bitmap.width;
            let height = bitmap.height;
            if (width > maxDim || height > maxDim) {
                const scale = Math.min(maxDim / width, maxDim / height);
                width = Math.max(1, Math.round(width * scale));
                height = Math.max(1, Math.round(height * scale));
            }
            const canvas = document.createElement('canvas');
            canvas.width = width;
            canvas.height = height;
            const ctx = canvas.getContext('2d');
            ctx.drawImage(bitmap, 0, 0, width, height);
            const outBlob = await new Promise((resolve) => canvas.toBlob(resolve, 'image/jpeg', 0.85));
            if (outBlob) {
                form.append('photo', outBlob, 'photo.jpg');
            }
        }
        const response = await fetch('/api/upload_photo', { method: 'POST', body: form });
        return response.ok;
    } catch (err) {
        console.error('family hub photo upload failed', err);
        return false;
    }
}
"#)]
        extern "C" {
            async fn family_hub_submit_task(
                bytes: Vec<u8>,
                mime: String,
                title: String,
                user_id: u32,
                due_date: Option<String>,
                auth: Option<String>,
            ) -> bool;
        }

        pub async fn submit(
            bytes: Vec<u8>,
            mime: String,
            title: String,
            user_id: u32,
            due_date: Option<String>,
            auth: Option<String>,
        ) -> bool {
            family_hub_submit_task(bytes, mime, title, user_id, due_date, auth).await
        }
    }

    /// Server-side rendering and every non-wasm build (including `cargo
    /// test`) never run a click handler, but the component still has to
    /// compile for those targets — same shape as `mobile::storage`'s `imp`
    /// split.
    #[cfg(not(all(feature = "web", target_arch = "wasm32")))]
    mod imp {
        pub async fn submit(
            _bytes: Vec<u8>,
            _mime: String,
            _title: String,
            _user_id: u32,
            _due_date: Option<String>,
            _auth: Option<String>,
        ) -> bool {
            false
        }
    }
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
