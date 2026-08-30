//! Screensaver server functions.
//!
//! Split out of the former `src/server/api.rs` by T1.2; **owned by T2.7** from
//! here on (phone uploads, placeholder seeding, idle timeout, optional
//! schedule).
//!
//! **Reconciled with T2.5 (`docs/HANDOFF.md` "T2.7 → T2.5, restated").** The
//! plan's title for this task reads "phone upload route reusing T2.5's
//! pipeline", but T2.5 (the general photo-task multipart route) had not
//! landed when this task first ran, so its first pass shipped a parallel,
//! self-contained base64-through-a-`#[server]`-fn pipeline instead. Now that
//! `src/server/api/photos.rs` (T2.5) has landed on `main`,
//! [`upload_screensaver_image_handler`] below is a real
//! `axum::extract::Multipart` route — the same shape as T2.5's own
//! `POST /api/upload_photo` — wired into `src/server/router.rs` with the
//! same raised body limit and the same `nosniff`/`attachment` headers on the
//! directory it serves from, and it calls
//! [`super::photos::sniff_downscale_reencode`] for the allowlist + re-encode
//! step rather than re-implementing it. The old base64 `#[server] fn
//! upload_screensaver_image` is gone; there is exactly one
//! allowlist-and-re-encode implementation in the tree now.

use dioxus::prelude::*;

#[cfg(feature = "server")]
use axum::extract::Multipart;
#[cfg(feature = "server")]
use axum::http::StatusCode;
#[cfg(feature = "server")]
use axum::response::{IntoResponse, Response};

/// Web paths of the ambient screensaver photos, sorted by file name.
#[server(endpoint = "list_screensaver_images")]
pub async fn list_screensaver_images() -> Result<Vec<String>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        let dir = crate::server::config::FamilyHubConfig::load().screensaver_dir();
        // R-31/G8 end-to-end: T0.7 committed 3 CC0 JPEGs into the **in-tree**
        // `assets/screensaver/` (owned by T0.7), but the directory this
        // function actually reads is the **runtime** data directory
        // (`docs/reviews/PURPLE_TEAM.md` §P4: "T2.7 — screensaver uploads at
        // runtime, not in-tree"), which starts out empty on a fresh install.
        // Seed it from the placeholders embedded at compile time so the
        // acceptance contract ("lists >= 3 images, every URL 200
        // image/jpeg") holds the very first time this ever runs, with no
        // separate install step.
        ensure_placeholders_seeded(&dir).await;
        ensure_background_tasks(ScreensaverSchedule::default());
        Ok(images_in_dir(&dir).await)
    }
    #[cfg(not(feature = "server"))]
    {
        unreachable!("server function bodies only run on the server")
    }
}

