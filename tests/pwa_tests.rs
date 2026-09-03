//! **T2.2 acceptance suite** — the phone PWA.
//!
//! Normative contract: `docs/reviews/PURPLE_TEAM.md` §P3, row T2.2, five
//! lettered assertions, reproduced here as the section headings below.
//! **No Lighthouse** — every claim about installability and offline
//! behaviour is proved by an HTTP response or a pure-Rust unit of logic.
//!
//! The HTTP half boots the real `build_router` behind an ephemeral-port
//! listener and drives it with `reqwest`, exactly like `tests/router_tests.rs`
//! and `tests/http_tests.rs`, so what is asserted is what a phone would
//! actually receive.

#![cfg(feature = "server")]

use std::collections::HashSet;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use family_calendar::client::components::mobile::pwa;
use family_calendar::client::components::mobile::queue::{
    OfflineQueue, QueueToast, QueuedMutation, QueuedMutationEntry, MAX_AGE_MS,
};
use family_calendar::client::components::mobile::MobileTab;
use family_calendar::server::config::FamilyHubConfig;
use family_calendar::server::db;
use family_calendar::server::router::build_router;

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

fn init_test_env() -> PathBuf {
    static ONCE: std::sync::Once = std::sync::Once::new();
    let base = std::env::temp_dir().join(format!("familyhub-pwa-tests-{}", std::process::id()));
    ONCE.call_once(|| {
        // Windows reuses PIDs: wipe any leftover scratch dir from an earlier run first.
        let _ = std::fs::remove_dir_all(&base);
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
    });
    base
}

fn test_config() -> FamilyHubConfig {
    FamilyHubConfig {
        data_dir: init_test_env(),
        http_addr: "127.0.0.1:0".parse().expect("valid socket address"),
        tls_addr: "127.0.0.1:0".parse().expect("valid socket address"),
        screensaver_schedule_hour: None,
        log_level: None,
    }
}

