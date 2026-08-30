//! Photo-upload multipart route and delete-with-file server fn (T2.5).
//!
//! Replaces the v1 base64-through-a-`#[server]`-fn path
//! ([`super::routine::create_photo_task`]) for real camera photos: axum's
//! default body limit is 2 MB (`DefaultBodyLimit`, `axum-core`), so a modern
//! phone photo 413'd before it ever reached the database (G14/R-08). This
//! route gets its own raised limit — `DefaultBodyLimit::max(25 MiB)`, applied
//! **only** to this one route in `router.rs` (T2.5's edit there, PURPLE
//! §P3 T2.5) — and a real streaming `multipart/form-data` body instead of a
//! base64 blob inflating the JSON payload by another third.
//!
//! **Server-side re-encode, not just an extension check (R-23c).** The
//! uploaded bytes' format is *sniffed from its magic bytes*
//! (`image::guess_format`), never trusted from the client's filename or
//! `Content-Type` header: a `.svg` mislabelled `.jpg` sniffs as neither JPEG,
//! PNG nor WebP and is rejected (415) before a byte is written. Anything that
//! *does* sniff as one of the three allowed formats is decoded, downscaled to
//! fit within 1600×1600 if larger than that, and **re-encoded** through the
//! `image` crate's own encoder for that same sniffed format — so a valid PNG
//! renamed `.jpg` is stored with a `.png` extension (the sniffed format wins,
//! never the client's claim), and nothing resembling the original bytes
//! (which could smuggle a polyglot past a naive extension check) ever reaches
//! disk unmodified.
//!
//! `delete_custom_task` (a `#[server]` fn, not a raw route — deletion needs no
//! multipart body) reuses T1.6's `backup::delete_custom_task` verbatim, per
//! the task brief, after checking `user_id` actually owns `task_id` — the
//! same ownership rule T1.5 established for `toggle_custom_task` (R-23).
//!
//! **Reused by T2.7 (`docs/HANDOFF.md` "T2.7 → T2.5, restated").** T2.7's own
//! branch originally shipped a second, parallel base64-sniff-and-re-encode
//! implementation for screensaver photos, built before this file existed.
//! The reconciliation moved the sniff/decode/downscale/re-encode step into
//! [`sniff_downscale_reencode`] below — a pure, allowlist-and-re-encode
//! helper with no opinion on where the bytes end up — so
//! `super::screensaver::upload_screensaver_image_handler` calls the exact
//! same allowlist (jpeg/png/webp only, sniffed from magic bytes, R-23c) and
//! the exact same re-encode instead of maintaining its own copy. Only the
//! *destination* (directory, file name) still differs between the two
//! callers, which is why `store_photo` below stays the thin,
//! task-photo-specific wrapper around it.

use dioxus::prelude::*;

#[cfg(feature = "server")]
use axum::extract::Multipart;
#[cfg(feature = "server")]
use axum::http::StatusCode;
#[cfg(feature = "server")]
use axum::response::{IntoResponse, Response};

/// Images larger than this on either axis are downscaled before storage.
/// The phone UI already downscales before it ever sends bytes
/// (`client::components::routine`'s photo dialog) — this is the server-side
/// backstop that also covers a direct API call, a future non-Rust client, or
/// a phone whose client-side downscale silently failed.
#[cfg(feature = "server")]
const MAX_DIMENSION: u32 = 1600;

/// JPEG re-encode quality. 82 keeps a downscaled 1600px photo comfortably
/// under the 400 KB PURPLE §P3 T2.5(a) budget without visible banding.
#[cfg(feature = "server")]
const JPEG_QUALITY: u8 = 82;

/// The three formats this route accepts and re-encodes. Anything else —
/// including a format the `image` crate can decode but PURPLE §P3 T2.5 does
/// not list (GIF, BMP, TIFF, …), and anything that fails to sniff as an
/// image at all — is rejected with 415.
#[cfg(feature = "server")]
fn allowed_format(format: image::ImageFormat) -> Option<(&'static str, image::ImageFormat)> {
    match format {
        image::ImageFormat::Jpeg => Some(("jpg", image::ImageFormat::Jpeg)),
        image::ImageFormat::Png => Some(("png", image::ImageFormat::Png)),
        image::ImageFormat::WebP => Some(("webp", image::ImageFormat::WebP)),
        _ => None,
    }
}

