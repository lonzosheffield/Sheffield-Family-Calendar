#[cfg(not(feature = "server"))]
fn main() {
    use family_calendar::client::app::App;

    dioxus::launch(App);
}

// T0.6: routes live in `family_calendar::server::router` (frozen here — see
// `docs/reviews/PURPLE_TEAM.md` §P4). This file only boots the tokio runtime
// and hands off to `router::run`.
#[cfg(feature = "server")]
fn main() {
    use family_calendar::server::{config::FamilyHubConfig, router};

    // PURPLE §P5.4: the rustls CryptoProvider is installed as the first line of main.
    family_calendar::server::tls::install_crypto_provider();
    tokio::runtime::Runtime::new()
        .expect("failed to start tokio runtime")
        .block_on(router::run(FamilyHubConfig::load()));
}
