//! Screensaver completion (T2.7): placeholder seeding, the phone upload
//! route, and GET-serves-image/jpeg, all exercised as real HTTP round trips
//! against the production router (`server::router::build_router`, T0.6),
//! mirroring `tests/http_tests.rs`'s harness pattern.
//!
//! `docs/reviews/PURPLE_TEAM.md` §P3 T2.7 acceptance:
//! - `GET /api/screensaver` [here: `list_screensaver_images`] lists >= 3
//!   images and every returned URL returns 200 with `image/jpeg`.
//! - Uploading a new image makes it appear in the list.
//! - (idle-timeout and schedule assertions live as unit tests in
//!   `src/client/components/screensaver.rs` and
//!   `src/server/api/screensaver.rs` — pure state machines, not HTTP.)
//! - (Q1-07) an upload with no parent session → 401, the list is unchanged.
//!
//! **Reconciled with T2.5 (`docs/HANDOFF.md` "T2.7 → T2.5, restated").** The
//! upload endpoint below is now `axum::extract::Multipart`, the same shape
//! T2.5's `tests/photo_tests.rs` exercises against `/api/upload_photo` — this
//! file's `Part`/`multipart_body`/`post_multipart` helpers are the same
//! hand-built multipart body for the same reason given there (own test
//! binary, `Cargo.toml` not owned by this task, three parts is little enough
//! boilerplate). The old JSON `{"image_base64": "..."}` body is gone with the
//! `#[server]` fn it used to hit.

#![cfg(feature = "server")]

use std::net::SocketAddr;
use std::time::Duration;

use family_calendar::server::auth;
use family_calendar::server::db;

/// Mint a fresh parent session token for a test that needs one — same
/// reasoning `tests/photo_tests.rs::parent_token` documents: `auth`'s session
/// store is process-global and this binary's server runs in-process.
fn parent_token() -> String {
    auth::issue_session()
}

/// Dioxus 0.7 mounts `#[server(endpoint = "...")]` functions under `/api`
/// (see `tests/http_tests.rs`); `/api/upload_screensaver_image` is a raw
/// axum route registered at the same path by `server::router::build_router`.
const LIST_ENDPOINT: &str = "/api/list_screensaver_images";
const UPLOAD_ENDPOINT: &str = "/api/upload_screensaver_image";

// ---------------------------------------------------------------------------
// Multipart helpers (mirrors tests/photo_tests.rs exactly)
// ---------------------------------------------------------------------------

