//! Central axum router construction (PLAN v2 T0.6, extended by T1.3).
//!
//! [`build_router`] is the single place production HTTP routes are
//! registered. `src/main.rs` is reduced to booting the tokio runtime and
//! delegating to [`run`] so it stays under 25 lines and is **frozen**
//! thereafter (`docs/reviews/PURPLE_TEAM.md` §P4). Later tasks extend this
//! router in their own waves rather than duplicating it: T1.3 added the
//! HTTPS listener and a real `/ca.crt`; T2.5 added the multipart photo-upload
//! route (`POST /api/upload_photo`, its own raised `DefaultBodyLimit` — R-08)
//! and the `nosniff`/`attachment` headers on `/uploads` (R-23c); **T2.7**
//! (reconciled with T2.5, `docs/HANDOFF.md` "T2.7 → T2.5, restated") adds the
//! matching `POST /api/upload_screensaver_image` multipart route — same
//! raised limit, same shared allowlist/re-encode pipeline
//! (`api::photos::sniff_downscale_reencode`) — and the same
//! `nosniff`/`attachment` headers on `/assets/screensaver`, in place of the
//! self-contained base64 pipeline its first pass shipped before T2.5 existed.
//! `router.rs` is not a file T2.7 otherwise owns (§P4), but a raw axum route
//! can only be registered here; this task's own brief ("reuse T2.5's
//! multipart pipeline") is the reconciliation this edit performs.
//!
//! **Split origins (PLAN v2 D3′).** There is one router and two listeners:
//!
//! ```text
//! :8080  HTTP   serves everything the TV needs — /tv, /ws, /assets,
//!               /uploads, /ca.crt, /health — and 308s only the phone
//!               surface (/m*, /manifest.webmanifest, /sw.js) to HTTPS.
//! :8443  HTTPS  serves everything, including /m and wss.
//! ```
//!
//! The TV never needs a secure context (no service worker, no install
//! prompt, no camera), which is exactly why it can stay on plain HTTP and
//! never has to trust a private CA — the finding that dissolved the v1
//! kiosk/TLS conflict. The phone surface must be secure or it gets none of
//! those things, hence the one-way upgrade.
//!
//! [`build_router`] is the shared router and answers `/m` with 200;
//! [`build_http_router`] is that router behind the upgrade layer and is what
//! the :8080 listener actually serves. Keeping them separate is what lets
//! the same handler tree serve both origins.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{DefaultBodyLimit, Request};
use axum::http::{header, HeaderValue, StatusCode, Uri};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::Router;
use dioxus::prelude::*;
use tower_http::services::ServeDir;

use crate::client::app::App;
use crate::client::components::mobile::pwa;
use crate::client::components::qr::phone_join_url;
use crate::server::api::realtime;
use crate::server::config::FamilyHubConfig;
use crate::server::pki::{CertProvider, CertSource, SelfSignedCa};
use crate::server::tls::{install_crypto_provider, TlsListener};

/// How often the renewal task re-checks the leaf. Six hours is far finer
/// than the 30-day window it guards, and coarse enough to be invisible on a
/// machine that runs for months.
const RENEWAL_CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(6 * 60 * 60);

/// The raised body limit for `POST /api/upload_photo` (T2.5, PURPLE §P3
/// T2.5 / R-08) **and** `POST /api/upload_screensaver_image` (T2.7, reusing
/// the same limit as part of reusing the whole pipeline). Applied to those
/// two routes only, via [`axum::routing::MethodRouter::layer`] — every other
/// route (including the JSON `#[server]` fn endpoints Dioxus mounts under
/// `/api`) keeps axum's global 2 MB default, which is exactly what PURPLE
/// §P3 T2.5(b) proves by demonstrating a 413 on an unraised route with the
/// same handler.
const UPLOAD_PHOTO_BODY_LIMIT_BYTES: usize = 25 * 1024 * 1024;

