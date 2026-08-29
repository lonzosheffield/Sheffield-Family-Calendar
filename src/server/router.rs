//! Central axum router construction (PLAN v2 T0.6).
//!
//! [`build_router`] is the single place production HTTP routes are
//! registered. `src/main.rs` is reduced to booting the tokio runtime and
//! delegating to [`run`] so it stays under 25 lines and is **frozen**
//! thereafter (`docs/reviews/PURPLE_TEAM.md` §P4). Later tasks extend this
//! router in their own waves rather than duplicating it: T1.3 adds the HTTPS
//! listener and a real `/ca.crt`, T2.5 adds the multipart photo-upload route
//! — both serialized after T0.6 per the file-ownership table.
//!
//! The four root routes below (`/manifest.webmanifest`, `/sw.js`, `/ca.crt`,
//! `/health`) are deliberately **stubs**: they exist and answer with the
//! right status/content-type so downstream tasks (T1.3, T1.7, T2.2) can wire
//! real behaviour behind an already-stable URL, per G6/G8's root-cause
//! (serving these from a hashed `/assets/` path breaks PWA `scope`).

use std::path::PathBuf;

use axum::http::header;
use axum::response::{IntoResponse, Redirect};
use axum::routing::get;
use axum::Router;
use dioxus::prelude::*;
use tower_http::services::ServeDir;

use crate::client::app::App;
use crate::server::api::realtime;
use crate::server::config::FamilyHubConfig;

/// Build the production axum router for `config`. Pure and synchronous so
/// tests can construct it directly against a throwaway [`FamilyHubConfig`]
/// without booting a database pool or a listener.
///
/// **`/mobile` vs `/m` (`docs/HANDOFF.md` H-2):** both routes exist and both
/// render the routine-only phone view; `/m` is the canonical short URL of
/// record for the split-origin deployment (PLAN v2 D3′) and the one T0.6's
/// own acceptance test exercises, but `/mobile` is a pre-existing route
/// with its own protected T0.3 acceptance assertion
/// (`tests/http_tests.rs::http_mobile_serves_routine_only_view` — H-2 notes
/// it "must not be weakened without a Boss commit to `PLAN.md`"). Since
/// nothing in the plan actually calls for dropping or redirecting it, T0.6
/// leaves it exactly as it already behaves rather than force a choice that
/// isn't its call to make; `Route::Mobile` in `client/app.rs` is unchanged.
pub fn build_router(config: &FamilyHubConfig) -> Router {
    Router::new()
        .route("/", get(redirect_root_to_tv))
        .route("/manifest.webmanifest", get(manifest_stub))
        .route("/sw.js", get(service_worker_stub))
        .route("/ca.crt", get(ca_cert_stub))
        .route("/health", get(health_stub))
        .route("/ws", get(realtime::ws_handler))
        .nest_service("/uploads", ServeDir::new(config.upload_dir()))
        .nest_service(
            "/assets/screensaver",
            ServeDir::new(config.screensaver_dir()),
        )
        .serve_dioxus_application(ServeConfig::new(), App)
}

/// `/` is not a page of its own (D3′): the TV kiosk lives at `/tv`, the phone
/// PWA at `/m`. A permanent redirect keeps old bookmarks/QR codes working.
async fn redirect_root_to_tv() -> Redirect {
    Redirect::permanent("/tv")
}

/// T0.6 stub. T2.2 replaces the body with the real manifest (icons from
/// T0.7, `scope: "/"`, `start_url: "/m"`) but keeps this route and its
/// `application/manifest+json` content type, which is what matters for PWA
/// installability (G6).
async fn manifest_stub() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/manifest+json")],
        r#"{"name":"Sheffield Family Hub","scope":"/","start_url":"/m"}"#,
    )
}

/// T0.6 stub. T2.2 replaces the body with the real service worker
/// (app-shell precache, network-first server fns, cache-first uploads) but
/// keeps this route and its `text/javascript` content type.
async fn service_worker_stub() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript")],
        "// T0.6 stub; T2.2 lands the real service worker here.\n",
    )
}

