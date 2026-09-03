//! The offline mutation queue (PLAN v2 **D6** / **R-15**, task **T2.2**).
//!
//! A parent ticks a routine item on a phone that has just walked out of Wi-Fi
//! range. The call fails. Three things then have to be true for the tick not
//! to be silently lost or, worse, silently re-applied to the *wrong day*:
//!
//! * the mutation is kept, in `localStorage`, so it survives the PWA being
//!   swiped away or the phone being locked;
//! * it carries the **date it was intended for**, not the date it eventually
//!   reaches the server — a tick made at 23:58 that replays at 00:05 belongs
//!   to yesterday, and the server validates that date within ±1 day
//!   (`db::date_within_window`, T1.5);
//! * it carries an **idempotency key**, minted once at enqueue time and never
//!   regenerated, so a replay that is delivered twice (a retried request, an
//!   iOS app opened twice, two tabs) is claimed once by `db::claim_mutation`
//!   and applied once.
//!
//! There is no Background Sync here on purpose: iOS Safari does not implement
//! it, so relying on it would give the two parents different guarantees
//! depending on their phone. Replay is driven by the Rust client on
//! reconnect and on app open, which behaves identically on both platforms —
//! the promise written down in `docs/PWA.md`.
//!
//! Everything in this module is a plain struct with injected time and an
//! injected sender: no DOM, no Dioxus, no clock of its own inside the logic.
//! That is what makes the T2.2 acceptance assertions (`tests/pwa_tests.rs`)
//! real tests rather than a browser demo.

use serde::{Deserialize, Serialize};

use crate::client::components::mobile::storage;

/// `localStorage` key holding the serialized queue.
pub const QUEUE_STORAGE_KEY: &str = "familyhub.offline_queue.v1";

/// How long a queued mutation stays replayable, in milliseconds.
///
/// **48 hours.** Past that the mutation is almost certainly about a day the
/// server would reject anyway (`date_within_window` is ±1 day) and replaying
/// it would tick a box the family has long since moved on from. Dropping it
/// is the honest outcome — and it is dropped *loudly*, with a toast, never
/// quietly.
pub const MAX_AGE_MS: i64 = 48 * 60 * 60 * 1000;

/// Upper bound on queued entries, so a phone left offline for a week cannot
/// grow `localStorage` without limit. Oldest first out.
pub const MAX_ENTRIES: usize = 200;

/// One mutation the phone could not deliver.
///
/// Only single-boy toggles are queueable. Photo upload is deliberately *not*:
/// a multi-megabyte body does not belong in `localStorage`, and T2.5's
/// upload route is a foreground action a parent can simply retry. Nor is a
/// **Together** tick (`docs/homeschool/PLAN_HOMESCHOOL.md` §2 H6): it fans out
/// over a group whose membership only the server knows, so replaying it from
/// a phone that has been offline for a day could tick a boy who has since
/// moved to another week. It shows a toast on failure instead.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "mutation", rename_all = "snake_case")]
pub enum QueuedMutation {
    ToggleRoutineTask {
        user_id: u32,
        template_id: u32,
        completed: bool,
    },
    ToggleCustomTask {
        user_id: u32,
        task_id: u32,
        completed: bool,
    },
    /// One boy's School occurrence (HS5, review finding R-14).
    ///
    /// A lesson tick needs **two** dates: `QueuedMutationEntry::date` is the
    /// day the parent made the change (what the server's ±1 day window
    /// checks), while `scheduled_date` is the day the occurrence was *due* —
    /// four days earlier, for a catch-up tick. Carrying only the first, as
    /// the routine toggles do, would replay a catch-up onto the wrong day.
    ToggleLesson {
        user_id: u32,
        subject_id: i64,
        assignment_id: Option<i64>,
        week: i64,
        scheduled_date: String,
        completed: bool,
    },
    /// One parent-added School task (HS5, review finding D-7).
    ///
    /// Without this a parent's offline tick on an extra would be silently lost
    /// while the identical tick one row above — a lesson — survived, an
    /// inconsistency `docs/PWA.md` promises does not exist.
    ToggleExtra {
        user_id: u32,
        extra_id: i64,
        completed: bool,
    },
}

