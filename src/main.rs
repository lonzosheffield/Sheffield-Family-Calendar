use family_calendar::client::app::App;

#[cfg(not(feature = "server"))]
fn main() {
    dioxus::launch(App);
}

#[cfg(feature = "server")]
fn main() {
    use axum::routing::get;
    use dioxus::prelude::*;
    use family_calendar::server::{api::realtime, calendar, config::FamilyHubConfig, db};
    use tower_http::services::ServeDir;

    tokio::runtime::Runtime::new()
        .expect("failed to start tokio runtime")
        .block_on(async move {
            // T0.5: every path this process touches (DB, uploads,
            // screensaver, PKI, logs) is resolved once here, absolutely,
            // from `FamilyHubConfig` — never relative to the current working
            // directory (G23/R-14) — and logged before anything else binds
            // or opens a file.
            let config = FamilyHubConfig::load();
            config
                .ensure_dirs_and_log()
                .expect("failed to prepare the data directory");
            // Downstream server-fn bodies (`db::pool`, `api::create_photo_task`,
            // `api::list_screensaver_images`) each resolve `FamilyHubConfig`
            // independently; pinning `DATABASE_URL` here keeps every one of
            // them, and this process's own `db::pool()` call below, pointed
            // at the exact same absolute file.
            std::env::set_var("DATABASE_URL", config.database_url());

            db::pool().await.expect("failed to open the database");
            calendar::spawn_polling_task();

            // Dioxus 0.7 removed the 0.6 serve-config builder type;
            // `ServeConfig::new()` picks up the `dx`-generated
            // `public/index.html` next to the executable, and
            // `serve_dioxus_application` now hands back a fully-stated
            // `Router<()>` instead of `Self`.
            let router = axum::Router::new()
                .route("/ws", get(realtime::ws_handler))
                .nest_service("/uploads", ServeDir::new(config.upload_dir()))
                .serve_dioxus_application(ServeConfig::new(), App);

            // `dioxus_cli_config`'s bare-`IP`/`PORT` address helper is
            // removed from the release path per PLAN v2 T0.5 /
            // PURPLE_TEAM.md finding 10 (D7′); the bind address now always
            // comes from `FamilyHubConfig` (`FAMILY_HUB_ADDR`, default
            // `0.0.0.0:8080`).
            let listener = tokio::net::TcpListener::bind(config.http_addr)
                .await
                .expect("failed to bind server address");

            axum::serve(listener, router.into_make_service())
                .await
                .expect("server error");
        });
}