/// The on-disk directory is resolved from `FamilyHubConfig` (T0.5: absolute,
/// under `FAMILY_HUB_DATA_DIR`), independent of the URL route the
/// screensaver is served under (`/assets/screensaver`, wired up by T0.6's
/// `ServeDir`). Shared between [`list_screensaver_images`] and
/// [`upload_screensaver_image_handler`] so both report exactly the same set
/// of images.
#[cfg(feature = "server")]
async fn images_in_dir(dir: &std::path::Path) -> Vec<String> {
    const URL_PREFIX: &str = "/assets/screensaver";

    let mut images = Vec::new();
    let Ok(mut entries) = tokio::fs::read_dir(dir).await else {
        return images;
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        let is_image = matches!(
            std::path::Path::new(&name)
                .extension()
                .and_then(|ext| ext.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref(),
            Some("jpg" | "jpeg" | "png" | "webp" | "avif")
        );
        if is_image {
            images.push(format!("{URL_PREFIX}/{name}"));
        }
    }

    images.sort();
    images
}

/// `POST /api/upload_screensaver_image` — an `axum::extract::Multipart`
/// route wired into `src/server/router.rs` alongside T2.5's
/// `POST /api/upload_photo`, with the same raised `DefaultBodyLimit`. Accepts
/// two multipart fields: `auth` (the parent session token, sent **before**
/// `photo`) and `photo`, and adds the photo to the ambient screensaver's
/// runtime directory.
///
/// Allowlist + re-encode is [`super::photos::sniff_downscale_reencode`] —
/// the same implementation T2.5's task-photo route calls, not a second copy
/// of it (`docs/HANDOFF.md` "T2.7 → T2.5, restated"): the payload's real
/// format is sniffed from its magic bytes and anything that is not
/// jpeg/png/webp is rejected before it is decoded at all; whatever *is*
/// accepted is downscaled if oversized and re-encoded through that same
/// sniffed format's encoder, so a mis-labelled or hostile upload can never
/// reach disk as anything other than a re-encode this server itself
/// produced. Responds with the refreshed image list (same JSON shape
/// [`list_screensaver_images`] returns) so a caller can update its UI in one
/// round trip.
///
/// **Q1-07**: gated on [`super::photos::require_parent_session`] — the same
/// check `upload_photo_handler` uses — before the `photo` bytes are read, so
/// an unbounded (up to 25 MiB) unauthenticated post can no longer fill
/// `screensaver/` on any LAN client's say-so. **Q2-02**: that check now
/// accepts the `fh_session` cookie as well as the `auth` field, which is why
/// this handler takes a `HeaderMap`.
#[cfg(feature = "server")]
pub async fn upload_screensaver_image_handler(
    headers: axum::http::HeaderMap,
    mut multipart: Multipart,
) -> Response {
    let mut auth: Option<String> = None;
    let mut photo_bytes: Option<Vec<u8>> = None;

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(err) => return err.into_response(),
        };
        match field.name() {
            Some("auth") => {
                if let Ok(text) = field.text().await {
                    auth = Some(text);
                }
            }
            Some("photo") => {
                if let Err(response) =
                    super::photos::require_parent_session(auth.as_deref(), &headers)
                {
                    return *response;
                }
                match field.bytes().await {
                    Ok(bytes) => {
                        if !bytes.is_empty() {
                            photo_bytes = Some(bytes.to_vec());
                        }
                    }
                    Err(err) => return err.into_response(),
                }
            }
            _ => {
                // Unknown field: drain it and move on rather than error, so a
                // future client can add a field without breaking this route —
                // the same rule T2.5's `upload_photo_handler` follows.
                if let Err(err) = field.bytes().await {
                    return err.into_response();
                }
            }
        }
    }

    let Some(bytes) = photo_bytes else {
        return (StatusCode::BAD_REQUEST, "missing photo field").into_response();
    };

    let reencoded = match super::photos::sniff_downscale_reencode(&bytes) {
        Ok(reencoded) => reencoded,
        Err(response) => return *response,
    };

    let dir = crate::server::config::FamilyHubConfig::load().screensaver_dir();
    if let Err(err) = tokio::fs::create_dir_all(&dir).await {
        tracing::error!(%err, "upload_screensaver_image: could not open the screensaver directory");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not store the image",
        )
            .into_response();
    }
    ensure_placeholders_seeded(&dir).await;

    let filename = format!("upload-{}.{}", uuid::Uuid::new_v4(), reencoded.ext);
    if let Err(err) = tokio::fs::write(dir.join(&filename), &reencoded.bytes).await {
        tracing::error!(%err, "upload_screensaver_image: failed to save the image");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not store the image",
        )
            .into_response();
    }

    ensure_background_tasks(ScreensaverSchedule::default());
    (StatusCode::OK, axum::Json(images_in_dir(&dir).await)).into_response()
}

/// The three CC0 placeholders T0.7 committed to the (git-tracked) source
/// tree, embedded into the binary so seeding the runtime directory needs no
/// install-time file copy step.
#[cfg(feature = "server")]
const PLACEHOLDER_JPEGS: [(&str, &[u8]); 3] = [
    (
        "placeholder-1.jpg",
        include_bytes!("../../../assets/screensaver/screensaver-1.jpg"),
    ),
    (
        "placeholder-2.jpg",
        include_bytes!("../../../assets/screensaver/screensaver-2.jpg"),
    ),
    (
        "placeholder-3.jpg",
        include_bytes!("../../../assets/screensaver/screensaver-3.jpg"),
    ),
];

/// Write out any placeholder that is not already present in `dir`. Cheap
/// (three `try_exists` checks) and idempotent, so it is safe to call on
/// every request rather than gating it behind a one-shot `OnceLock` — a
/// parent who deletes every photo from the runtime directory gets the
/// placeholders back on the next poll instead of an empty screensaver.
#[cfg(feature = "server")]
async fn ensure_placeholders_seeded(dir: &std::path::Path) {
    if tokio::fs::create_dir_all(dir).await.is_err() {
        return;
    }
    for (name, bytes) in PLACEHOLDER_JPEGS {
        let path = dir.join(name);
        if !tokio::fs::try_exists(&path).await.unwrap_or(false) {
            let _ = tokio::fs::write(&path, bytes).await;
        }
    }
}