impl QueuedMutation {
    /// The profile the mutation acts on — the same `user_id` the server's
    /// ownership check (T1.5) will test.
    pub fn user_id(&self) -> u32 {
        match self {
            QueuedMutation::ToggleRoutineTask { user_id, .. }
            | QueuedMutation::ToggleCustomTask { user_id, .. }
            | QueuedMutation::ToggleLesson { user_id, .. }
            | QueuedMutation::ToggleExtra { user_id, .. } => *user_id,
        }
    }

    /// Short human label for the queue list in Settings.
    pub fn label(&self) -> &'static str {
        match self {
            QueuedMutation::ToggleRoutineTask { .. } => "Routine step",
            QueuedMutation::ToggleCustomTask { .. } => "Task",
            QueuedMutation::ToggleLesson { .. } => "School lesson",
            QueuedMutation::ToggleExtra { .. } => "School task",
        }
    }
}

/// A queued mutation plus the two fields R-15 exists for.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct QueuedMutationEntry {
    /// Minted once, at enqueue time. **Never** regenerated on replay — that
    /// is the whole idempotency guarantee.
    pub key: String,
    /// `YYYY-MM-DD`, the day the change was *meant* for.
    pub date: String,
    /// Wall-clock milliseconds at enqueue, for the 48 h expiry.
    pub queued_at_ms: i64,
    pub mutation: QueuedMutation,
}

impl QueuedMutationEntry {
    pub fn is_expired(&self, now_ms: i64) -> bool {
        now_ms.saturating_sub(self.queued_at_ms) > MAX_AGE_MS
    }
}

/// Something the UI should tell the family about. Rendered as a toast by
/// [`super::MobileShell`]; returned as data here so the queue itself stays
/// free of any UI dependency.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum QueueToast {
    /// Entries passed [`MAX_AGE_MS`] and were dropped without being applied.
    Expired { count: usize, dates: Vec<String> },
    /// Entries were delivered to the server.
    Replayed { count: usize },
    /// Replay stopped early; `remaining` are still queued for the next try.
    ReplayFailed { remaining: usize, reason: String },
}

impl QueueToast {
    pub fn message(&self) -> String {
        match self {
            QueueToast::Expired { count, dates } => format!(
                "{count} offline change{} from {} {} too old to send and {} discarded.",
                plural(*count),
                dates.join(", "),
                was_were(*count),
                was_were(*count)
            ),
            QueueToast::Replayed { count } => {
                format!("Sent {count} change{} saved while offline.", plural(*count))
            }
            QueueToast::ReplayFailed { remaining, reason } => format!(
                "Still offline — {remaining} change{} waiting to send ({reason}).",
                plural(*remaining)
            ),
        }
    }

    /// `true` for the toast the acceptance test looks for: an expiry drop.
    pub fn is_expiry(&self) -> bool {
        matches!(self, QueueToast::Expired { .. })
    }
}

fn plural(count: usize) -> &'static str {
    if count == 1 {
        ""
    } else {
        "s"
    }
}

fn was_were(count: usize) -> &'static str {
    if count == 1 {
        "was"
    } else {
        "were"
    }
}

/// What one [`OfflineQueue::replay`] pass did.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct ReplayReport {
    /// Entries the server accepted (or deduped) and that left the queue.
    pub sent: usize,
    /// Entries dropped by the 48 h expiry before any send was attempted.
    pub expired: usize,
    /// Entries still queued when the pass ended.
    pub remaining: usize,
    /// Toasts for the UI, in the order they happened.
    pub toasts: Vec<QueueToast>,
}

/// The queue itself: an ordered list of undelivered mutations.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct OfflineQueue {
    entries: Vec<QueuedMutationEntry>,
}

