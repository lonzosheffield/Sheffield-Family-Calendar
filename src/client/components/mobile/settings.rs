//! The **Settings** tab: first-run setup, parent sign-in, the offline
//! queue's state, and the install/offline promise the family can actually
//! read on the phone.
//!
//! Nothing here decides anything about authentication. The six digits go
//! straight to `POST /api/login` (or, the very first time, to `POST
//! /api/setup` with the hub's setup code), which is the only place a PIN is
//! ever checked (T1.4, R-17/R-23); the browser stores the `HttpOnly` cookie
//! the hub answers with and this tab never sees a token at all. A wrong PIN
//! is answered slowly by the server's exponential backoff, with no lockout —
//! a wall display that can lock a parent out is a self-inflicted outage
//! (§P5.5 default 9).
//!
//! **QA round 2, Q2-02** added the first branch. Before it, this tab offered
//! only "Enter the six-digit parent PIN" → `verify_parent_pin`, which on a
//! fresh hub answers `PinNotSet`: there was no UI anywhere that could set the
//! first PIN, so `docs/OWNER_CHECKLIST.md` step 4 could not be completed and
//! every parent-only affordance (the TV remote, photo tasks, task delete,
//! calendar CRUD) was dead on a real install.

use dioxus::prelude::*;

use crate::client::components::mobile::queue::OfflineQueue;
use crate::client::components::mobile::session::{self, SessionSignal, SessionState};

/// PIN length (§P5.5 default 9 — six digits, not four).
const PIN_LENGTH: usize = 6;

/// The first-run setup code the hub prints at boot is six digits too
/// (`auth::generate_six_digit_code`).
const SETUP_CODE_LENGTH: usize = 6;

