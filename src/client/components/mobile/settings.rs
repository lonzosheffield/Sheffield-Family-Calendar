//! The **Settings** tab: parent sign-in, the offline queue's state, and the
//! install/offline promise the family can actually read on the phone.
//!
//! Nothing here decides anything about authentication. The six digits go
//! straight to `api::profiles::verify_parent_pin`, which is the only place a
//! PIN is ever checked (T1.4, R-17/R-23), and this tab stores whatever token
//! comes back. A wrong PIN is answered slowly by the server's exponential
//! backoff, with no lockout — a wall display that can lock a parent out is a
//! self-inflicted outage (§P5.5 default 9).

use dioxus::prelude::*;

use crate::client::components::mobile::queue::OfflineQueue;
use crate::client::components::mobile::session;
use crate::server::api::verify_parent_pin;

/// PIN length (§P5.5 default 9 — six digits, not four).
const PIN_LENGTH: usize = 6;

#[component]
pub fn MobileSettings(queue_version: u64, on_retry: EventHandler<()>) -> Element {
    let mut pin = use_signal(String::new);
    let mut status = use_signal(|| Option::<String>::None);
    let mut signed_in = use_signal(session::is_parent);

    // Reading `queue_version` here is what makes the badge below re-render
    // when the shell enqueues or replays something.
    let _ = queue_version;
    let queue = OfflineQueue::load();

    rsx! {
        div { class: "flex flex-col gap-6",
            section {
                h2 { class: "mb-1 text-lg font-bold text-sheffield-dark", "Parent sign-in" }
                if signed_in() {
                    p { class: "mb-3 text-sm text-slate-500",
                        "This phone is signed in as a parent and can control the TV and edit the family's settings."
                    }
                    button {
                        class: "rounded-2xl bg-white px-4 py-3 text-base font-semibold text-sheffield-accent shadow ring-1 ring-slate-200",
                        onclick: move |_| {
                            session::clear();
                            signed_in.set(false);
                            status.set(Some("Signed out on this phone.".into()));
                        },
                        "Sign out"
                    }
                } else {
                    p { class: "mb-3 text-sm text-slate-500",
                        "Enter the six-digit parent PIN. The hub checks it, never this phone."
                    }
                    form {
                        class: "flex gap-2",
                        onsubmit: move |event| async move {
                            // Dioxus 0.7 submits the form by default (D7' break #7).
                            event.prevent_default();
                            let attempt = pin();
                            if attempt.len() != PIN_LENGTH
                                || !attempt.chars().all(|c| c.is_ascii_digit())
                            {
                                status.set(Some(format!("The PIN is {PIN_LENGTH} digits.")));
                                return;
                            }
                            match verify_parent_pin(attempt).await {
                                Ok(token) => {
                                    session::store(&token);
                                    signed_in.set(true);
                                    pin.set(String::new());
                                    status.set(Some("Signed in.".into()));
                                }
                                Err(_) => {
                                    pin.set(String::new());
                                    status
                                        .set(
                                            Some("That PIN was not accepted. Try again.".into()),
                                        );
                                }
                            }
                        },
                        input {
                            class: "min-w-0 flex-1 rounded-2xl bg-white px-4 py-3 text-lg tracking-[0.4em] shadow ring-1 ring-slate-200",
                            r#type: "password",
                            inputmode: "numeric",
                            autocomplete: "off",
                            maxlength: PIN_LENGTH as i64,
                            aria_label: "Parent PIN",
                            value: "{pin}",
                            oninput: move |event| pin.set(event.value()),
                        }
                        button {
                            class: "rounded-2xl bg-sheffield-dark px-5 py-3 text-base font-semibold text-white shadow",
                            r#type: "submit",
                            "Sign in"
                        }
                    }
                }
                if let Some(message) = status() {
                    p { class: "mt-2 text-sm font-semibold text-slate-600", "{message}" }
                }
            }

            section {
                h2 { class: "mb-1 text-lg font-bold text-sheffield-dark", "Saved while offline" }
                if queue.is_empty() {
                    p { class: "text-sm text-slate-500",
                        "Nothing waiting — every change has reached the hub."
                    }
                } else {
                    p { class: "mb-3 text-sm text-slate-500",
                        "{queue.len()} change(s) are waiting to be sent. They keep the day they were made for, and the hub applies each one exactly once."
                    }
                    ul { class: "mb-3 flex flex-col gap-2",
                        for entry in queue.entries() {
                            li {
                                key: "{entry.key}",
                                class: "rounded-2xl bg-white px-4 py-3 text-sm shadow ring-1 ring-slate-200",
                                span { class: "font-semibold text-sheffield-dark",
                                    "{entry.mutation.label()}"
                                }
                                span { class: "text-slate-500", " · for {entry.date}" }
                            }
                        }
                    }
                    button {
                        class: "rounded-2xl bg-sheffield-dark px-5 py-3 text-base font-semibold text-white shadow",
                        onclick: move |_| on_retry.call(()),
                        "Try sending now"
                    }
                }
            }

            section {
                h2 { class: "mb-1 text-lg font-bold text-sheffield-dark", "Offline & install" }
                p { class: "text-sm text-slate-500",
                    "Add the hub to your home screen to use it like an app. On Android changes are sent again as soon as the phone reconnects; on iPhone they are sent the next time you open the app. Either way nothing is lost for two days, and nothing is applied twice."
                }
            }
        }
    }
}
