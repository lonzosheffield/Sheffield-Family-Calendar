//! The Fire TV kiosk — the 10-foot UI (PLAN v2 **D8**, task **T2.1**).
//!
//! `/tv` is the hub's primary display: a 1920 × 1080 television across the
//! room, driven by a D-pad remote with seven usable buttons and no pointer.
//! Everything in this directory exists to make one sentence true —
//! **a child completes their entire morning routine on the television, with
//! the remote, alone** (R-12 / D1) — and to make that provable on a Windows
//! PC with no television attached.
//!
//! | Module | What it owns |
//! | --- | --- |
//! | [`keymap`] | the seven remote keys, the aliases Fire OS and Silk use, and the `?keys=1` log. **No `Escape`** (R-11). |
//! | [`model`] | panels, zones, focus identities, and [`model::focus_order`] — the single definition of what is focusable and in what order |
//! | [`nav`] | [`nav::on_key`], the pure transition function the whole remote runs through |
//! | [`style`] | the focus ring, the four-size type scale (all ≥ 28 px, headings ≥ 44 px) and the 5 % overscan class |
//! | [`palette`] | **T3.4** — the Sheffield palette as a contract: every ink/ground pair, its WCAG ratio computed in Rust, and the ink picker for the profile discs |
//! | [`staleness`] | the permanent "updated HH:MM" line and the red disconnected badge (T1.7's tracker, ported to wasm) |
//! | [`clock`] | the hub's own wall clock, polled — the badge's server pulse and the source of HH:MM |
//! | [`surface`] | [`surface::TvSurface`], a pure component that renders a [`model::TvModel`] |
//! | [`shell`] | [`shell::TvShell`], the live wiring: resources, realtime bus, key listener |
//! | [`fixture`] | the canonical kiosk the golden files are written against |
//!
//! # The shape of the acceptance contract
//!
//! PURPLE §P3 T2.1 asks for six things and each one lands on a seam here:
//!
//! * **(a)** the exact ordered list of focusable ids, against a golden file,
//!   every one wearing a focus ring → [`model::focus_order`] +
//!   [`style::focus_class`], asserted on rendered HTML in `tests/tv_tests.rs`.
//! * **(b), (c)** an injected `SetView` / `SetActiveProfile` changes what is
//!   rendered → [`model::TvModel::apply_server_message`].
//! * **(d)** a pure transition test per key → [`nav::on_key`].
//! * **(e)** every routine item within 12 presses → [`nav::presses_to_reach`],
//!   a breadth-first search over the real state machine.
//! * **(f)** typography and overscan against a committed allowlist →
//!   [`style`] plus a grep of the rendered markup.
//!
//! # The scope line (PURPLE §P5.5 default 35)
//!
//! Routine completion, profile switching, panel navigation, screensaver
//! dismissal and the join-QR overlay are fully operable from the remote.
//! Drawing, photo capture, calendar editing and all administration are
//! phone-only, and the television says so where a child might otherwise
//! wait for something to happen.

pub mod clock;
pub mod keymap;
pub mod model;
pub mod nav;
pub mod palette;
pub mod shell;
pub mod staleness;
pub mod style;
pub mod surface;

// Test data for the golden files. Compiled for `cargo test` and for the
// server build that runs it; never part of the wasm bundle the television
// downloads.
#[cfg(any(test, feature = "server"))]
pub mod fixture;

pub use shell::TvShell;