/// Build the production axum router for `config`. Pure and synchronous so
/// tests can construct it directly against a throwaway [`FamilyHubConfig`]
/// without booting a database pool or a listener.
///
/// This is the **shared** router: it serves both origins and answers every
/// route, `/m` included, with real content. The HTTP-only upgrade rule
/// lives in [`build_http_router`] instead, so that a route's behaviour on
/// the HTTPS origin is never coupled to how the HTTP origin treats it.
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
    let pki_dir = config.pki_dir();
    let health_config = config.clone();

    Router::new()
        .route("/", get(redirect_root_to_tv))
        // T2.2 filled these three in. T0.6 registered the first two as stubs
        // and its own doc comment reserved their bodies for this task
        // ("T2.2 replaces the body with the real manifest … but keeps this
        // route and its content type"); `/icons/{file}` is new, and serves
        // the T0.7 icon set from the binary at a hash-free URL so the
        // manifest can reference it and an install can never fail on a
        // missing file. Everything they serve lives in
        // `client::components::mobile::pwa` — see `docs/HANDOFF.md`
        // "T2.2 → Boss / T2.5".
        .route(pwa::MANIFEST_PATH, get(pwa::handlers::manifest))
        .route(pwa::SERVICE_WORKER_PATH, get(pwa::handlers::service_worker))
        .route("/icons/{file}", get(pwa::handlers::icon))
        .route(
            "/ca.crt",
            get(move || {
                let pki_dir = pki_dir.clone();
                async move { ca_cert(pki_dir).await }
            }),
        )
        .route(
            "/health",
            get(move || {
                let health_config = health_config.clone();
                async move { crate::server::health::health_handler(health_config).await }
            }),
        )
        .route("/ws", get(realtime::ws_handler))
        .route(
            "/api/upload_photo",
            post(crate::server::api::photos::upload_photo_handler)
                .layer(DefaultBodyLimit::max(UPLOAD_PHOTO_BODY_LIMIT_BYTES)),
        )
        .route(
            "/api/upload_screensaver_image",
            post(crate::server::api::screensaver::upload_screensaver_image_handler)
                .layer(DefaultBodyLimit::max(UPLOAD_PHOTO_BODY_LIMIT_BYTES)),
        )
        .nest_service("/uploads", uploads_router(config))
        .nest_service("/assets/screensaver", screensaver_router(config))
        .serve_dioxus_application(ServeConfig::new(), App)
}

/// `/uploads` as its own tiny `Router<()>`, so [`uploads_security_headers`]
/// wraps **only** this service rather than the whole tree.
///
/// `nest_service` takes anything implementing `tower::Service<Request>` — a
/// fully-built `Router<()>` qualifies directly (that is how axum routers
/// nest at all) — so passing one here, instead of `.merge`ing it into
/// [`build_router`]'s own router, never touches that outer router's state
/// type. That distinction matters: `serve_dioxus_application` is only
/// implemented for `Router<FullstackState>` (`dioxus_server::server`), and
/// Rust infers the whole `build_router` chain's state as `FullstackState`
/// *backward* from that one terminal call, since nothing earlier in the
/// chain fixes it to anything else. A `.merge()` of a concretely-typed
/// `Router<()>` would fix the merged router's state to `()` too and break
/// that inference — `nest_service` does not have this problem because a
/// nested service's own type is independent of its parent's state.
fn uploads_router(config: &FamilyHubConfig) -> Router {
    // `fallback_service`, not `.nest_service("/uploads", ...)` — this router
    // is itself mounted at `/uploads` by `build_router`'s own
    // `.nest_service("/uploads", uploads_router(config))`, so nesting the
    // same prefix again here would require `/uploads/uploads/<file>`.
    Router::new()
        .fallback_service(ServeDir::new(config.upload_dir()))
        .layer(axum::middleware::from_fn(uploads_security_headers))
}

/// `/assets/screensaver` as its own tiny `Router<()>`, mirroring
/// [`uploads_router`] exactly (T2.7 reusing T2.5's pipeline, including its
/// headers) — every screensaver photo is this server's own re-encoded JPEG
/// (T0.7's committed placeholders included, R-31/G8), served with the same
/// `nosniff`/`attachment` belt-and-braces [`uploads_security_headers`]
/// already gives `/uploads`.
fn screensaver_router(config: &FamilyHubConfig) -> Router {
    Router::new()
        .fallback_service(ServeDir::new(config.screensaver_dir()))
        .layer(axum::middleware::from_fn(uploads_security_headers))
}