struct Part {
    name: &'static str,
    file: Option<(&'static str, &'static str)>, // (filename, content_type)
    bytes: Vec<u8>,
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

fn text_part(name: &'static str, value: impl Into<Vec<u8>>) -> Part {
    Part {
        name,
        file: None,
        bytes: value.into(),
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
    const BOUNDARY: &str = "familyhub-t2-7-test-boundary";
    let body = multipart_body(BOUNDARY, parts);
    http_client()
        .post(format!("http://{addr}{path}"))
        .header(
            "content-type",
            format!("multipart/form-data; boundary={BOUNDARY}"),
        )
        .body(body)
        .send()
        .await
        .expect("the upload route should respond")
}

fn init_test_env() -> std::path::PathBuf {
    static ONCE: std::sync::Once = std::sync::Once::new();
    let base = std::env::temp_dir().join(format!(
        "familyhub-screensaver-tests-{}",
        std::process::id()
    ));
    ONCE.call_once(|| {
        std::fs::create_dir_all(&base).expect("test scratch directory is creatable");

        let db_path = base.join("family.db");
        let url = format!(
            "sqlite://{}",
            db_path.display().to_string().replace('\\', "/")
        );
        std::env::set_var("DATABASE_URL", url);

        let public = base.join("public");
        std::fs::create_dir_all(&public).expect("test public directory is creatable");
        std::env::set_var("DIOXUS_PUBLIC_PATH", &public);

        // `list_screensaver_images`/`upload_screensaver_image_handler` each
        // resolve `FamilyHubConfig::load()` independently (same pattern
        // `router::run` documents for `DATABASE_URL`), so this env var — not
        // the `config` this file hands to `build_router` — is what actually
        // decides which directory those two read and write. Without it they
        // fall back to the default data dir and every assertion below 404s
        // against the *router's* `ServeDir`, which serves `base` correctly.
        std::env::set_var("FAMILY_HUB_DATA_DIR", &base);
    });
    base
}

/// Boot the exact production router on an OS-assigned free port, with its
/// own throwaway data directory so this file's screensaver uploads never
/// collide with another test binary's.
async fn spawn_test_server() -> SocketAddr {
    let base = init_test_env();
    db::pool().await.expect("test sqlite pool opens");

    let config = family_calendar::server::config::FamilyHubConfig {
        data_dir: base,
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

    addr
}

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .expect("reqwest client builds")
}

/// (a) `GET /api/screensaver` (here: `list_screensaver_images`) lists >= 3
/// images on a **freshly seeded** data directory, and every one of them
/// answers 200 `image/jpeg` when actually fetched — the full R-31/G8
/// end-to-end proof: T0.7's placeholders are not just committed to the
/// source tree, they reach a real HTTP response.
#[tokio::test]
async fn screensaver_lists_at_least_three_placeholder_images_each_serving_as_jpeg() {
    let addr = spawn_test_server().await;
    let client = http_client();

    let response = client
        .post(format!("http://{addr}{LIST_ENDPOINT}"))
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .await
        .expect("list_screensaver_images should respond");
    assert_eq!(
        response.status().as_u16(),
        200,
        "list endpoint must answer 200"
    );

    let body = response.text().await.expect("response body");
    let images: Vec<String> =
        serde_json::from_str(&body).expect("list_screensaver_images returns a JSON string array");

    assert!(
        images.len() >= 3,
        "expected >= 3 seeded screensaver images, got {}: {images:?}",
        images.len()
    );

    for path in &images {
        let image_response = client
            .get(format!("http://{addr}{path}"))
            .send()
            .await
            .unwrap_or_else(|err| panic!("GET {path} should respond: {err}"));
        assert_eq!(
            image_response.status().as_u16(),
            200,
            "GET {path} must be 200"
        );
        let content_type = image_response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        assert_eq!(
            content_type, "image/jpeg",
            "GET {path} must be served as image/jpeg, got {content_type:?}"
        );
    }
}

/// (b) Uploading a new image makes it appear in the list. Uses the real 12
/// MP fixture T0.7 committed (`tests/fixtures/photo_12mp.jpg`) so this also
/// proves the shared re-encode pipeline (`api::photos::sniff_downscale_reencode`,
/// reused from T2.5) handles a real photo — POSTed as `multipart/form-data`
/// to the raised-limit route, exactly the shape `tests/photo_tests.rs`
/// exercises against `/api/upload_photo`.
#[tokio::test]
async fn uploading_a_new_image_makes_it_appear_in_the_list() {
    let addr = spawn_test_server().await;
    let client = http_client();

    let before_body = client
        .post(format!("http://{addr}{LIST_ENDPOINT}"))
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .await
        .expect("list responds")
        .text()
        .await
        .expect("list body");
    let before: Vec<String> = serde_json::from_str(&before_body).expect("list is a JSON array");

    let fixture_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/photo_12mp.jpg");
    let raw = std::fs::read(&fixture_path).expect("photo_12mp.jpg fixture is readable");

    let upload_response = post_multipart(
        addr,
        UPLOAD_ENDPOINT,
        &[
            text_part("auth", parent_token()),
            file_part("photo", "photo_12mp.jpg", "image/jpeg", raw),
        ],
    )
    .await;

    assert_eq!(
        upload_response.status().as_u16(),
        200,
        "upload must succeed, body was {:?}",
        upload_response.text().await
    );

    let after: Vec<String> =
        serde_json::from_str(&upload_response.text().await.expect("upload response body"))
            .expect("upload_screensaver_image_handler returns the refreshed JSON array");

    assert_eq!(
        after.len(),
        before.len() + 1,
        "the list must grow by exactly one entry: before {before:?}, after {after:?}"
    );
    let new_entries: Vec<&String> = after.iter().filter(|path| !before.contains(path)).collect();
    assert_eq!(
        new_entries.len(),
        1,
        "exactly one new image must appear: before {before:?}, after {after:?}"
    );

    // And it really is fetchable as a JPEG, same as the seeded placeholders.
    let new_path = new_entries[0];
    let fetched = client
        .get(format!("http://{addr}{new_path}"))
        .send()
        .await
        .unwrap_or_else(|err| panic!("GET {new_path} should respond: {err}"));
    assert_eq!(fetched.status().as_u16(), 200);
    assert_eq!(
        fetched
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok()),
        Some("image/jpeg")
    );
}

/// A payload whose real bytes are not an image at all — the allowlist must
/// reject it before anything is written to disk (R-23c spirit), regardless
/// of what a client claims about it.
#[tokio::test]
async fn a_non_image_payload_is_rejected_and_nothing_is_added() {
    let addr = spawn_test_server().await;
    let client = http_client();

    let before_body = client
        .post(format!("http://{addr}{LIST_ENDPOINT}"))
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .await
        .expect("list responds")
        .text()
        .await
        .expect("list body");
    let before: Vec<String> = serde_json::from_str(&before_body).expect("list is a JSON array");

    let not_an_image = b"<svg xmlns=\"http://www.w3.org/2000/svg\"><script>alert(1)</script></svg>";
    let upload_response = post_multipart(
        addr,
        UPLOAD_ENDPOINT,
        &[
            text_part("auth", parent_token()),
            file_part("photo", "x.svg", "image/svg+xml", not_an_image.to_vec()),
        ],
    )
    .await;

    assert_ne!(
        upload_response.status().as_u16(),
        200,
        "a non-image payload must not be accepted as 200"
    );

    let after_body = client
        .post(format!("http://{addr}{LIST_ENDPOINT}"))
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .await
        .expect("list responds")
        .text()
        .await
        .expect("list body");
    let after: Vec<String> = serde_json::from_str(&after_body).expect("list is a JSON array");

    assert_eq!(
        after, before,
        "a rejected upload must not change the screensaver list"
    );
}

/// Reusing T2.5's pipeline includes reusing its headers (`docs/HANDOFF.md`
/// "T2.7 → T2.5, restated"): `/assets/screensaver` now carries the same
/// `nosniff`/`attachment` belt-and-braces `/uploads` gets (PURPLE §P3
/// T2.5(e), R-23c), on every response including the seeded placeholders.
#[tokio::test]
async fn screensaver_images_are_served_with_nosniff_and_attachment() {
    let addr = spawn_test_server().await;
    let client = http_client();

    let body = client
        .post(format!("http://{addr}{LIST_ENDPOINT}"))
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .await
        .expect("list responds")
        .text()
        .await
        .expect("list body");
    let images: Vec<String> = serde_json::from_str(&body).expect("list is a JSON array");
    let path = images.first().expect("at least one seeded image");

    let fetched = client
        .get(format!("http://{addr}{path}"))
        .send()
        .await
        .unwrap_or_else(|err| panic!("GET {path} should respond: {err}"));
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

/// Q1-07's twin of `tests/photo_tests.rs`'s
/// `t2_5_g_an_upload_without_a_parent_session_is_401_and_writes_nothing`:
/// before Q1-07, any LAN client could fill `screensaver/` with unbounded
/// (up to 25 MiB) unauthenticated posts. With no `auth` field at all, the
/// upload must 401 and the list must be unchanged.
#[tokio::test]
async fn an_upload_without_a_parent_session_is_401_and_writes_nothing() {
    let addr = spawn_test_server().await;
    let client = http_client();

    let before_body = client
        .post(format!("http://{addr}{LIST_ENDPOINT}"))
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .await
        .expect("list responds")
        .text()
        .await
        .expect("list body");
    let before: Vec<String> = serde_json::from_str(&before_body).expect("list is a JSON array");

    let upload_response = post_multipart(
        addr,
        UPLOAD_ENDPOINT,
        &[file_part("photo", "photo.jpg", "image/jpeg", vec![1, 2, 3])],
    )
    .await;

    assert_eq!(
        upload_response.status(),
        reqwest::StatusCode::UNAUTHORIZED,
        "an unauthenticated screensaver upload must 401"
    );

    let after_body = client
        .post(format!("http://{addr}{LIST_ENDPOINT}"))
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .await
        .expect("list responds")
        .text()
        .await
        .expect("list body");
    let after: Vec<String> = serde_json::from_str(&after_body).expect("list is a JSON array");

    assert_eq!(
        after, before,
        "an unauthenticated upload must not change the screensaver list"
    );
}