/// T0.6 stub. T1.3 replaces the body with the real `rcgen` self-signed CA
/// certificate (PEM, valid X.509) but keeps this route and its
/// `application/x-x509-ca-cert` content type.
async fn ca_cert_stub() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/x-x509-ca-cert")],
        "-- T0.6 stub; T1.3 issues the real self-signed CA certificate here. --\n",
    )
}

/// T0.6 stub. T1.7 replaces the body with the real health JSON (db
/// reachability, last poll, cert expiry, disk free, WS client count,
/// uptime, migration version) but keeps this route and its `application/json`
/// content type.
async fn health_stub() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/json")],
        r#"{"status":"stub"}"#,
    )
}

/// `dioxus_server::ServeConfig::new()` — which [`build_router`] calls via
/// `serve_dioxus_application` — hard-panics (`server.rs::serve_dir_cached`)
/// if the resolved public-assets directory (`DIOXUS_PUBLIC_PATH`, else
/// `<exe dir>/public`) doesn't exist. `dx build` always creates it, so the
/// release path built by CI is fine, but a bare `cargo run --features
/// server`, or this binary running as a Windows service (T3.1) from an
/// install that skipped `dx build`, would crash at startup on it
/// (`docs/HANDOFF.md` H-1). Create the directory up front so this can never
/// happen; harmless (and a no-op) when `dx build` already created it.
fn ensure_public_dir_exists() {
    let public_path = std::env::var("DIOXUS_PUBLIC_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf))
                .unwrap_or_else(|| PathBuf::from("."))
                .join("public")
        });
    if let Err(err) = std::fs::create_dir_all(&public_path) {
        tracing::warn!(
            path = %public_path.display(),
            %err,
            "failed to create the Dioxus public-assets directory"
        );
    }
}

/// Resolve `config`, prepare its data directory, open the database pool,
/// start the calendar poller, and serve [`build_router`] forever on
/// `config.http_addr`. Extracted out of `main.rs` (T0.6) so `main.rs` stays
/// under 25 lines and never defines a route itself.
pub async fn run(config: FamilyHubConfig) {
    // T0.5: every path this process touches (DB, uploads, screensaver, PKI,
    // logs) is resolved once here, absolutely, from `FamilyHubConfig` —
    // never relative to the current working directory (G23/R-14) — and
    // logged before anything else binds or opens a file.
    config
        .ensure_dirs_and_log()
        .expect("failed to prepare the data directory");
    ensure_public_dir_exists();
    // Downstream server-fn bodies (`db::pool`, `api::create_photo_task`,
    // `api::list_screensaver_images`) each resolve `FamilyHubConfig`
    // independently; pinning `DATABASE_URL` here keeps every one of them,
    // and this process's own `db::pool()` call below, pointed at the exact
    // same absolute file.
    std::env::set_var("DATABASE_URL", config.database_url());

    crate::server::db::pool()
        .await
        .expect("failed to open the database");
    crate::server::calendar::spawn_polling_task();

    let router = build_router(&config);

    // `dioxus_cli_config`'s bare-`IP`/`PORT` address helper is removed from
    // the release path per PLAN v2 T0.5 / PURPLE_TEAM.md finding 10 (D7′);
    // the bind address always comes from `FamilyHubConfig`
    // (`FAMILY_HUB_ADDR`, default `0.0.0.0:8080`).
    let listener = tokio::net::TcpListener::bind(config.http_addr)
        .await
        .expect("failed to bind server address");

    axum::serve(listener, router.into_make_service())
        .await
        .expect("server error");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `docs/HANDOFF.md` H-1: `ensure_public_dir_exists` must actually
    /// create the directory `dioxus_server::ServeConfig::new()` would
    /// otherwise panic looking for.
    #[test]
    fn ensure_public_dir_exists_creates_the_directory() {
        let dir = std::env::temp_dir().join(format!(
            "familyhub-router-public-dir-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::env::set_var("DIOXUS_PUBLIC_PATH", &dir);

        assert!(
            !dir.exists(),
            "test setup: {} must start absent",
            dir.display()
        );
        ensure_public_dir_exists();
        assert!(
            dir.is_dir(),
            "expected ensure_public_dir_exists() to create {}",
            dir.display()
        );

        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("DIOXUS_PUBLIC_PATH");
    }
}