/// `X-Content-Type-Options: nosniff` + `Content-Disposition: attachment` on
/// every `/uploads` **and** `/assets/screensaver` response (PURPLE §P3
/// T2.5(e), R-23c, reused by T2.7). `upload_photo_handler` and
/// `upload_screensaver_image_handler` both re-encode every stored image
/// through the `image` crate, which strips anything that isn't valid
/// JPEG/PNG/WebP pixel data — these two headers are the belt to that
/// re-encode's braces: even if a byte sequence somehow survived re-encoding
/// *and* got served with a browser-guessed `Content-Type`, `nosniff` stops
/// the browser from sniffing its own MIME type and `attachment` refuses to
/// render it inline (R-23: "MIME→stored XSS"). Neither header affects an
/// `<img>` tag's own inline rendering (only top-level navigation/"Save
/// as"), which is how both the TV screensaver and the phone's task photos
/// already display these files.
async fn uploads_security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment"),
    );
    response
}

/// [`build_router`] wrapped in the plain-HTTP origin's one rule: the phone
/// surface is 308'd to the HTTPS origin (PLAN v2 D3′, PURPLE_TEAM.md §P5.5
/// default 6 — "8080 serves the TV in full; it 308s only `/m*`,
/// `/manifest.webmanifest` and `/sw.js`"). Everything else — `/tv`, `/ws`,
/// `/assets`, `/uploads`, `/ca.crt`, `/health` — is served, not redirected,
/// because the TV must keep working even if TLS is broken.
pub fn build_http_router(config: &FamilyHubConfig) -> Router {
    let tls_port = config.tls_addr.port();
    build_router(config).layer(axum::middleware::from_fn(
        move |request: Request, next: Next| async move {
            match upgrade_target(request.uri(), request.headers(), tls_port) {
                Some(location) => Redirect::permanent(&location).into_response(),
                None => next.run(request).await,
            }
        },
    ))
}

/// Does this request belong to the phone surface, and if so where does it
/// go on the HTTPS origin?
///
/// The host is taken from the request's `Host` header, so a phone that
/// typed the reserved IP is upgraded to that same IP and a phone that typed
/// `familyhub.local` stays on the name — an absolute redirect to a
/// hard-coded address would break one of the two.
fn upgrade_target(uri: &Uri, headers: &header::HeaderMap, tls_port: u16) -> Option<String> {
    let path = uri.path();
    let is_phone_surface = path == "/m"
        || path.starts_with("/m/")
        || path == "/mobile"
        || path.starts_with("/mobile/")
        || path == "/manifest.webmanifest"
        || path == "/sw.js";
    if !is_phone_surface {
        return None;
    }

    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(':').next().unwrap_or(value))
        .filter(|host| !host.is_empty())
        .unwrap_or("localhost");

    let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
    Some(format!("https://{host}:{tls_port}{path}{query}"))
}

/// `/` is not a page of its own (D3′): the TV kiosk lives at `/tv`, the phone
/// PWA at `/m`. A permanent redirect keeps old bookmarks/QR codes working.
async fn redirect_root_to_tv() -> Redirect {
    Redirect::permanent("/tv")
}

/// The local CA certificate the owner installs on each phone (PLAN v2 D3′,
/// Appendix A A6). Served on **both** origins, and specifically on the
/// plain-HTTP one: a phone that does not yet trust the CA cannot fetch the
/// CA over a connection secured by it.
///
/// Only the CA *certificate* is ever served. The CA private key stays in
/// `<data>\pki\ca.key` with a narrowed ACL and is excluded from backups
/// (T1.6).
async fn ca_cert(pki_dir: PathBuf) -> Response {
    match pki_for(&pki_dir) {
        Ok(pki) => (
            [
                (header::CONTENT_TYPE, "application/x-x509-ca-cert"),
                (
                    header::CONTENT_DISPOSITION,
                    "attachment; filename=\"familyhub-ca.crt\"",
                ),
            ],
            pki.ca_pem(),
        )
            .into_response(),
        Err(err) => {
            tracing::error!(%err, "could not load the local CA to serve /ca.crt");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "the local certificate authority is unavailable\n",
            )
                .into_response()
        }
    }
}