#[component]
pub fn MobileSettings(queue_version: u64, on_retry: EventHandler<()>) -> Element {
    let mut session_state: SessionSignal = use_context();
    let mut status = use_signal(|| Option::<String>::None);

    // Reading `queue_version` here is what makes the badge below re-render
    // when the shell enqueues or replays something.
    let _ = queue_version;
    let queue = OfflineQueue::load();

    rsx! {
        div { class: "flex flex-col gap-6",
            section {
                h2 { class: "mb-1 text-lg font-bold text-sheffield-dark", "Parent sign-in" }
                match session_state() {
                    Some(SessionState::Parent) => rsx! {
                        p { class: "mb-3 text-sm text-slate-600",
                            "This phone is signed in as a parent and can control the TV and edit the family's settings."
                        }
                        button {
                            class: "rounded-2xl border-l-4 border-sheffield-accent bg-white px-4 py-3 text-base font-semibold text-slate-800 shadow ring-1 ring-slate-200",
                            onclick: move |_| async move {
                                session::logout().await;
                                session_state.set(Some(SessionState::SignedOut));
                                status.set(Some("Signed out on this phone.".into()));
                            },
                            "Sign out"
                        }
                    },
                    Some(SessionState::FirstRun) => rsx! {
                        FirstRunForm { session_state, status }
                    },
                    _ => rsx! {
                        SignInForm { session_state, status }
                    },
                }
                if let Some(message) = status() {
                    p { class: "mt-2 text-sm font-semibold text-slate-600", "{message}" }
                }
            }

            section {
                h2 { class: "mb-1 text-lg font-bold text-sheffield-dark", "Saved while offline" }
                if queue.is_empty() {
                    p { class: "text-sm text-slate-600",
                        "Nothing waiting — every change has reached the hub."
                    }
                } else {
                    p { class: "mb-3 text-sm text-slate-600",
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
                                span { class: "text-slate-600", " · for {entry.date}" }
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
                p { class: "text-sm text-slate-600",
                    "Add the hub to your home screen to use it like an app. On Android changes are sent again as soon as the phone reconnects; on iPhone they are sent the next time you open the app. Either way nothing is lost for two days, and nothing is applied twice."
                }
            }
        }
    }
}

/// `GET /api/session` answered `404`: this hub has never had a parent PIN.
///
/// The setup code proves physical access to the hub PC — it is written to
/// `<data>\setup-code.txt` and printed in the log at boot, and never served
/// over the network — which is what stops any device on the home Wi-Fi from
/// claiming the family's first PIN before the parents do.
#[component]
fn FirstRunForm(session_state: SessionSignal, status: Signal<Option<String>>) -> Element {
    let mut session_state = session_state;
    let mut status = status;
    let mut setup_code = use_signal(String::new);
    let mut pin = use_signal(String::new);
    let mut confirm = use_signal(String::new);

    rsx! {
        p { class: "mb-3 text-sm text-slate-600",
            "First-time setup. Type the setup code from %ProgramData%\\FamilyHub\\setup-code.txt on the hub PC (or its log), then choose a six-digit PIN."
        }
        form {
            class: "flex flex-col gap-2",
            onsubmit: move |event| async move {
                // Dioxus 0.7 submits the form by default (D7' break #7).
                event.prevent_default();
                if !is_six_digits(&setup_code()) {
                    status.set(Some(format!("The setup code is {SETUP_CODE_LENGTH} digits.")));
                    return;
                }
                if !is_six_digits(&pin()) {
                    status.set(Some(format!("The PIN is {PIN_LENGTH} digits.")));
                    return;
                }
                if pin() != confirm() {
                    status.set(Some("Those two PINs are different.".into()));
                    return;
                }
                if session::setup(&setup_code(), &pin()).await {
                    setup_code.set(String::new());
                    pin.set(String::new());
                    confirm.set(String::new());
                    session_state.set(Some(SessionState::Parent));
                    status.set(Some("PIN set — this phone is signed in.".into()));
                } else {
                    setup_code.set(String::new());
                    pin.set(String::new());
                    confirm.set(String::new());
                    status
                        .set(
                            Some(
                                "That setup code was not accepted. Check the code on the hub and try again."
                                    .into(),
                            ),
                        );
                }
            },
            input {
                class: "min-w-0 rounded-2xl bg-white px-4 py-3 text-lg tracking-[0.4em] shadow ring-1 ring-slate-200",
                r#type: "text",
                inputmode: "numeric",
                autocomplete: "off",
                maxlength: SETUP_CODE_LENGTH as i64,
                aria_label: "Setup code",
                placeholder: "Setup code",
                value: "{setup_code}",
                oninput: move |event| setup_code.set(event.value()),
            }
            input {
                class: "min-w-0 rounded-2xl bg-white px-4 py-3 text-lg tracking-[0.4em] shadow ring-1 ring-slate-200",
                r#type: "password",
                inputmode: "numeric",
                autocomplete: "off",
                maxlength: PIN_LENGTH as i64,
                aria_label: "New parent PIN",
                placeholder: "New PIN",
                value: "{pin}",
                oninput: move |event| pin.set(event.value()),
            }
            input {
                class: "min-w-0 rounded-2xl bg-white px-4 py-3 text-lg tracking-[0.4em] shadow ring-1 ring-slate-200",
                r#type: "password",
                inputmode: "numeric",
                autocomplete: "off",
                maxlength: PIN_LENGTH as i64,
                aria_label: "Confirm parent PIN",
                placeholder: "Confirm PIN",
                value: "{confirm}",
                oninput: move |event| confirm.set(event.value()),
            }
            button {
                class: "rounded-2xl bg-sheffield-dark px-5 py-3 text-base font-semibold text-white shadow",
                r#type: "submit",
                "Set the PIN"
            }
        }
    }
}

/// A PIN exists and this phone does not hold a session — or the probe has
/// not answered yet, in which case offering sign-in is the honest default
/// (the shell re-renders the moment `/api/session` lands).
#[component]
fn SignInForm(session_state: SessionSignal, status: Signal<Option<String>>) -> Element {
    let mut session_state = session_state;
    let mut status = status;
    let mut pin = use_signal(String::new);

    rsx! {
        p { class: "mb-3 text-sm text-slate-600",
            "Enter the six-digit parent PIN. The hub checks it, never this phone."
        }
        form {
            class: "flex gap-2",
            onsubmit: move |event| async move {
                // Dioxus 0.7 submits the form by default (D7' break #7).
                event.prevent_default();
                if !is_six_digits(&pin()) {
                    status.set(Some(format!("The PIN is {PIN_LENGTH} digits.")));
                    return;
                }
                let ok = session::login(&pin()).await;
                pin.set(String::new());
                if ok {
                    session_state.set(Some(SessionState::Parent));
                    status.set(Some("Signed in.".into()));
                } else {
                    status.set(Some("That PIN was not accepted. Try again.".into()));
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
}

/// Six ASCII digits — the shape of both the PIN (§P5.5 default 9) and the
/// first-run setup code. Checked here only to keep a doomed round trip off a
/// phone's radio; the hub checks it again and is the only authority.
fn is_six_digits(value: &str) -> bool {
    value.len() == PIN_LENGTH && value.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Render the tab with the session context pinned to one state, the way
    /// `MobileShell` provides it, and hand back the markup.
    ///
    /// SSR is enough here — the branch is chosen by the context signal, not
    /// by anything a browser does — and it is the same renderer
    /// `tests/tv_tests.rs` asserts the kiosk's focus order with.
    #[cfg(feature = "server")]
    fn render_with(state: SessionState) -> String {
        #[component]
        fn Harness(state: SessionState) -> Element {
            use_context_provider(|| Signal::new(Some(state)));
            rsx! {
                MobileSettings { queue_version: 0u64, on_retry: move |()| {} }
            }
        }

        dioxus::ssr::render_element(rsx! {
            Harness { state }
        })
    }

    /// **Q2-02**: the finding was that a fresh hub had no first-run form on
    /// any surface, so `docs/OWNER_CHECKLIST.md` step 4 could not be
    /// completed and no parent session could ever be obtained. `FirstRun`
    /// must therefore offer the setup form — and only it.
    #[cfg(feature = "server")]
    #[test]
    fn the_first_run_state_renders_the_setup_form() {
        let html = render_with(SessionState::FirstRun);
        assert!(
            html.contains("First-time setup"),
            "the FirstRun branch must offer the setup form, got: {html}"
        );
        assert!(
            html.contains("Setup code"),
            "the setup form needs a labelled setup-code field, got: {html}"
        );
        assert!(
            !html.contains("Sign out"),
            "a hub with no PIN cannot be signed out of"
        );
    }

    #[cfg(feature = "server")]
    #[test]
    fn the_parent_state_renders_sign_out() {
        let html = render_with(SessionState::Parent);
        assert!(
            html.contains("Sign out"),
            "a signed-in parent must be able to sign out, got: {html}"
        );
        assert!(
            !html.contains("First-time setup"),
            "a hub with a PIN must never offer first-run setup again"
        );
    }

    /// The third branch, and the pre-probe default: offer the PIN.
    #[cfg(feature = "server")]
    #[test]
    fn the_signed_out_state_renders_the_pin_form() {
        for state in [SessionState::SignedOut, SessionState::FirstRun] {
            let html = render_with(state);
            let has_pin_form = html.contains("Enter the six-digit parent PIN");
            assert_eq!(
                has_pin_form,
                state == SessionState::SignedOut,
                "{state:?} rendered the wrong form: {html}"
            );
        }
    }

    #[test]
    fn only_six_ascii_digits_are_worth_sending() {
        assert!(is_six_digits("482913"));
        assert!(!is_six_digits("48291"));
        assert!(!is_six_digits("4829134"));
        assert!(!is_six_digits("48291a"));
        assert!(!is_six_digits(""));
    }
}
