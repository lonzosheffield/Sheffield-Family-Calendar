//! `family-hub.exe` — the Windows service host / CLI (PLAN v2 D9, task
//! T3.1): `install|uninstall|start|stop|status|run|tv-probe`. Every
//! subcommand's real logic lives in `family_calendar::server::service`
//! (`docs/reviews/PURPLE_TEAM.md` §P4 gives that file to this task); this
//! binary is deliberately thin so it stays a stable, un-tested-directly
//! entry point — everything worth unit testing already is, in
//! `server::service::tests` against mocked collaborators.
//!
//! Two invocation shapes:
//!
//! * An explicit subcommand (`family-hub.exe run`, `install`, ...) —
//!   handled directly by [`family_calendar::server::service::dispatch`].
//! * No arguments at all — the shape the Service Control Manager uses when
//!   it starts an installed service. [`service::try_run_as_service`]
//!   attempts the SCM handshake; if it is rejected (this is not actually an
//!   SCM-launched process — a developer just ran the exe with no args from
//!   a console), usage is printed instead of hanging.
//!
//! `family-hub.exe` is a *separate binary target* from the Dioxus fullstack
//! app's own executable (`src/main.rs`, frozen since T0.6 — §P4) so this
//! file, and everything it calls into, can grow without ever touching that
//! frozen entry point.

use family_calendar::server::service;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.is_empty() {
        // Either "started by the SCM with no launch arguments" (the normal
        // shape — this call blocks for the service's whole lifetime and
        // only returns once the SCM has stopped it) or "run with no
        // subcommand from a console" (this call returns `false`
        // immediately because there is no SCM to hand off to).
        if service::try_run_as_service() {
            return;
        }
        eprintln!(
            "usage: family-hub.exe <install|uninstall|start|stop|status|run|tv-probe|import-curriculum>"
        );
        std::process::exit(2);
    }

    std::process::exit(service::dispatch(&args));
}