/// Process-wide cache of opened PKI directories.
///
/// [`build_router`] takes a `&FamilyHubConfig`, not a certificate provider,
/// because its signature is load-bearing for T0.3's and T0.6's protected
/// acceptance tests. Keying the provider by directory keeps `/ca.crt`
/// answering with the *same* CA the HTTPS listener is using (both resolve
/// `config.pki_dir()`), while still letting each test binary point at its
/// own throwaway directory.
///
/// `pub(crate)` rather than private: `server::health::health_handler` (T1.7)
/// resolves the exact same cached `Arc<SelfSignedCa>` for `/health`'s
/// `cert_not_after`/`days_to_expiry` fields, so they can never drift from the
/// certificate the HTTPS listener is actually serving (`docs/HANDOFF.md`
/// "H-14. For T1.7 — `/health` cert fields").
pub(crate) fn pki_for(
    dir: &std::path::Path,
) -> Result<Arc<SelfSignedCa>, crate::server::pki::PkiError> {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::sync::OnceLock;

    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<SelfSignedCa>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());

    if let Some(existing) = cache.get(dir) {
        return Ok(existing.clone());
    }
    let pki = Arc::new(SelfSignedCa::open(dir)?);
    cache.insert(dir.to_path_buf(), pki.clone());
    Ok(pki)
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
/// start the calendar poller, and serve the router on **both** origins
/// forever (PLAN v2 D3′). Extracted out of `main.rs` (T0.6) so `main.rs`
/// stays under 25 lines and never defines a route itself.
pub async fn run(config: FamilyHubConfig) {
    // First statement of the server's real entrypoint, before anything can
    // touch rustls (`reqwest` also links it, so a second provider is on the
    // table and `get_default()` would panic rather than choose).
    // PURPLE_TEAM.md §P5.4 words this as "the first line of `main`";
    // `main.rs` is frozen by §P4's ownership table, and `run` is the first
    // thing it calls, so this is that line — see `docs/HANDOFF.md` H-7.
    install_crypto_provider();
    // T1.7: `/health`'s `uptime_seconds` measures from here — as close to the
    // real process start as `main.rs` being frozen (T0.6) allows T1.7 to get.
    crate::server::health::mark_started();

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
    // T1.2 H-7: start the DST-safe midnight tick at boot rather than waiting
    // for the first WebSocket upgrade to self-start it.
    crate::server::api::realtime::ensure_background_tasks();
    // T1.6 H-15: register the nightly backup / retention sweep on the
    // day-rolled hook. Exactly once — each call adds another hook closure.
    crate::server::backup::register_nightly_hooks();
    // T2.7 (reconciled with T2.5): start the optional screensaver schedule
    // loop at boot rather than waiting for the first screensaver server fn
    // or upload to self-start it (same H-7 precedent as the two calls above;
    // closes the "production wiring gap" the first pass of T2.7 logged).
    // `OnceLock`-guarded, so this and any later self-start call are both
    // harmless.
    crate::server::api::screensaver::ensure_background_tasks();

    // Certificate source. Only `SelfSignedCa` exists in this wave; an
    // unrecognised `certs.mode` fails here, loudly, rather than at the
    // first renewal months later.
    let CertSource::SelfSignedCa = CertSource::from_mode(None).expect("supported certs.mode");
    let pki = pki_for(&config.pki_dir()).expect("failed to open the local certificate authority");
    match pki.renew_if_due() {
        Ok(true) => tracing::info!("re-issued the server leaf certificate at startup"),
        Ok(false) => {}
        Err(err) => tracing::error!(%err, "could not renew the server leaf certificate"),
    }

    let http_router = build_http_router(&config);
    let https_router = build_router(&config);

    // `dioxus_cli_config`'s bare-`IP`/`PORT` address helper is removed from
    // the release path per PLAN v2 T0.5 / PURPLE_TEAM.md finding 10 (D7′);
    // the bind address always comes from `FamilyHubConfig`
    // (`FAMILY_HUB_ADDR`, default `0.0.0.0:8080`).
    let http_listener = tokio::net::TcpListener::bind(config.http_addr)
        .await
        .expect("failed to bind the HTTP server address");
    let http_addr = http_listener.local_addr().unwrap_or(config.http_addr);

    let tls = TlsListener::bind(config.tls_addr, pki.as_ref())
        .await
        .expect("failed to bind the HTTPS server address");
    let tls_addr = tls.local_addr;
    let resolver = tls.resolver();

    // mDNS is a convenience layered on top of the IP the QR encodes, so it
    // is registered after both listeners are actually bound and it never
    // fails the boot.
    crate::server::mdns::register_best_effort(http_addr.port(), tls_addr.port());
    log_join_urls(http_addr, tls_addr);

    // Renewal loop: re-issue at 30 days remaining (or when the host's
    // addresses change) and push the new leaf into the live resolver, so a
    // display that is never power-cycled never serves an expired
    // certificate and never needs a restart to stop doing so.
    tokio::spawn({
        let pki = pki.clone();
        async move {
            loop {
                tokio::time::sleep(RENEWAL_CHECK_INTERVAL).await;
                match pki.renew_if_due() {
                    Ok(true) => match resolver.replace(&pki.current()) {
                        Ok(()) => tracing::info!("hot-reloaded a re-issued leaf certificate"),
                        Err(err) => tracing::error!(%err, "re-issued leaf could not be loaded"),
                    },
                    Ok(false) => {}
                    Err(err) => tracing::error!(%err, "certificate renewal check failed"),
                }
            }
        }
    });

    tokio::select! {
        result = axum::serve(http_listener, http_router.into_make_service()) => {
            result.expect("http server error");
        }
        () = tls.serve(https_router) => {}
    }
}