impl OfflineQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[QueuedMutationEntry] {
        &self.entries
    }

    /// Queue `mutation` for `date`, minting a fresh idempotency key.
    ///
    /// Returns the stored entry so a caller can log or display it. The key
    /// comes from [`crate::client::components::routine::new_idempotency_key`],
    /// which is already the key generator every *online* mutation on this
    /// client uses — one generator, one uniqueness argument.
    pub fn enqueue(
        &mut self,
        mutation: QueuedMutation,
        date: impl Into<String>,
        now_ms: i64,
    ) -> QueuedMutationEntry {
        let entry = QueuedMutationEntry {
            key: crate::client::components::routine::new_idempotency_key(),
            date: date.into(),
            queued_at_ms: now_ms,
            mutation,
        };
        self.push(entry.clone());
        entry
    }

    /// Re-admit an entry that already has a key — used when a replay pass
    /// fails partway and by [`Self::load`].
    pub fn push(&mut self, entry: QueuedMutationEntry) {
        self.entries.push(entry);
        while self.entries.len() > MAX_ENTRIES {
            self.entries.remove(0);
        }
    }

    /// Drop everything older than [`MAX_AGE_MS`], returning a toast when
    /// anything was actually dropped.
    ///
    /// Separated from [`Self::replay`] so the expiry rule can be tested (and
    /// run on app open, before the network is even consulted) on its own.
    pub fn expire(&mut self, now_ms: i64) -> Option<QueueToast> {
        let mut count = 0usize;
        let mut dates: Vec<String> = Vec::new();
        self.entries.retain(|entry| {
            if entry.is_expired(now_ms) {
                count += 1;
                if !dates.contains(&entry.date) {
                    dates.push(entry.date.clone());
                }
                false
            } else {
                true
            }
        });
        if count == 0 {
            None
        } else {
            Some(QueueToast::Expired { count, dates })
        }
    }

    /// Send every queued entry through `send`, oldest first, stopping at the
    /// first failure so ordering is preserved for the next attempt.
    ///
    /// `send` is injected rather than hard-wired to the server functions so
    /// the acceptance test can count calls and effects. The production caller
    /// passes [`send_to_server`].
    pub async fn replay<F, Fut>(&mut self, now_ms: i64, mut send: F) -> ReplayReport
    where
        F: FnMut(QueuedMutationEntry) -> Fut,
        Fut: std::future::Future<Output = Result<(), String>>,
    {
        let mut report = ReplayReport::default();

        let before = self.entries.len();
        if let Some(toast) = self.expire(now_ms) {
            report.expired = before - self.entries.len();
            report.toasts.push(toast);
        }

        let pending = std::mem::take(&mut self.entries);
        let mut iter = pending.into_iter();
        let mut failure: Option<String> = None;

        for entry in iter.by_ref() {
            match send(entry.clone()).await {
                Ok(()) => report.sent += 1,
                Err(reason) => {
                    // Keep this one and everything behind it, in order.
                    self.entries.push(entry);
                    failure = Some(reason);
                    break;
                }
            }
        }
        self.entries.extend(iter);

        if report.sent > 0 {
            report
                .toasts
                .push(QueueToast::Replayed { count: report.sent });
        }
        report.remaining = self.entries.len();
        if let Some(reason) = failure {
            report.toasts.push(QueueToast::ReplayFailed {
                remaining: report.remaining,
                reason,
            });
        }
        report
    }

    // -----------------------------------------------------------------
    // Persistence
    // -----------------------------------------------------------------

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{\"entries\":[]}".to_string())
    }

    /// Parse a stored queue. A corrupt or older-shaped payload yields an
    /// empty queue rather than an error: losing an unreadable queue is bad,
    /// but a phone that cannot open its own app at all is worse.
    pub fn from_json(raw: &str) -> Self {
        serde_json::from_str(raw).unwrap_or_default()
    }

    /// Read the queue out of `localStorage`.
    pub fn load() -> Self {
        storage::get(QUEUE_STORAGE_KEY)
            .map(|raw| Self::from_json(&raw))
            .unwrap_or_default()
    }

    /// Write the queue back to `localStorage`, removing the key entirely when
    /// the queue is empty so a fresh install never reads a stale shell.
    pub fn save(&self) {
        if self.entries.is_empty() {
            storage::remove(QUEUE_STORAGE_KEY);
        } else {
            storage::set(QUEUE_STORAGE_KEY, &self.to_json());
        }
    }
}

/// Record a mutation that could not be delivered, in one call.
///
/// Load, enqueue with a fresh key and the intended `date`, save. This is the
/// single line a failing call site needs:
///
/// ```ignore
/// if toggle_routine_task(user_id, template_id, completed, date.clone(), key).await.is_err() {
///     queue::record_offline_failure(
///         QueuedMutation::ToggleRoutineTask { user_id, template_id, completed },
///         date,
///     );
/// }
/// ```
///
/// The two call sites that need it live in
/// `src/client/components/routine.rs`, which belongs to T2.5 in this wave
/// (`docs/reviews/PURPLE_TEAM.md` §P4) — the request is filed in
/// `docs/HANDOFF.md` rather than made here.
pub fn record_offline_failure(mutation: QueuedMutation, date: impl Into<String>) {
    let mut queue = OfflineQueue::load();
    queue.enqueue(mutation, date, now_ms());
    queue.save();
}

