//! The **TV Remote** tab — the phone driving the kiosk (PLAN v2 **D1**).
//!
//! Both messages this sends are the ones §P2c gates on authorisation:
//! `SetView` and `SetActiveProfile` are broadcast by the server *only* after
//! it has checked a valid parent session token, or that the sender is asking
//! for its own profile (R-23b). The token is passed as
//! `auth: Option<SessionToken>` exactly as the protocol defines — see
//! `session.rs` and `docs/HANDOFF.md` H-19 for why it is a bearer value on
//! this surface for now.
//!
//! The phone never mutates the TV's state locally and never assumes the send
//! worked: the kiosk applies the change when the *server* re-emits it, and
//! this phone (like every other client) learns about it from
//! `RealtimeBus::requested_view`.

use dioxus::prelude::*;

use crate::client::components::mobile::session;
use crate::client::realtime::use_realtime;
use crate::shared::types::{profile_name, ClientMessage, MaximizedView, FAMILY_PROFILE_COUNT};

/// The four things the remote can put on the screen. `None` is "restore the
/// three-panel dashboard", which is the TV's Backspace key (D8).
const VIEWS: [(MaximizedView, &str); 4] = [
    (MaximizedView::None, "Dashboard"),
    (MaximizedView::Routine, "Routine"),
    (MaximizedView::Calendar, "Calendar"),
    (MaximizedView::Whiteboard, "Whiteboard"),
];

#[component]
pub fn TvRemote() -> Element {
    let (bus, sender) = use_realtime();
    let signed_in = session::is_parent();
    let connected = (bus.connected)();

    rsx! {
        div { class: "flex flex-col gap-6",
            section {
                h2 { class: "mb-1 text-lg font-bold text-sheffield-dark", "Show on the TV" }
                p { class: "mb-3 text-sm text-slate-500",
                    "Changes what the whole family sees on the television."
                }
                div { class: "grid grid-cols-2 gap-3",
                    for (view , label) in VIEWS {
                        button {
                            key: "{label}",
                            class: "rounded-2xl bg-white px-4 py-4 text-base font-semibold text-sheffield-dark shadow ring-1 ring-slate-200 active:bg-sheffield-light/30 disabled:opacity-40",
                            disabled: !connected,
                            onclick: move |_| {
                                sender
                                    .send(ClientMessage::SetView {
                                        view,
                                        auth: session::token(),
                                    });
                            },
                            "{label}"
                        }
                    }
                }
            }

            section {
                h2 { class: "mb-1 text-lg font-bold text-sheffield-dark", "Whose routine" }
                p { class: "mb-3 text-sm text-slate-500",
                    "Switches the profile shown on the television."
                }
                div { class: "grid grid-cols-2 gap-3",
                    for user_id in 1..=FAMILY_PROFILE_COUNT {
                        button {
                            key: "{user_id}",
                            class: "rounded-2xl bg-white px-4 py-4 text-base font-semibold text-sheffield-dark shadow ring-1 ring-slate-200 active:bg-sheffield-light/30 disabled:opacity-40",
                            disabled: !connected,
                            onclick: move |_| {
                                sender
                                    .send(ClientMessage::SetActiveProfile {
                                        user_id: i64::from(user_id),
                                        auth: session::token(),
                                    });
                            },
                            "{profile_name(user_id)}"
                        }
                    }
                }
            }

            if !connected {
                p { class: "rounded-2xl bg-sheffield-accent/10 p-4 text-sm font-semibold text-sheffield-accent",
                    "Not connected to the hub — the remote will work again as soon as the phone is back on the home Wi-Fi."
                }
            } else if !signed_in {
                p { class: "rounded-2xl bg-sheffield-sun/20 p-4 text-sm font-semibold text-slate-700",
                    "Sign in with the parent PIN under Settings to control the TV. Without it the hub ignores these buttons."
                }
            }
        }
    }
}