// ---------------------------------------------------------------------------
// Optional scheduled screensaver (PURPLE §P5.5 default 20: off by default)
// ---------------------------------------------------------------------------

/// A schedule that forces the TV into the ambient screensaver at a fixed
/// local hour, independent of the idle-timeout that normally drives it.
///
/// `enabled: false` by default, per PURPLE §P5.5 default 20 ("screensaver
/// schedule off by default") — a family that never opts in gets exactly
/// today's idle-only behaviour, forever.
#[cfg(feature = "server")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreensaverSchedule {
    pub enabled: bool,
    /// Local hour (`0..=23`) the schedule fires at, when enabled.
    pub hour: u32,
}

#[cfg(feature = "server")]
impl Default for ScreensaverSchedule {
    fn default() -> Self {
        Self {
            enabled: false,
            hour: 22,
        }
    }
}

#[cfg(feature = "server")]
impl ScreensaverSchedule {
    /// Whether the schedule should fire on this tick: enabled, the current
    /// local hour matches the configured one, and it has not already fired
    /// for this same hour (so a loop ticking every minute fires once, not
    /// sixty times, per hour it is due).
    pub fn due(&self, current_hour: u32, last_fired_hour: Option<u32>) -> bool {
        self.enabled && current_hour == self.hour && last_fired_hour != Some(current_hour)
    }

    /// Build a schedule from [`crate::server::config::FamilyHubConfig::screensaver_schedule_hour`]
    /// (QA round 1, Q1-14). `enabled` follows `hour.is_some()` directly — an
    /// owner opts in by setting the hour, nothing more — so this is the
    /// enable path `ScreensaverSchedule::default()` never had: before this,
    /// `default()` was the *only* instance the codebase ever constructed,
    /// which made `enabled: true` dead code no caller could ever reach.
    pub fn from_config_hour(hour: Option<u32>) -> Self {
        match hour {
            Some(hour) => Self {
                enabled: true,
                hour,
            },
            None => Self::default(),
        }
    }
}

/// One evaluation of the schedule: pure and synchronous, so it is unit
/// testable without spawning the background loop or waiting on real time
/// (T2.7 acceptance (d): "with the schedule disabled, no `SetView` is
/// emitted at the configured hour").
#[cfg(feature = "server")]
pub fn evaluate_schedule(
    schedule: &ScreensaverSchedule,
    current_hour: u32,
    last_fired_hour: Option<u32>,
) -> Option<crate::shared::types::ServerMessage> {
    schedule.due(current_hour, last_fired_hour).then_some(
        crate::shared::types::ServerMessage::SetView {
            view: crate::shared::types::MaximizedView::Screensaver,
        },
    )
}

/// Guards the schedule loop so it is spawned at most once per process, no
/// matter how many times a caller in this module calls
/// [`ensure_background_tasks`].
///
/// **Wired at boot.** `src/server/router.rs::run` now calls this explicitly,
/// next to T1.2's `realtime::ensure_background_tasks()` and T1.6's
/// `backup::register_nightly_hooks()` (same pattern, H-7's precedent) — the
/// production wiring gap the first pass of this task left open
/// (`docs/HANDOFF.md` "Production wiring gap, logged rather than worked
/// around") is closed as part of this reconciliation. `OnceLock::get_or_init`
/// still guards it so the still-present self-start calls from
/// `list_screensaver_images` and `upload_screensaver_image_handler` (every
/// `/tv`/`/m` load, and every upload) stay harmless no-ops after boot has
/// already started it.
///
/// QA round 1, Q1-14: `ensure_background_tasks` now takes the
/// [`ScreensaverSchedule`] to run rather than always building
/// `ScreensaverSchedule::default()` itself, which was the actual bug — no
/// caller anywhere in the tree ever constructed an *enabled* schedule, so
/// `enabled: true` was unreachable code. `router::run` is the only caller
/// that can see [`crate::server::config::FamilyHubConfig`], so it is the one
/// that builds the real schedule via [`ScreensaverSchedule::from_config_hour`]
/// and wins the `OnceLock` race (it runs at boot, before either self-start
/// call can fire); the two self-start call sites below keep passing
/// `ScreensaverSchedule::default()` — disabled — purely as the pre-boot
/// safety net they always were.
#[cfg(feature = "server")]
static SCHEDULE_TASK_STARTED: std::sync::OnceLock<()> = std::sync::OnceLock::new();