/// Wall-clock milliseconds since the Unix epoch.
///
/// Deliberately **not**
/// [`crate::client::realtime::now_millis`], which is `performance.now()` —
/// monotonic since page load and therefore reset to ~0 every time the PWA is
/// reopened. A 48 h expiry measured on that clock would never fire.
#[cfg(all(feature = "web", target_arch = "wasm32"))]
pub fn now_ms() -> i64 {
    wasm_date::now() as i64
}

#[cfg(all(feature = "web", target_arch = "wasm32"))]
mod wasm_date {
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = Date, js_name = now)]
        pub fn now() -> f64;
    }
}

#[cfg(not(all(feature = "web", target_arch = "wasm32")))]
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_millis() as i64)
        .unwrap_or_default()
}

/// Deliver one queued entry through the real server functions.
///
/// The entry's own `date` and `key` are passed straight through, which is
/// what makes the replay both correctly dated and idempotent: the server
/// claims `key` in `mutation_log` and applies the change at most once,
/// however many times this is called.
pub async fn send_to_server(entry: QueuedMutationEntry) -> Result<(), String> {
    use crate::server::api::{
        toggle_custom_task, toggle_extra, toggle_lesson, toggle_routine_task,
    };
    use crate::shared::homeschool::LogStatus;

    let result = match entry.mutation {
        QueuedMutation::ToggleRoutineTask {
            user_id,
            template_id,
            completed,
        } => toggle_routine_task(user_id, template_id, completed, entry.date, entry.key).await,
        QueuedMutation::ToggleCustomTask {
            user_id,
            task_id,
            completed,
        } => toggle_custom_task(user_id, task_id, completed, entry.date, entry.key).await,
        // A queued School tick is always a plain *done* — `skipped` and a note
        // are deliberate, considered acts a parent makes with the hub in
        // front of them, not something worth replaying blind a day later.
        QueuedMutation::ToggleLesson {
            user_id,
            subject_id,
            assignment_id,
            week,
            scheduled_date,
            completed,
        } => {
            toggle_lesson(
                i64::from(user_id),
                subject_id,
                assignment_id,
                week,
                scheduled_date,
                completed,
                LogStatus::Done,
                None,
                entry.date,
                entry.key,
            )
            .await
        }
        QueuedMutation::ToggleExtra {
            user_id: _,
            extra_id,
            completed,
        } => {
            toggle_extra(
                extra_id,
                completed,
                LogStatus::Done,
                None,
                entry.date,
                entry.key,
            )
            .await
        }
    };
    result.map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toggle(template_id: u32) -> QueuedMutation {
        QueuedMutation::ToggleRoutineTask {
            user_id: 1,
            template_id,
            completed: true,
        }
    }

    #[test]
    fn a_queue_round_trips_through_json() {
        let mut queue = OfflineQueue::new();
        queue.enqueue(toggle(1), "2026-08-29", 1_000);
        queue.enqueue(
            QueuedMutation::ToggleCustomTask {
                user_id: 2,
                task_id: 7,
                completed: false,
            },
            "2026-08-28",
            2_000,
        );

        let restored = OfflineQueue::from_json(&queue.to_json());
        assert_eq!(restored, queue);
    }

    #[test]
    fn a_corrupt_payload_yields_an_empty_queue_rather_than_a_panic() {
        assert!(OfflineQueue::from_json("not json at all").is_empty());
        assert!(OfflineQueue::from_json("").is_empty());
    }

    #[test]
    fn the_queue_never_grows_past_its_cap() {
        let mut queue = OfflineQueue::new();
        for template_id in 0..(MAX_ENTRIES as u32 + 10) {
            queue.enqueue(toggle(template_id), "2026-08-29", 1_000);
        }
        assert_eq!(queue.len(), MAX_ENTRIES);
        // The oldest were the ones dropped.
        assert_eq!(
            queue.entries()[0].mutation,
            toggle(10),
            "the cap must drop the oldest entries, not the newest"
        );
    }
}
