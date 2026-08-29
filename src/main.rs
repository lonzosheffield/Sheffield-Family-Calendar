use family_calendar::client::app::App;

#[cfg(not(feature = "server"))]
fn main() {
    dioxus::launch(App);
}

#[cfg(feature = "server")]
fn main() {
    use axum::routing::get;
    use dioxus::prelude::*;
    use family_calendar::server::{api::realtime, calendar, db};
    use tower_http::services::ServeDir;

    tokio::runtime::Runtime::new()
        .expect("failed to start tokio runtime")
        .block_on(async move {
            db::pool().await.expect("failed to open family.db");
            calendar::spawn_polling_task();

            // Dioxus 0.7 removed the 0.6 serve-config builder type;
            // `ServeConfig::new()` picks up the `dx`-generated
            // `public/index.html` next to the executable, and
            // `serve_dioxus_application` now hands back a fully-stated
            // `Router<()>` instead of `Self`.
            let router = axum::Router::new()
                .route("/ws", get(realtime::ws_handler))
                .nest_service("/uploads", ServeDir::new(db::UPLOAD_DIR))
                .serve_dioxus_application(ServeConfig::new(), App);

            let address = dioxus_cli_config::fullstack_address_or_localhost();
            let listener = tokio::net::TcpListener::bind(address)
                .await
                .expect("failed to bind server address");

            axum::serve(listener, router.into_make_service())
                .await
                .expect("server error");
        });
}
