//! The School ("house") tab's server-side storage (HS1, `docs/homeschool/PLAN_HOMESCHOOL.md` §2).
//!
//! Two halves, deliberately separate:
//!
//! * [`db`] — every query over the tables `migrations/0005_homeschool.sql`
//!   creates. Mutations are generic over `sqlx::SqliteExecutor` exactly like
//!   [`crate::server::db::set_routine_completion`], so a server fn can run the
//!   claim (`db::claim_mutation`) and the write on **one** transaction and have
//!   a failed write roll the idempotency claim back with it (QA round 1,
//!   Q1-08). Reads take `&SqlitePool` and are pointed at the read pool by
//!   their callers (H-9).
//! * [`loader`] — the TOML curriculum format (H5), its validator, the
//!   boot-time insert-missing-only loader, the `--replace` bulk path behind
//!   `family-hub.exe import-curriculum`, and the Isaiah enrollment seed.
//!
//! Nothing here knows about the shared DTOs in `src/shared/` (HS3's file, built
//! in the same wave): these functions return plain row structs and HS4's server
//! functions map them into the wire types. That keeps the two wave-A tasks on
//! disjoint files, as `docs/PLAN.md` §4 requires.
//!
//! **N1 (§0):** no curriculum *content* is committed anywhere in this module or
//! its tests. The only curriculum this repository carries is the invented
//! `tests/fixtures/curricula/sample-year.toml`; the family's real files live in
//! the gitignored `docs/homeschool/curriculum/` and are copied into the data
//! directory by hand.

#[cfg(feature = "server")]
pub mod db;
#[cfg(feature = "server")]
pub mod loader;

#[cfg(feature = "server")]
pub use loader::{load_and_seed, seed_enrollments};
