pub mod api;

#[cfg(feature = "server")]
pub mod auth;
#[cfg(feature = "server")]
// T1.6 - backup, retention and delete-with-file paths (PLAN v2 R-17/R-18).
pub mod backup;
#[cfg(feature = "server")]
pub mod calendar;
#[cfg(feature = "server")]
pub mod config;
#[cfg(feature = "server")]
pub mod db;
// T1.7 - /health JSON + the TV staleness badge state machine.
#[cfg(feature = "server")]
pub mod health;
// HS1 - the School tab's storage: migration 0005's queries, the curriculum
// TOML loader and the enrollment seed (docs/homeschool/PLAN_HOMESCHOOL.md).
#[cfg(feature = "server")]
pub mod homeschool;
// T1.3 - TLS + PKI + mDNS for the split-origin deployment (PLAN v2 D3').
#[cfg(feature = "server")]
pub mod mdns;
#[cfg(feature = "server")]
pub mod pki;
#[cfg(feature = "server")]
pub mod router;
// T3.1 - Windows service host + CLI (PLAN v2 D9): install/uninstall/start/
// stop/status/run/tv-probe, logging, firewall + power-plan configuration.
#[cfg(feature = "server")]
pub mod service;
#[cfg(feature = "server")]
pub mod tls;
