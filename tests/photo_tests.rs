//! **T2.5 acceptance suite** — `docs/reviews/PURPLE_TEAM.md` §P3 T2.5:
//! "Photo tasks v2".
//!
//! | # | Assertion | Test |
//! | --- | --- | --- |
//! | a | 12 MP fixture → 2xx, < 3 s, stored ≤ 400 KB | `t2_5_a_*` |
//! | b | same shape of POST to a route without the raised limit → 413 | `t2_5_b_*` |
//! | c | `x.svg` (`image/svg+xml`) → 415, nothing written | `t2_5_c_*` |
//! | d | a valid PNG renamed `.jpg` → stored with the correct extension | `t2_5_d_*` |
//! | e | `GET /uploads/<f>` carries `nosniff` and `attachment` | `t2_5_e_*` |
//! | f | `due_date = yesterday` hidden from today's list; delete removes row + file | `t2_5_f_*` |
//! | g | (Q1-07) an upload with no parent session → 401, nothing written | `t2_5_g_*` |
//!
//! (a) posts with the **cookie only** since QA round 2's Q2-02 — that is what
//! the phone sends now — while (g) keeps proving that a request with neither
//! credential is refused.
//!
//! Own test binary/process, same reasoning `tests/routine_tests.rs` gives:
//! integration test binaries cannot share private helpers, and each `cargo
//! test` target is its own process so `DATABASE_URL`/`FAMILY_HUB_DATA_DIR`
//! set here cannot collide with another binary's.

#![cfg(feature = "server")]

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use family_calendar::server::auth;
use family_calendar::server::db;

/// Mint a fresh parent session token for a test that needs one. `auth`'s
/// session store is process-global, and this test binary's `axum::serve`
/// runs in-process (`tokio::spawn`), so a token minted here is exactly what
/// the running server's `auth::require_session` will accept (Q1-07).
fn parent_token() -> String {
    auth::issue_session()
}

// ---------------------------------------------------------------------------
// Harness (mirrors tests/routine_tests.rs::init_test_env / spawn_test_server)
// ---------------------------------------------------------------------------

fn init_test_env() -> PathBuf {
    static ONCE: std::sync::Once = std::sync::Once::new();
    let base = std::env::temp_dir().join(format!("familyhub-photo-tests-{}", std::process::id()));
    ONCE.call_once(|| {
        std::fs::create_dir_all(&base).expect("test scratch directory is creatable");

        let db_path = base.join("family.db");
        let url = format!(
            "sqlite://{}",
            db_path.display().to_string().replace('\\', "/")
        );
        std::env::set_var("DATABASE_URL", url);

        // `api::photos::upload_photo_handler`/`delete_custom_task` resolve
        // `db::upload_dir()` via `FamilyHubConfig::load()` (env-based), which
        // must agree with the explicit `FamilyHubConfig` this harness hands
        // `build_router` below, or the `/uploads` `ServeDir` and the
        // handler's own writes would point at two different directories.
        std::env::set_var("FAMILY_HUB_DATA_DIR", &base);

        let public = base.join("public");
        std::fs::create_dir_all(&public).expect("test public directory is creatable");
        std::env::set_var("DIOXUS_PUBLIC_PATH", &public);
    });
    base
}

async fn spawn_test_server() -> (SocketAddr, PathBuf) {
    let base = init_test_env();
    db::pool().await.expect("test sqlite pool opens");

    let config = family_calendar::server::config::FamilyHubConfig {
        data_dir: base.clone(),
        http_addr: "127.0.0.1:0".parse().expect("valid socket address"),
        tls_addr: "127.0.0.1:0".parse().expect("valid socket address"),
        screensaver_schedule_hour: None,
        log_level: None,
    };
    let router = family_calendar::server::router::build_router(&config);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind an ephemeral port");
    let addr = listener.local_addr().expect("listener has a local address");

    tokio::spawn(async move {
        let _ = axum::serve(listener, router.into_make_service()).await;
    });

    (addr, config.upload_dir())
}