async fn spawn_router() -> SocketAddr {
    let config = test_config();
    db::pool().await.expect("test sqlite pool opens");
    let router = build_router(&config);
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

fn content_type(response: &reqwest::Response) -> String {
    response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string()
}

// ---------------------------------------------------------------------------
// (a) GET /manifest.webmanifest
// ---------------------------------------------------------------------------

/// T2.2 (a): 200, `application/manifest+json`, parses, `scope == "/"`,
/// `start_url == "/m"`, `display == "standalone"`, at least two icons of
/// which at least one has a `purpose` containing `maskable`.
#[tokio::test]
async fn manifest_is_served_from_root_with_the_fields_an_install_requires() {
    let addr = spawn_router().await;

    let response = http_client()
        .get(format!("http://{addr}/manifest.webmanifest"))
        .send()
        .await
        .expect("GET /manifest.webmanifest should respond");

    assert_eq!(response.status().as_u16(), 200);
    let ct = content_type(&response);
    assert!(
        ct.contains("application/manifest+json"),
        "expected application/manifest+json, got {ct:?}"
    );

    let body = response.text().await.expect("response body");
    let manifest: serde_json::Value =
        serde_json::from_str(&body).expect("the manifest must parse as JSON");

    assert_eq!(manifest["scope"], "/", "scope must be the whole origin");
    assert_eq!(
        manifest["start_url"], "/m",
        "start_url must be the phone URL"
    );
    assert_eq!(manifest["display"], "standalone");

    let icons = manifest["icons"]
        .as_array()
        .expect("the manifest must list icons");
    assert!(
        icons.len() >= 2,
        "expected at least 2 icons, got {}",
        icons.len()
    );
    assert!(
        icons.iter().any(|icon| icon["purpose"]
            .as_str()
            .is_some_and(|purpose| purpose.contains("maskable"))),
        "at least one icon must declare purpose maskable"
    );
}

/// A manifest that lists icons nobody can fetch is the same defect as a
/// manifest with `icons: []` (G6). Every `src` must really answer 200 with a
/// PNG.
#[tokio::test]
async fn every_icon_the_manifest_lists_is_actually_served() {
    let addr = spawn_router().await;

    let manifest: serde_json::Value =
        serde_json::from_str(pwa::MANIFEST_JSON).expect("the manifest must parse as JSON");
    let icons = manifest["icons"].as_array().expect("icons array");

    for icon in icons {
        let src = icon["src"].as_str().expect("every icon has a src");
        let response = http_client()
            .get(format!("http://{addr}{src}"))
            .send()
            .await
            .unwrap_or_else(|err| panic!("GET {src} should respond: {err}"));
        assert_eq!(response.status().as_u16(), 200, "{src} must be served");
        assert_eq!(content_type(&response), "image/png", "{src} must be a PNG");
        let bytes = response.bytes().await.expect("icon body");
        assert!(
            bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
            "{src} must really be PNG bytes"
        );
    }
}

/// Traversal guard on the `/icons/{file}` route: it is a fixed-table lookup,
/// never a filesystem read.
#[tokio::test]
async fn an_unknown_icon_name_is_a_404_and_not_a_file_read() {
    let addr = spawn_router().await;

    for name in ["nope.png", "..%2F..%2FCargo.toml"] {
        let response = http_client()
            .get(format!("http://{addr}/icons/{name}"))
            .send()
            .await
            .expect("the icon route should respond");
        assert_eq!(
            response.status().as_u16(),
            404,
            "/icons/{name} must not resolve"
        );
    }
}

// ---------------------------------------------------------------------------
// (b) GET /sw.js
// ---------------------------------------------------------------------------

/// T2.2 (b): 200, `text/javascript`, body ≤ 6 KB, containing `install`,
/// `activate` and `fetch` listeners.
#[tokio::test]
async fn service_worker_is_served_from_root_small_and_with_all_three_listeners() {
    let addr = spawn_router().await;

    let response = http_client()
        .get(format!("http://{addr}/sw.js"))
        .send()
        .await
        .expect("GET /sw.js should respond");

    assert_eq!(response.status().as_u16(), 200);
    let ct = content_type(&response);
    assert!(
        ct.contains("text/javascript"),
        "a service worker must be served as JavaScript, got {ct:?}"
    );

    let body = response.text().await.expect("response body");
    assert!(
        body.len() <= 6 * 1024,
        "sw.js is {} bytes, over the 6 KB budget",
        body.len()
    );

    for event in ["install", "activate", "fetch"] {
        assert!(
            body.contains(&format!("addEventListener('{event}'")),
            "sw.js must register a {event} listener"
        );
    }

    // The three caching rules D6 names, each visible in the shipped script.
    assert!(body.contains("'/m'"), "the app shell must be precached");
    assert!(
        body.contains("/api/"),
        "server functions must be routed network-first"
    );
    assert!(
        body.contains("/uploads/") && body.contains("cacheFirst"),
        "uploads must be routed cache-first"
    );
}

/// The script served over HTTP is byte-for-byte the one compiled into the
/// binary by `include_str!` (PLAN v2 D6).
#[tokio::test]
async fn the_served_service_worker_is_the_included_file() {
    let addr = spawn_router().await;
    let body = http_client()
        .get(format!("http://{addr}/sw.js"))
        .send()
        .await
        .expect("GET /sw.js should respond")
        .text()
        .await
        .expect("response body");
    assert_eq!(body, pwa::SERVICE_WORKER_JS);
}

// ---------------------------------------------------------------------------
// (c) start_url inside scope; no content hash on either path
// ---------------------------------------------------------------------------

/// T2.2 (c) — the R-16 regression guard, as a pure `#[test]`.
#[test]
fn start_url_is_inside_scope_and_neither_path_carries_a_content_hash() {
    let manifest: serde_json::Value =
        serde_json::from_str(pwa::MANIFEST_JSON).expect("the manifest must parse as JSON");
    let scope = manifest["scope"].as_str().expect("scope is a string");
    let start_url = manifest["start_url"]
        .as_str()
        .expect("start_url is a string");

    assert!(
        pwa::path_is_within_scope(scope, start_url),
        "start_url {start_url:?} is outside scope {scope:?} — the install prompt will never appear (R-16)"
    );

    for path in [pwa::MANIFEST_PATH, pwa::SERVICE_WORKER_PATH] {
        assert!(
            !pwa::contains_content_hash(path),
            "{path} looks like a hashed asset URL; the manifest and the service worker must be served from stable root paths (G6)"
        );
        assert!(
            !path.contains("/assets/"),
            "{path} must not be served through the asset pipeline"
        );
        assert_eq!(
            path.matches('/').count(),
            1,
            "{path} must be a root path so the service worker's default scope is /"
        );
    }
}

/// The same guard one level up: the page's own `<link rel="manifest">` must
/// point at the root URL, because that link is what the browser actually
/// follows. This is the assertion that would have caught G6 in v1, where the
/// link went through `asset!()`.
#[tokio::test]
async fn the_phone_page_links_the_manifest_at_its_root_url() {
    let addr = spawn_router().await;

    let body = http_client()
        .get(format!("http://{addr}/m"))
        .header("accept", "text/html")
        .send()
        .await
        .expect("GET /m should respond")
        .text()
        .await
        .expect("response body");

    assert!(
        body.contains(r#"href="/manifest.webmanifest""#),
        "/m must link the manifest at its root URL"
    );
    assert!(
        !body.contains("manifest.json"),
        "the hashed v1 asset manifest must be gone"
    );
}

/// The source tree itself must not reintroduce the hashed manifest.
#[test]
fn no_source_file_routes_the_manifest_through_the_asset_pipeline() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("src is readable") {
            let path = entry.expect("a readable directory entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let source = std::fs::read_to_string(&path).expect("a readable source file");
                assert!(
                    !source.contains("asset!(\"/assets/manifest"),
                    "{} routes the manifest through asset!() again (G6/R-16)",
                    path.display()
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// (d) The offline mutation queue
// ---------------------------------------------------------------------------

/// A stand-in for the hub that behaves like the real one in the only way that
/// matters here: it records every call, and it applies a mutation **once per
/// idempotency key** — which is exactly what `db::claim_mutation` (T1.5)
/// does server-side.
#[derive(Default)]
struct FakeHub {
    calls: Mutex<Vec<QueuedMutationEntry>>,
    applied: Mutex<HashSet<String>>,
    offline: Mutex<bool>,
}

impl FakeHub {
    fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn set_offline(&self, offline: bool) {
        *self.offline.lock().expect("lock") = offline;
    }

    fn call_count(&self) -> usize {
        self.calls.lock().expect("lock").len()
    }

    /// How many mutations actually took effect — distinct claimed keys.
    fn effect_count(&self) -> usize {
        self.applied.lock().expect("lock").len()
    }

    fn deliver(&self, entry: QueuedMutationEntry) -> Result<(), String> {
        if *self.offline.lock().expect("lock") {
            return Err("network unreachable".to_string());
        }
        self.calls.lock().expect("lock").push(entry.clone());
        self.applied.lock().expect("lock").insert(entry.key);
        Ok(())
    }
}

fn three_offline_ticks(queue: &mut OfflineQueue, now_ms: i64) {
    queue.enqueue(
        QueuedMutation::ToggleRoutineTask {
            user_id: 1,
            template_id: 1,
            completed: true,
        },
        "2026-08-29",
        now_ms,
    );
    queue.enqueue(
        QueuedMutation::ToggleRoutineTask {
            user_id: 1,
            template_id: 2,
            completed: true,
        },
        "2026-08-29",
        now_ms + 1_000,
    );
    // Made just after midnight, for *yesterday* — the case R-15 exists for.
    queue.enqueue(
        QueuedMutation::ToggleCustomTask {
            user_id: 2,
            task_id: 9,
            completed: true,
        },
        "2026-08-28",
        now_ms + 2_000,
    );
}

/// T2.2 (d) part 1: enqueue while offline → three entries, distinct
/// idempotency keys, each stamped with the date it was intended for.
#[test]
fn three_offline_mutations_queue_with_distinct_keys_and_their_own_dates() {
    let mut queue = OfflineQueue::new();
    three_offline_ticks(&mut queue, 1_700_000_000_000);

    assert_eq!(queue.len(), 3);

    let keys: HashSet<&str> = queue.entries().iter().map(|e| e.key.as_str()).collect();
    assert_eq!(keys.len(), 3, "every queued mutation needs its own key");
    assert!(
        queue.entries().iter().all(|e| !e.key.is_empty()),
        "an empty idempotency key would let the server apply a replay twice"
    );

    let dates: Vec<&str> = queue.entries().iter().map(|e| e.date.as_str()).collect();
    assert_eq!(
        dates,
        vec!["2026-08-29", "2026-08-29", "2026-08-28"],
        "each mutation keeps the date it was made for, not the date it is sent"
    );
}

/// T2.2 (d) part 2 and 3: replaying sends three calls and empties the queue;
/// replaying the same three entries a second time still produces only three
/// *effects*, because the keys are minted once and never regenerated.
#[tokio::test]
async fn replay_sends_every_entry_once_and_a_second_replay_changes_nothing() {
    let hub = FakeHub::new();
    let now = 1_700_000_000_000i64;

    let mut queue = OfflineQueue::new();
    hub.set_offline(true);
    three_offline_ticks(&mut queue, now);
    assert_eq!(hub.call_count(), 0, "nothing reaches an offline hub");

    // Keep a copy of exactly what was queued, so the second replay is a
    // genuine redelivery of the same entries (a retried request, an iOS app
    // opened twice, a second tab) rather than three new mutations.
    let redelivery: Vec<QueuedMutationEntry> = queue.entries().to_vec();

    hub.set_offline(false);
    let report = {
        let hub = hub.clone();
        queue
            .replay(now, move |entry| {
                let hub = hub.clone();
                async move { hub.deliver(entry) }
            })
            .await
    };

    assert_eq!(report.sent, 3, "all three must be delivered");
    assert_eq!(report.remaining, 0);
    assert!(queue.is_empty(), "a fully replayed queue is emptied");
    assert_eq!(hub.call_count(), 3, "three server calls");
    assert_eq!(hub.effect_count(), 3, "three effects");

    // Second replay of the very same entries.
    let mut again = OfflineQueue::new();
    for entry in redelivery {
        again.push(entry);
    }
    let second = {
        let hub = hub.clone();
        again
            .replay(now, move |entry| {
                let hub = hub.clone();
                async move { hub.deliver(entry) }
            })
            .await
    };

    assert_eq!(second.sent, 3);
    assert_eq!(hub.call_count(), 6, "the second delivery really happened");
    assert_eq!(
        hub.effect_count(),
        3,
        "replaying twice must still produce three effects — the idempotency keys are unchanged"
    );
}

/// A replay that fails partway keeps the rest of the queue, in order, for the
/// next attempt. Without this, one flaky send would drop everything behind it.
#[tokio::test]
async fn a_failed_send_stops_the_replay_and_keeps_the_remainder_in_order() {
    let now = 1_700_000_000_000i64;
    let mut queue = OfflineQueue::new();
    three_offline_ticks(&mut queue, now);
    let expected_tail: Vec<String> = queue.entries()[1..]
        .iter()
        .map(|entry| entry.key.clone())
        .collect();

    let attempts = Arc::new(Mutex::new(0usize));
    let report = {
        let attempts = attempts.clone();
        queue
            .replay(now, move |_entry| {
                let attempts = attempts.clone();
                async move {
                    let mut n = attempts.lock().expect("lock");
                    *n += 1;
                    if *n == 1 {
                        Ok(())
                    } else {
                        Err("network unreachable".to_string())
                    }
                }
            })
            .await
    };

    assert_eq!(report.sent, 1);
    assert_eq!(report.remaining, 2);
    assert_eq!(
        *attempts.lock().expect("lock"),
        2,
        "it stops at the failure"
    );
    let tail: Vec<String> = queue
        .entries()
        .iter()
        .map(|entry| entry.key.clone())
        .collect();
    assert_eq!(
        tail, expected_tail,
        "order is preserved for the next attempt"
    );
    assert!(
        report
            .toasts
            .iter()
            .any(|toast| matches!(toast, QueueToast::ReplayFailed { remaining: 2, .. })),
        "the family is told two changes are still waiting: {:?}",
        report.toasts
    );
}

/// T2.2 (d) part 4: an entry older than 48 hours is dropped, and dropping it
/// raises a toast event rather than happening silently.
#[tokio::test]
async fn an_entry_older_than_forty_eight_hours_is_dropped_with_a_toast() {
    let now = 1_700_000_000_000i64;
    let stale_at = now - MAX_AGE_MS - 1;

    let mut queue = OfflineQueue::new();
    queue.enqueue(
        QueuedMutation::ToggleRoutineTask {
            user_id: 1,
            template_id: 1,
            completed: true,
        },
        "2026-08-26",
        stale_at,
    );
    queue.enqueue(
        QueuedMutation::ToggleRoutineTask {
            user_id: 1,
            template_id: 2,
            completed: true,
        },
        "2026-08-29",
        now,
    );

    // The expiry rule on its own, before any network is consulted.
    let mut expiring = queue.clone();
    let toast = expiring
        .expire(now)
        .expect("dropping an entry must raise a toast event");
    assert_eq!(
        toast,
        QueueToast::Expired {
            count: 1,
            dates: vec!["2026-08-26".to_string()],
        }
    );
    assert!(
        toast.is_expiry() && toast.message().contains("2026-08-26"),
        "the toast must name the day whose change was discarded: {}",
        toast.message()
    );
    assert_eq!(expiring.len(), 1, "the fresh entry survives");

    // And through a real replay: the stale entry is never sent.
    let hub = FakeHub::new();
    let report = {
        let hub = hub.clone();
        queue
            .replay(now, move |entry| {
                let hub = hub.clone();
                async move { hub.deliver(entry) }
            })
            .await
    };

    assert_eq!(report.expired, 1);
    assert_eq!(report.sent, 1, "only the fresh mutation is sent");
    assert_eq!(hub.call_count(), 1);
    assert_eq!(
        hub.calls.lock().expect("lock")[0].date,
        "2026-08-29",
        "the expired mutation must never reach the hub"
    );
    assert!(
        report.toasts.iter().any(QueueToast::is_expiry),
        "the replay must surface the expiry toast: {:?}",
        report.toasts
    );
}

/// The expiry boundary itself: exactly 48 h old is kept, one millisecond past
/// it is dropped.
#[test]
fn the_expiry_boundary_is_exactly_forty_eight_hours() {
    let now = 1_700_000_000_000i64;

    let mut at_the_boundary = OfflineQueue::new();
    at_the_boundary.enqueue(
        QueuedMutation::ToggleRoutineTask {
            user_id: 1,
            template_id: 1,
            completed: true,
        },
        "2026-08-27",
        now - MAX_AGE_MS,
    );
    assert!(
        at_the_boundary.expire(now).is_none(),
        "an entry exactly 48 h old is still replayable"
    );
    assert_eq!(at_the_boundary.len(), 1);

    let mut past_it = OfflineQueue::new();
    past_it.enqueue(
        QueuedMutation::ToggleRoutineTask {
            user_id: 1,
            template_id: 1,
            completed: true,
        },
        "2026-08-27",
        now - MAX_AGE_MS - 1,
    );
    assert!(past_it.expire(now).is_some());
    assert!(past_it.is_empty());
}

/// The queue has to survive the PWA being closed, so it round-trips through
/// its `localStorage` payload with keys and dates intact.
#[test]
fn the_queue_survives_a_serialisation_round_trip_with_keys_and_dates_intact() {
    let mut queue = OfflineQueue::new();
    three_offline_ticks(&mut queue, 1_700_000_000_000);

    let restored = OfflineQueue::from_json(&queue.to_json());
    assert_eq!(restored.len(), 3);
    for (before, after) in queue.entries().iter().zip(restored.entries()) {
        assert_eq!(before.key, after.key);
        assert_eq!(before.date, after.date);
        assert_eq!(before.mutation, after.mutation);
    }
}

// ---------------------------------------------------------------------------
// The six bottom tabs
// ---------------------------------------------------------------------------

/// PLAN v2 §3 T2.2 asked for five tabs — "Routine · Calendar · Board · TV
/// Remote · Settings", closing G9 (`/mobile` was routine-only).
/// `docs/homeschool/PLAN_HOMESCHOOL.md` §2 H6 / §3 HS5 adds **School** as tab
/// 2 of 6 and relabels *TV Remote* to *Remote* so six columns fit at `text-xs`
/// without a new phone type size. HS5's own "Do" clause directs this test to
/// become the six-tab test in the new order; every other assertion in this
/// file is untouched.
#[test]
fn the_phone_has_the_six_bottom_tabs_the_plan_names() {
    let labels: Vec<&str> = MobileTab::ALL.iter().map(|tab| tab.label()).collect();
    assert_eq!(
        labels,
        vec!["Routine", "School", "Calendar", "Board", "Remote", "Settings"]
    );
}

// ---------------------------------------------------------------------------
// (e) docs/PWA.md — the per-platform offline promise
// ---------------------------------------------------------------------------

/// T2.2 (e): the doc states the per-platform promise. Android replays on
/// reconnect; **iOS replays on next app open** because it has no Background
/// Sync — the asymmetry RR-6 tracks, and the reason the queue is Rust in
/// `localStorage` rather than a Background Sync queue inside `sw.js`.
#[test]
fn the_pwa_doc_states_the_per_platform_offline_promise() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/PWA.md");
    let doc = std::fs::read_to_string(&path).expect("docs/PWA.md must exist and be readable");

    for required in [
        "## Android",
        "## iOS",
        "Background Sync",
        "next app open",
        "on reconnect",
        "48 hours",
        "idempotency key",
        "/manifest.webmanifest",
        "/sw.js",
    ] {
        assert!(
            doc.contains(required),
            "docs/PWA.md must state {required:?}"
        );
    }

    // The two promises must be attached to the right platform, not merely
    // both present somewhere in the file.
    let android = section(&doc, "## Android");
    let ios = section(&doc, "## iOS");
    assert!(
        android.contains("on reconnect"),
        "the Android section must promise replay on reconnect"
    );
    assert!(
        ios.contains("next app open"),
        "the iOS section must promise replay on next app open"
    );
    assert!(
        ios.contains("Background Sync"),
        "the iOS section must say why it differs: no Background Sync"
    );
}

/// The text of one `## ` section, up to the next one.
fn section<'a>(doc: &'a str, heading: &str) -> &'a str {
    let start = doc
        .find(heading)
        .unwrap_or_else(|| panic!("docs/PWA.md is missing {heading}"));
    let rest = &doc[start + heading.len()..];
    match rest.find("\n## ") {
        Some(end) => &rest[..end],
        None => rest,
    }
}
