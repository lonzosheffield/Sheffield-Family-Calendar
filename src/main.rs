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

            let router = axum::Router::new()
                .route("/ws", get(realtime::ws_handler))
                .nest_service("/uploads", ServeDir::new(db::UPLOAD_DIR))
                .serve_dioxus_application(ServeConfigBuilder::default(), App);

            let address = dioxus_cli_config::fullstack_address_or_localhost();
            let listener = tokio::net::TcpListener::bind(address)
                .await
                .expect("failed to bind server address");

            axum::serve(listener, router.into_make_service())
                .await
                .expect("server error");
        });
}
