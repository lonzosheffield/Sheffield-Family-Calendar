//! Profile server functions.
//!
//! Split out of the former `src/server/api.rs` by T1.2 so that **T1.4** owns a
//! file of its own (`docs/reviews/PURPLE_TEAM.md` §P4). T1.4 lands
//! `0003_profiles.sql`, the profile CRUD server functions, the parent PIN and
//! the session token here.
//!
//! Protocol v2 already carries the broadcast T1.4 needs
//! ([`crate::shared::types::ServerMessage::ProfilesUpdated`], PURPLE default
//! 12/W6); [`publish_profiles_updated`] is the one call it has to make after a
//! profile is created, renamed, recoloured or removed.

/// Tell every connected client that the profile list changed, so it refetches.
///
/// Deliberately parameterless: a profile change is rare and affects the whole
/// roster, so there is nothing useful to scope it by (unlike `RoutineUpdated`
/// and `TasksUpdated`, which carry `user_id` + `date`).
#[cfg(feature = "server")]
pub fn publish_profiles_updated() {
    super::realtime::publish(&crate::shared::types::ServerMessage::ProfilesUpdated);
}