/// A *second*, deliberately unlayered router carrying only
/// `upload_photo_handler` — no `DefaultBodyLimit::max` — to prove assertion
/// (b): the same handler, without T2.5's raised limit, falls back to axum's
/// global 2 MB default (`axum_core::extract::default_body_limit`) and 413s.
async fn spawn_unraised_upload_server() -> SocketAddr {
    init_test_env();
    db::pool().await.expect("test sqlite pool opens");

    let router = axum::Router::new().route(
        "/api/upload_photo",
        axum::routing::post(family_calendar::server::api::photos::upload_photo_handler),
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind an ephemeral port");
    let addr = listener.local_addr().expect("listener has a local address");

    tokio::spawn(async move {
        let _ = axum::serve(listener, router.into_make_service()).await;
    });

    addr
}

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("reqwest client builds")
}

/// Hand-build a `multipart/form-data` body rather than adding reqwest's
/// `multipart` cargo feature (`Cargo.toml` is not a T2.5-owned file, §P4) —
/// three text/byte parts is little enough boilerplate that pulling in
/// `mime_guess` + extra `futures-util` wiring for it isn't worth a
/// serialized Cargo.toml micro-commit.
struct Part {
    name: &'static str,
    file: Option<(&'static str, &'static str)>, // (filename, content_type)
    bytes: Vec<u8>,
}

fn text_part(name: &'static str, value: impl Into<Vec<u8>>) -> Part {
    Part {
        name,
        file: None,
        bytes: value.into(),
    }
}

fn file_part(
    name: &'static str,
    filename: &'static str,
    content_type: &'static str,
    bytes: Vec<u8>,
) -> Part {
    Part {
        name,
        file: Some((filename, content_type)),
        bytes,
    }
}

fn multipart_body(boundary: &str, parts: &[Part]) -> Vec<u8> {
    let mut body = Vec::new();
    for part in parts {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        match part.file {
            Some((filename, content_type)) => {
                body.extend_from_slice(
                    format!(
                        "Content-Disposition: form-data; name=\"{}\"; filename=\"{filename}\"\r\n",
                        part.name
                    )
                    .as_bytes(),
                );
                body.extend_from_slice(format!("Content-Type: {content_type}\r\n\r\n").as_bytes());
            }
            None => {
                body.extend_from_slice(
                    format!(
                        "Content-Disposition: form-data; name=\"{}\"\r\n\r\n",
                        part.name
                    )
                    .as_bytes(),
                );
            }
        }
        body.extend_from_slice(&part.bytes);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    body
}

async fn post_multipart(addr: SocketAddr, path: &str, parts: &[Part]) -> reqwest::Response {
    post_multipart_with_cookie(addr, path, None, parts).await
}

/// The same POST, optionally carrying an `fh_session` cookie instead of (or
/// as well as) an `auth` form field.
///
/// **Q2-02**: the phone no longer has a bearer token to append to the form —
/// its session is the `HttpOnly` cookie the browser attaches to the
/// same-origin `fetch` on its own — so `require_parent_session` accepts the
/// cookie too. This is the only credential a real phone sends now.
async fn post_multipart_with_cookie(
    addr: SocketAddr,
    path: &str,
    session_cookie: Option<&str>,
    parts: &[Part],
) -> reqwest::Response {
    const BOUNDARY: &str = "familyhub-t2-5-test-boundary";
    let body = multipart_body(BOUNDARY, parts);
    let mut request = http_client().post(format!("http://{addr}{path}")).header(
        "content-type",
        format!("multipart/form-data; boundary={BOUNDARY}"),
    );
    if let Some(token) = session_cookie {
        request = request.header("cookie", format!("fh_session={token}"));
    }
    request
        .body(body)
        .send()
        .await
        .expect("the upload route should respond")
}

/// A tiny, valid PNG (10×10, solid colour), encoded in memory. Used by
/// assertion (d) to prove the server re-encodes based on **sniffed** content,
/// not the client's claimed filename/`Content-Type`.
fn tiny_png_bytes() -> Vec<u8> {
    let img = image::RgbImage::from_pixel(10, 10, image::Rgb([200, 40, 40]));
    let mut buf = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut buf, image::ImageFormat::Png)
        .expect("encodes a tiny PNG");
    buf.into_inner()
}