/// Log both surfaces' URLs at startup so the owner can read the kiosk URL
/// and the phone URL straight out of the service log (D9's "all paths
/// logged at startup", extended to the two origins).
fn log_join_urls(http_addr: SocketAddr, tls_addr: SocketAddr) {
    let host = crate::server::pki::primary_ipv4_address()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "127.0.0.1".to_string());

    tracing::info!(
        kiosk_url = %format!("http://{host}:{}/tv", http_addr.port()),
        phone_url = %phone_join_url(&host, tls_addr.port()),
        ca_url = %format!("http://{host}:{}/ca.crt", http_addr.port()),
        "hub is serving"
    );
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

    fn headers_with_host(host: &str) -> header::HeaderMap {
        let mut headers = header::HeaderMap::new();
        headers.insert(header::HOST, host.parse().expect("valid host header"));
        headers
    }

    #[test]
    fn only_the_phone_surface_is_upgraded_to_https() {
        let headers = headers_with_host("10.0.0.42:8080");

        for path in ["/m", "/m/settings", "/manifest.webmanifest", "/sw.js"] {
            let uri: Uri = path.parse().expect("valid uri");
            assert_eq!(
                upgrade_target(&uri, &headers, 8443).as_deref(),
                Some(format!("https://10.0.0.42:8443{path}").as_str()),
                "{path} is phone surface and must be upgraded"
            );
        }

        // Everything the TV needs stays on plain HTTP: the kiosk must keep
        // working even when TLS is broken (that is the whole point of D3').
        for path in ["/tv", "/ws", "/health", "/ca.crt", "/uploads/x.jpg", "/"] {
            let uri: Uri = path.parse().expect("valid uri");
            assert_eq!(
                upgrade_target(&uri, &headers, 8443),
                None,
                "{path} must be served on the HTTP origin, not redirected"
            );
        }
    }

    #[test]
    fn the_upgrade_keeps_the_requested_host_and_query() {
        let uri: Uri = "/m?tab=calendar".parse().expect("valid uri");
        assert_eq!(
            upgrade_target(&uri, &headers_with_host("familyhub.local"), 8443).as_deref(),
            Some("https://familyhub.local:8443/m?tab=calendar")
        );
    }

    #[test]
    fn pki_for_returns_the_same_authority_for_the_same_directory() {
        let dir =
            std::env::temp_dir().join(format!("familyhub-router-pki-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let first = pki_for(&dir).expect("opens");
        let second = pki_for(&dir).expect("opens");
        assert!(
            Arc::ptr_eq(&first, &second),
            "/ca.crt and the HTTPS listener must share one CA per data directory"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
