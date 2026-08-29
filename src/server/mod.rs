pub mod api;

#[cfg(feature = "server")]
pub mod auth;
#[cfg(feature = "server")]
pub mod calendar;
#[cfg(feature = "server")]
pub mod config;
#[cfg(feature = "server")]
pub mod db;
// T1.3 - TLS + PKI + mDNS for the split-origin deployment (PLAN v2 D3').
#[cfg(feature = "server")]
pub mod mdns;
#[cfg(feature = "server")]
pub mod pki;
#[cfg(feature = "server")]
pub mod router;
#[cfg(feature = "server")]
pub mod tls;