// ---------------------------------------------------------------------------
// (a) 12 MP fixture → 2xx, < 3 s, stored ≤ 400 KB
// ---------------------------------------------------------------------------

#[tokio::test]
async fn t2_5_a_a_real_12mp_photo_uploads_fast_and_small() {
    let (addr, upload_dir) = spawn_test_server().await;
    let fixture = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/photo_12mp.jpg"
    ))
    .expect("the 12 MP fixture is committed (T0.7)");
    assert!(
        fixture.len() > 100_000,
        "sanity: the fixture should be a real photo, not a stub"
    );

    // Q2-02: **only** the `fh_session` cookie — no `auth` form field at all.
    // This is byte-for-byte the credential the phone now sends, since the
    // session stopped being a bearer value the page could read.
    let started = Instant::now();
    let response = post_multipart_with_cookie(
        addr,
        "/api/upload_photo",
        Some(&parent_token()),
        &[
            text_part("user_id", "1"),
            text_part("title", "Clean your room"),
            file_part("photo", "photo_12mp.jpg", "image/jpeg", fixture),
        ],
    )
    .await;
    let elapsed = started.elapsed();

    assert!(
        response.status().is_success(),
        "expected 2xx, got {}",
        response.status()
    );
    assert!(
        elapsed < Duration::from_secs(3),
        "upload took {elapsed:?}, budget is < 3 s"
    );

    let body: serde_json::Value = response.json().await.expect("JSON response");
    let photo_path = body["photo_path"]
        .as_str()
        .expect("photo_path present")
        .to_string();
    let file_name = photo_path.rsplit('/').next().expect("has a filename");
    let stored = upload_dir.join(file_name);
    let metadata = std::fs::metadata(&stored).expect("the re-encoded file exists on disk");
    assert!(
        metadata.len() <= 400 * 1024,
        "stored file is {} bytes, budget is <= 400 KB",
        metadata.len()
    );
    assert!(
        file_name.ends_with(".jpg"),
        "a JPEG upload should stay a .jpg after re-encode, got {file_name}"
    );
}

// ---------------------------------------------------------------------------
// (b) same POST, no raised limit → 413
// ---------------------------------------------------------------------------

#[tokio::test]
async fn t2_5_b_without_the_raised_limit_a_large_upload_413s() {
    let addr = spawn_unraised_upload_server().await;
    // Comfortably over axum's global 2 MB default; content need not be a
    // valid image — the body-size limit rejects the request before the
    // handler ever gets to sniff anything.
    let oversized = vec![0u8; 3 * 1024 * 1024];

    // Q1-07: `require_parent_session` runs the moment the `photo` field is
    // seen, *before* its bytes are read — so a request lacking `auth`
    // never reaches the body read this assertion is about at all (it 401s
    // first, which the test infra would legitimately see as a dropped
    // connection while a 3 MB body is still mid-flight). A valid session is
    // what lets the request reach the actual body-size check.
    let response = post_multipart(
        addr,
        "/api/upload_photo",
        &[
            text_part("auth", parent_token()),
            text_part("user_id", "1"),
            text_part("title", "Too big"),
            file_part("photo", "big.jpg", "image/jpeg", oversized),
        ],
    )
    .await;

    assert_eq!(
        response.status(),
        reqwest::StatusCode::PAYLOAD_TOO_LARGE,
        "an unraised route must 413 on a body over axum's 2 MB default — this is what proves T2.5's DefaultBodyLimit::max is the reason (a) succeeds"
    );
}

// ---------------------------------------------------------------------------
// (c) x.svg mislabelled image/svg+xml → 415, nothing written
// ---------------------------------------------------------------------------

