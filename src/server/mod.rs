pub mod api;

#[cfg(feature = "server")]
pub mod calendar;
#[cfg(feature = "server")]
pub mod config;
#[cfg(feature = "server")]
pub mod db;
#[cfg(feature = "server")]
pub mod router;