/// `POST /api/upload_photo` — axum `Multipart`, wired into `router.rs` with
/// its own raised `DefaultBodyLimit` (R-08). Multipart fields:
///
/// | Field | Required | Meaning |
/// | --- | --- | --- |
/// | `user_id` | yes | the profile the task belongs to |
/// | `title` | yes, non-empty | the task's title |
/// | `due_date` | no | `YYYY-MM-DD`; the task auto-hides once this passes |
/// | `photo` | no | the image file |
///
/// A request with no `photo` field still creates a task (a title-only custom
/// task, same as the v1 base64 fn with `photo_base64: None`).
#[cfg(feature = "server")]
pub async fn upload_photo_handler(mut multipart: Multipart) -> Response {
    let mut user_id: Option<u32> = None;
    let mut title: Option<String> = None;
    let mut due_date: Option<String> = None;
    let mut photo_bytes: Option<Vec<u8>> = None;

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(err) => return err.into_response(),
        };
        let field_name = field.name().unwrap_or_default().to_string();
        match field_name.as_str() {
            "user_id" => {
                if let Ok(text) = field.text().await {
                    user_id = text.trim().parse().ok();
                }
            }
            "title" => {
                if let Ok(text) = field.text().await {
                    let text = text.trim().to_string();
                    if !text.is_empty() {
                        title = Some(text);
                    }
                }
            }
            "due_date" => {
                if let Ok(text) = field.text().await {
                    let text = text.trim().to_string();
                    if !text.is_empty() {
                        due_date = Some(text);
                    }
                }
            }
            "photo" => match field.bytes().await {
                Ok(bytes) => {
                    if !bytes.is_empty() {
                        photo_bytes = Some(bytes.to_vec());
                    }
                }
                Err(err) => return err.into_response(),
            },
            _ => {
                // Unknown field: drain it and move on rather than error, so a
                // future client can add a field without breaking this server.
                if let Err(err) = field.bytes().await {
                    return err.into_response();
                }
            }
        }
    }

    let Some(user_id) = user_id else {
        return (StatusCode::BAD_REQUEST, "missing user_id").into_response();
    };
    let Some(title) = title else {
        return (StatusCode::BAD_REQUEST, "missing or empty title").into_response();
    };

    let photo_path = match photo_bytes {
        Some(bytes) => match store_photo(&bytes, user_id).await {
            Ok(path) => Some(path),
            Err(response) => return *response,
        },
        None => None,
    };

    let pool = match crate::server::db::pool().await {
        Ok(pool) => pool,
        Err(err) => {
            tracing::error!(%err, "upload_photo: database pool unavailable");
            return (StatusCode::INTERNAL_SERVER_ERROR, "database unavailable").into_response();
        }
    };

    let task_id = match crate::server::db::insert_custom_task_with_due_date(
        pool,
        user_id,
        &title,
        photo_path.as_deref(),
        due_date.as_deref(),
    )
    .await
    {
        Ok(id) => id,
        Err(err) => {
            tracing::error!(%err, "upload_photo: failed to insert the custom task row");
            return (StatusCode::INTERNAL_SERVER_ERROR, "failed to save task").into_response();
        }
    };

    // G22/W1's fix, extended to creation: every connected client (the TV
    // included) learns about the new task immediately rather than on its next
    // poll.
    super::realtime::publish(&crate::shared::types::ServerMessage::TasksUpdated {
        user_id: i64::from(user_id),
        date: chrono::Local::now().format("%Y-%m-%d").to_string(),
    });

    (
        StatusCode::CREATED,
        axum::Json(serde_json::json!({ "id": task_id, "photo_path": photo_path })),
    )
        .into_response()
}

/// Sniff, decode, downscale-if-needed and re-encode `bytes`, writing the
/// result under the configured upload directory and returning the web path
/// (`/uploads/<file>`). `Err` is a ready-to-return HTTP response — 415 for
/// anything that does not sniff as an allowed format or fails to decode,
/// 500 for an I/O failure writing the re-encoded bytes. Boxed
/// (`clippy::result_large_err`): `Response` itself is ~130 bytes, too large
/// for a hot `Result::Err` by clippy's default threshold — trivial here since
/// every path through this function is already the unhappy, once-per-request
/// case, not a loop.
#[cfg(feature = "server")]
async fn store_photo(bytes: &[u8], user_id: u32) -> Result<String, Box<Response>> {
    let ReencodedImage {
        bytes: encoded,
        ext,
    } = sniff_downscale_reencode(bytes)?;

    let upload_dir = crate::server::db::upload_dir();
    tokio::fs::create_dir_all(&upload_dir)
        .await
        .map_err(|err| {
            tracing::error!(%err, "upload_photo: failed to create the uploads directory");
            Box::new((StatusCode::INTERNAL_SERVER_ERROR, "failed to store image").into_response())
        })?;

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let file_name = format!("task-{user_id}-{stamp}.{ext}");
    tokio::fs::write(upload_dir.join(&file_name), &encoded)
        .await
        .map_err(|err| {
            tracing::error!(%err, "upload_photo: failed to write the re-encoded image");
            Box::new((StatusCode::INTERNAL_SERVER_ERROR, "failed to store image").into_response())
        })?;

    Ok(format!("/uploads/{file_name}"))
}