#[tokio::test]
async fn t2_5_c_an_svg_is_rejected_and_nothing_is_written() {
    let (addr, upload_dir) = spawn_test_server().await;
    let before = count_files(&upload_dir);

    let svg =
        br#"<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>"#.to_vec();
    let response = post_multipart(
        addr,
        "/api/upload_photo",
        &[
            text_part("auth", parent_token()),
            text_part("user_id", "2"),
            text_part("title", "Sneaky svg"),
            file_part("photo", "x.svg", "image/svg+xml", svg),
        ],
    )
    .await;

    assert_eq!(
        response.status(),
        reqwest::StatusCode::UNSUPPORTED_MEDIA_TYPE
    );
    assert_eq!(
        count_files(&upload_dir),
        before,
        "a rejected upload must not write any file"
    );

    // Nothing written means no task row either: user 2's list is unaffected.
    let pool = db::pool().await.expect("pool");
    let tasks = db::custom_tasks(pool, 2).await.expect("tasks");
    assert!(
        tasks.iter().all(|task| task.title != "Sneaky svg"),
        "a rejected upload must not create a custom task row"
    );
}

fn count_files(dir: &std::path::Path) -> usize {
    std::fs::read_dir(dir)
        .map(|entries| entries.count())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// (d) a valid PNG renamed .jpg → stored with the correct extension
// ---------------------------------------------------------------------------

#[tokio::test]
async fn t2_5_d_a_png_renamed_jpg_is_reencoded_with_the_correct_extension() {
    let (addr, _upload_dir) = spawn_test_server().await;

    let response = post_multipart(
        addr,
        "/api/upload_photo",
        &[
            text_part("auth", parent_token()),
            text_part("user_id", "3"),
            text_part("title", "Sneaky png"),
            // Filename and Content-Type both claim JPEG; the bytes are a
            // real PNG. The server must sniff the magic bytes, not trust
            // either claim.
            file_part("photo", "photo.jpg", "image/jpeg", tiny_png_bytes()),
        ],
    )
    .await;

    assert!(
        response.status().is_success(),
        "status: {}",
        response.status()
    );
    let body: serde_json::Value = response.json().await.expect("JSON response");
    let photo_path = body["photo_path"].as_str().expect("photo_path present");
    assert!(
        photo_path.ends_with(".png"),
        "a sniffed-PNG upload must be stored as .png regardless of the claimed filename/type, got {photo_path}"
    );
}

// ---------------------------------------------------------------------------
// (e) GET /uploads/<f> carries nosniff and attachment
// ---------------------------------------------------------------------------

#[tokio::test]
async fn t2_5_e_uploads_are_served_with_nosniff_and_attachment() {
    let (addr, _upload_dir) = spawn_test_server().await;

    let upload = post_multipart(
        addr,
        "/api/upload_photo",
        &[
            text_part("auth", parent_token()),
            text_part("user_id", "1"),
            text_part("title", "Headers check"),
            file_part("photo", "photo.jpg", "image/jpeg", tiny_png_bytes()),
        ],
    )
    .await;
    assert!(upload.status().is_success());
    let body: serde_json::Value = upload.json().await.expect("JSON response");
    let photo_path = body["photo_path"].as_str().expect("photo_path present");

    let fetched = http_client()
        .get(format!("http://{addr}{photo_path}"))
        .send()
        .await
        .expect("GET /uploads/<f> should respond");
    assert!(fetched.status().is_success());
    assert_eq!(
        fetched
            .headers()
            .get("x-content-type-options")
            .and_then(|v| v.to_str().ok()),
        Some("nosniff")
    );
    let disposition = fetched
        .headers()
        .get("content-disposition")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    assert!(
        disposition.contains("attachment"),
        "Content-Disposition was {disposition:?}"
    );
}

// ---------------------------------------------------------------------------
// (f) due_date = yesterday hidden from today's list; delete removes row + file
// ---------------------------------------------------------------------------

#[tokio::test]
async fn t2_5_f_a_task_due_yesterday_is_hidden_and_delete_removes_row_and_file() {
    let (addr, upload_dir) = spawn_test_server().await;
    let pool = db::pool().await.expect("pool");
    let user_id = 4u32;

    let yesterday = (chrono::Local::now().date_naive() - chrono::Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    let expired_id =
        db::insert_custom_task_with_due_date(pool, user_id, "Expired task", None, Some(&yesterday))
            .await
            .expect("insert expired task");
    let _current_id =
        db::insert_custom_task_with_due_date(pool, user_id, "Current task", None, Some(&today))
            .await
            .expect("insert current task");

    let tasks = db::custom_tasks(pool, user_id).await.expect("tasks");
    assert!(
        tasks.iter().all(|task| task.id != expired_id),
        "a task due yesterday must be absent from today's list"
    );
    assert!(
        tasks.iter().any(|task| task.title == "Current task"),
        "a task due today must still be present"
    );

    // Upload a real task with a photo, then delete it: the row *and* the
    // file must both go.
    let token = parent_token();
    let upload = post_multipart(
        addr,
        "/api/upload_photo",
        &[
            text_part("auth", token.clone()),
            text_part("user_id", user_id.to_string()),
            text_part("title", "Delete me"),
            file_part("photo", "photo.jpg", "image/jpeg", tiny_png_bytes()),
        ],
    )
    .await;
    assert!(upload.status().is_success());
    let body: serde_json::Value = upload.json().await.expect("JSON response");
    let task_id = body["id"].as_u64().expect("id present") as u32;
    let photo_path = body["photo_path"].as_str().expect("photo_path present");
    let file_name = photo_path.rsplit('/').next().expect("has a filename");
    let stored = upload_dir.join(file_name);
    assert!(stored.exists(), "the uploaded file exists before delete");

    // Q1-07: delete_custom_task now takes the parent session as its first
    // argument.
    let delete_response = http_client()
        .post(format!("http://{addr}/api/delete_custom_task"))
        .header("content-type", "application/json")
        .body(format!(
            r#"{{"auth":"{token}","user_id":{user_id},"task_id":{task_id}}}"#
        ))
        .send()
        .await
        .expect("delete_custom_task should respond");
    assert!(
        delete_response.status().is_success(),
        "delete_custom_task status: {}",
        delete_response.status()
    );

    let owner_after = db::custom_task_owner(pool, task_id).await.expect("query");
    assert_eq!(owner_after, None, "the row must be gone after delete");
    assert!(!stored.exists(), "the photo file must be gone after delete");
}

// ---------------------------------------------------------------------------
// (g) Q1-07: an upload with no parent session is 401 and writes nothing
// ---------------------------------------------------------------------------

/// PLAN §0.3/§P5.5 default 35: photo capture and task administration live
/// behind the parent PIN. Before Q1-07, this route accepted uploads from any
/// LAN client with no credential at all. No `auth` field at all (not merely
/// an invalid one — `require_session("")` must also reject).
#[tokio::test]
async fn t2_5_g_an_upload_without_a_parent_session_is_401_and_writes_nothing() {
    let (addr, upload_dir) = spawn_test_server().await;
    let before = count_files(&upload_dir);

    let response = post_multipart(
        addr,
        "/api/upload_photo",
        &[
            text_part("user_id", "1"),
            text_part("title", "No session"),
            file_part("photo", "photo.jpg", "image/jpeg", tiny_png_bytes()),
        ],
    )
    .await;

    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
    assert_eq!(
        count_files(&upload_dir),
        before,
        "an unauthenticated upload must not write any file"
    );

    let pool = db::pool().await.expect("pool");
    let tasks = db::custom_tasks(pool, 1).await.expect("tasks");
    assert!(
        tasks.iter().all(|task| task.title != "No session"),
        "an unauthenticated upload must not create a custom task row"
    );
}