#[cfg(feature = "server")]
pub fn ensure_background_tasks(schedule: ScreensaverSchedule) {
    SCHEDULE_TASK_STARTED.get_or_init(|| {
        tokio::spawn(schedule_loop(schedule));
    });
}

/// Re-evaluate `schedule` once a minute against the server-local hour and
/// publish [`crate::shared::types::ServerMessage::SetView`] when it fires.
/// With the default (disabled) schedule this evaluates to `None` on every
/// tick forever, so a family that never opts in never sees a `SetView` on
/// the wire from this loop — the behaviour T2.7 acceptance (d) checks
/// directly via [`evaluate_schedule`], without needing to wait on this real
/// loop.
#[cfg(feature = "server")]
async fn schedule_loop(schedule: ScreensaverSchedule) {
    use chrono::Timelike;

    let mut last_fired_hour: Option<u32> = None;
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        let hour = chrono::Local::now().hour();
        if let Some(message) = evaluate_schedule(&schedule, hour, last_fired_hour) {
            crate::server::api::realtime::publish(&message);
            last_fired_hour = Some(hour);
        }
    }
}

#[cfg(all(test, feature = "server"))]
mod schedule_tests {
    use super::*;
    use crate::shared::types::{MaximizedView, ServerMessage};

    #[test]
    fn schedule_is_disabled_by_default() {
        assert!(
            !ScreensaverSchedule::default().enabled,
            "PURPLE §P5.5 default 20: screensaver schedule off by default"
        );
    }

    /// QA round 1, Q1-14: `from_config_hour(None)` (no `familyhub.toml`
    /// `[screensaver] schedule_hour` and no `FAMILY_HUB_SCREENSAVER_HOUR`)
    /// must still land on exactly `ScreensaverSchedule::default()` — the
    /// off-by-default guarantee must survive the new constructor unchanged.
    #[test]
    fn from_config_hour_none_matches_default() {
        assert_eq!(
            ScreensaverSchedule::from_config_hour(None),
            ScreensaverSchedule::default()
        );
    }

    /// QA round 1, Q1-14: this is the enable path that did not exist before
    /// — `Some(hour)` is the only way to ever construct an `enabled: true`
    /// schedule now, and it must carry the configured hour through exactly.
    #[test]
    fn from_config_hour_some_enables_at_that_hour() {
        let schedule = ScreensaverSchedule::from_config_hour(Some(6));
        assert!(schedule.enabled);
        assert_eq!(schedule.hour, 6);
    }

    /// T2.7 acceptance (d): "with the schedule disabled, no `SetView` is
    /// emitted at the configured hour" — swept across every hour of the day,
    /// not just the configured one, so a disabled schedule is proven inert
    /// rather than merely untested at its one special hour.
    #[test]
    fn disabled_schedule_never_emits_at_any_hour() {
        let schedule = ScreensaverSchedule {
            enabled: false,
            hour: 22,
        };
        for hour in 0..24 {
            assert_eq!(
                evaluate_schedule(&schedule, hour, None),
                None,
                "hour {hour} must not emit while the schedule is disabled"
            );
        }
    }

    #[test]
    fn enabled_schedule_emits_only_at_its_configured_hour() {
        let schedule = ScreensaverSchedule {
            enabled: true,
            hour: 22,
        };
        assert_eq!(
            evaluate_schedule(&schedule, 22, None),
            Some(ServerMessage::SetView {
                view: MaximizedView::Screensaver
            })
        );
        for hour in (0..24).filter(|&h| h != 22) {
            assert_eq!(evaluate_schedule(&schedule, hour, None), None);
        }
    }

    #[test]
    fn enabled_schedule_does_not_repeat_within_the_same_hour() {
        let schedule = ScreensaverSchedule {
            enabled: true,
            hour: 22,
        };
        assert!(schedule.due(22, None));
        assert!(
            !schedule.due(22, Some(22)),
            "already fired for hour 22; a second tick in the same hour must not fire again"
        );
        assert!(
            schedule.due(22, Some(21)),
            "a stale last-fired hour must not suppress this hour's firing"
        );
    }
}