/// The result of [`sniff_downscale_reencode`]: the re-encoded bytes plus the
/// file extension that matches the *sniffed* format (never the caller's
/// claimed one, R-23c).
#[cfg(feature = "server")]
pub(crate) struct ReencodedImage {
    pub bytes: Vec<u8>,
    pub ext: &'static str,
}

/// The shared allowlist-and-re-encode pipeline (R-23c): sniff `bytes`' real
/// format from its magic bytes (never a claimed filename or `Content-Type`),
/// reject anything outside jpeg/png/webp, decode, downscale to fit within
/// [`MAX_DIMENSION`] on either axis if larger, and re-encode through the
/// `image` crate's own encoder for that sniffed format. Pure with respect to
/// storage — the caller decides where the bytes end up — so both
/// [`store_photo`] (task photos, under `/uploads`) and
/// `super::screensaver::upload_screensaver_image_handler` (ambient
/// screensaver photos, under `/assets/screensaver`) call this one
/// implementation instead of each maintaining their own copy of it.
#[cfg(feature = "server")]
pub(crate) fn sniff_downscale_reencode(bytes: &[u8]) -> Result<ReencodedImage, Box<Response>> {
    let sniffed = image::guess_format(bytes).ok().and_then(allowed_format);
    let Some((ext, format)) = sniffed else {
        return Err(Box::new(
            (
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "unsupported image type: only JPEG, PNG and WebP are accepted",
            )
                .into_response(),
        ));
    };

    let decoded = image::load_from_memory_with_format(bytes, format).map_err(|err| {
        tracing::warn!(%err, "sniff_downscale_reencode: sniffed a supported format but failed to decode it");
        Box::new(
            (
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "could not decode image data",
            )
                .into_response(),
        )
    })?;

    let (width, height) = (decoded.width(), decoded.height());
    let resized = if width > MAX_DIMENSION || height > MAX_DIMENSION {
        decoded.resize(
            MAX_DIMENSION,
            MAX_DIMENSION,
            image::imageops::FilterType::Triangle,
        )
    } else {
        decoded
    };

    let encoded = encode(&resized, format).map_err(|err| {
        tracing::error!(%err, "sniff_downscale_reencode: failed to re-encode image");
        Box::new((StatusCode::INTERNAL_SERVER_ERROR, "failed to encode image").into_response())
    })?;

    Ok(ReencodedImage {
        bytes: encoded,
        ext,
    })
}

/// Re-encode `image` in `format`, using an explicit quality for JPEG (the
/// generic `DynamicImage::write_to` path defaults to a fixed quality with no
/// way to tune it) and the crate's defaults for PNG/WebP (lossless — there is
/// no meaningful "quality" knob to turn down for either).
#[cfg(feature = "server")]
fn encode(image: &image::DynamicImage, format: image::ImageFormat) -> image::ImageResult<Vec<u8>> {
    use std::io::Cursor;

    let mut buf = Cursor::new(Vec::new());
    match format {
        image::ImageFormat::Jpeg => {
            let encoder =
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, JPEG_QUALITY);
            image.write_with_encoder(encoder)?;
        }
        other => image.write_to(&mut buf, other)?,
    }
    Ok(buf.into_inner())
}

/// Delete a custom task and its photo file. Reuses T1.6's
/// `backup::delete_custom_task` verbatim (per the task brief: "T1.6 provides
/// `delete_custom_task_row` in `db.rs` — reuse it"; `backup::delete_custom_task`
/// is the function that already wraps that row-delete with the file removal).
/// `user_id` must own `task_id` — the same ownership rule T1.5 established for
/// `toggle_custom_task` (R-23: "user 2 cannot toggle/delete user 3's task").
#[server(endpoint = "delete_custom_task")]
pub async fn delete_custom_task(user_id: u32, task_id: u32) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        // The ownership check only reads (H-9): the read pool, not the write
        // one, exactly like `toggle_custom_task`'s equivalent check.
        let read_pool = crate::server::db::read_pool()
            .await
            .map_err(super::to_server_error)?;
        let owner = crate::server::db::custom_task_owner(read_pool, task_id)
            .await
            .map_err(super::to_server_error)?;
        if owner != Some(user_id) {
            return Err(ServerFnError::new(
                "you may not delete another profile's task",
            ));
        }

        let pool = crate::server::db::pool()
            .await
            .map_err(super::to_server_error)?;
        let existed = crate::server::backup::delete_custom_task(
            pool,
            task_id,
            crate::server::db::upload_dir(),
        )
        .await
        .map_err(super::to_server_error)?;

        if existed {
            super::realtime::publish(&crate::shared::types::ServerMessage::TasksUpdated {
                user_id: i64::from(user_id),
                date: chrono::Local::now().format("%Y-%m-%d").to_string(),
            });
        }
        Ok(())
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (user_id, task_id);
        unreachable!("server function bodies only run on the server")
    }
}
