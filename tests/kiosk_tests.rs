//! B-6 acceptance suite: Unlock / Lock the TV kiosk from a parent phone
//! (`docs/design/PLAN_KIOSK_REMOTE.md` §3).
//!
//! | § 3 | tests |
//! | --- | --- |
//! | K-A1 allow-list | unit tests in `src/server/api/kiosk.rs` |
//! | K-A2 protocol | [`k_a2_the_exact_query_reaches_fully_and_both_shapes_are_read`], [`k_a2_k_a5_failure_cases_classify_and_leak_nothing`] |
//! | K-A3 save + command | [`k_a3_an_empty_password_is_rejected_without_touching_the_tv`], [`k_a3_a_rejected_password_stores_nothing_and_an_accepted_one_stores_both`], [`k_a3_commands_go_through_the_public_fns_and_status_never_carries_the_password`], [`k_a3_two_parents_can_both_command`] |
//! | K-A4 gating | [`k_a4_every_fn_without_a_session_answers_not_signed_in_and_never_reaches_the_tv`], [`k_a4_the_cookie_gate_refuses_a_cross_site_request`] |
//! | K-A5 no leaks | [`k_a2_k_a5_failure_cases_classify_and_leak_nothing`] (+ `Secret`/clamp unit tests in `kiosk.rs`) |
//! | K-A6 SSR | `src/client/components/mobile/settings.rs` tests |
//!
//! **Never a real TV.** The fake Fully is an axum router on `127.0.0.1:0`;
//! the hub is aimed at it through the stored row (`db::set_setting`), the
//! one seam §2.1 allows — `KioskBase::parse` itself refuses loopback. No test
//! here sends a byte to any LAN address.
//!
//! Every test that touches the shared settings rows or the process-wide
//! Fully gate holds [`serial`] for its whole body.

#![cfg(feature = "server")]

use std::collections::HashMap;
use std::fmt::Write as _;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use family_calendar::server::api::kiosk::{
    self, cookie_gate, fully_command, load_stored, save_if_accepted, FullyCmd, FullyReply, Secret,
    KIOSK_PASSWORD_SETTING, KIOSK_URL_SETTING,
};
use family_calendar::server::{auth, db};
use family_calendar::shared::types::{KioskAction, KioskOutcome};

/// The password the fake Fully accepts. Distinctive, so a leak is greppable.
const FAKE_PASSWORD: &str = "Fake-Fully-Pa55word!";

/// Point every test in this binary at one throwaway sqlite file and data
/// directory (HS9; mirrors `tests/profiles_tests.rs::init_test_env`).
fn init_test_env() -> std::path::PathBuf {
    static ONCE: std::sync::Once = std::sync::Once::new();
    let base = std::env::temp_dir().join(format!("familyhub-kiosk-tests-{}", std::process::id()));
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
        std::env::set_var("FAMILY_HUB_DATA_DIR", &base);

        let public = base.join("public");
        std::fs::create_dir_all(&public).expect("test public directory is creatable");
        std::env::set_var("DIOXUS_PUBLIC_PATH", &public);
    });
    base
}

/// One test at a time over the shared rows and the Fully gate (a 5 s
/// unreachable case would otherwise make a parallel test answer `Busy`).
async fn serial() -> tokio::sync::MutexGuard<'static, ()> {
    static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    init_test_env();
    SERIAL.lock().await
}

// ---------------------------------------------------------------------------
// The fake Fully Kiosk Remote Admin
// ---------------------------------------------------------------------------

#[derive(Clone)]
enum Mode {
    /// Behaves like Fully: password check, `deviceInfo` property object,
    /// `unlockKiosk` / `lockKiosk` flip `kioskLocked`.
    Real,
    /// `200 text/html`, not JSON.
    NonJson,
    /// `302` to another URL (which must never be followed).
    Redirect(String),
    /// `{"status":"Error","statustext":<this>}`.
    StatusText(String),
}

struct Fake {
    hits: AtomicUsize,
    locked: AtomicBool,
    mode: Mutex<Mode>,
    last_query: Mutex<HashMap<String, String>>,
}

impl Fake {
    fn hits(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }

    fn set_mode(&self, mode: Mode) {
        *self.mode.lock().expect("mode lock") = mode;
    }

    fn last_query(&self) -> HashMap<String, String> {
        self.last_query.lock().expect("query lock").clone()
    }
}

async fn fake_fully(
    State(fake): State<Arc<Fake>>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    fake.hits.fetch_add(1, Ordering::SeqCst);
    *fake.last_query.lock().expect("query lock") = query.clone();
    let mode = fake.mode.lock().expect("mode lock").clone();
    let json = |value: serde_json::Value| axum::Json(value).into_response();
    match mode {
        Mode::NonJson => "<html><body>Fully Remote Admin</body></html>".into_response(),
        Mode::Redirect(to) => (
            axum::http::StatusCode::FOUND,
            [(axum::http::header::LOCATION, to)],
        )
            .into_response(),
        Mode::StatusText(text) => json(serde_json::json!({"status": "Error", "statustext": text})),
        Mode::Real => {
            if query.get("password").map(String::as_str) != Some(FAKE_PASSWORD) {
                return json(serde_json::json!({"status": "Error", "statustext": "Please login"}));
            }
            match query.get("cmd").map(String::as_str) {
                Some("deviceInfo") => json(serde_json::json!({
                    "deviceName": "Fake Fire TV",
                    "appVersionName": "1.61.2",
                    "kioskLocked": fake.locked.load(Ordering::SeqCst),
                })),
                Some("unlockKiosk") => {
                    fake.locked.store(false, Ordering::SeqCst);
                    json(serde_json::json!({"status": "OK", "statustext": "Unlocking kiosk"}))
                }
                Some("lockKiosk") => {
                    fake.locked.store(true, Ordering::SeqCst);
                    json(serde_json::json!({"status": "OK", "statustext": "Locking kiosk"}))
                }
                _ => json(serde_json::json!({"status": "Error", "statustext": "Unknown command"})),
            }
        }
    }
}

/// Start a fake Fully on `127.0.0.1:0`, kiosk locked, in [`Mode::Real`].
async fn spawn_fake() -> (SocketAddr, Arc<Fake>) {
    let fake = Arc::new(Fake {
        hits: AtomicUsize::new(0),
        locked: AtomicBool::new(true),
        mode: Mutex::new(Mode::Real),
        last_query: Mutex::new(HashMap::new()),
    });
    let router = axum::Router::new()
        .route("/", axum::routing::get(fake_fully))
        .with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind the fake Fully on loopback");
    let addr = listener.local_addr().expect("fake address");
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    (addr, fake)
}

/// Aim the hub at `addr` the only way a loopback address can get there: the
/// stored row (§2.1).
async fn seed_rows(addr: SocketAddr, password: &str) {
    let pool = db::pool().await.expect("test pool");
    db::set_setting(pool, KIOSK_URL_SETTING, &format!("http://{addr}"))
        .await
        .expect("seed url row");
    db::set_setting(pool, KIOSK_PASSWORD_SETTING, password)
        .await
        .expect("seed password row");
}

async fn clear_rows() {
    let pool = db::pool().await.expect("test pool");
    sqlx::query("DELETE FROM settings WHERE key IN (?1, ?2)")
        .bind(KIOSK_URL_SETTING)
        .bind(KIOSK_PASSWORD_SETTING)
        .execute(pool)
        .await
        .expect("clear kiosk rows");
}

async fn stored_base(addr: SocketAddr) -> kiosk::KioskBase {
    seed_rows(addr, FAKE_PASSWORD).await;
    let pool = db::read_pool().await.expect("read pool");
    load_stored(pool)
        .await
        .expect("read rows")
        .expect("seeded rows load")
        .0
}

// ---------------------------------------------------------------------------
// K-A2 — protocol
// ---------------------------------------------------------------------------

#[tokio::test]
async fn k_a2_the_exact_query_reaches_fully_and_both_shapes_are_read() {
    let _serial = serial().await;
    let (addr, fake) = spawn_fake().await;
    let base = stored_base(addr).await;
    let password = Secret::new(FAKE_PASSWORD);

    let reply = fully_command(&base, FullyCmd::UnlockKiosk, &password).await;
    assert_eq!(reply, FullyReply::Ok { locked: None });
    let query = fake.last_query();
    assert_eq!(query.get("cmd").map(String::as_str), Some("unlockKiosk"));
    assert_eq!(
        query.get("password").map(String::as_str),
        Some(FAKE_PASSWORD)
    );
    assert_eq!(query.get("type").map(String::as_str), Some("json"));
    assert_eq!(query.len(), 3, "exactly cmd, password, type: {query:?}");

    // The device-property object (no `status`) is an answer to deviceInfo,
    // and `locked` comes from `kioskLocked`.
    assert_eq!(
        fully_command(&base, FullyCmd::DeviceInfo, &password).await,
        FullyReply::Ok {
            locked: Some(false)
        }
    );
    assert_eq!(
        fake.last_query().get("cmd").map(String::as_str),
        Some("deviceInfo")
    );
    assert_eq!(
        fully_command(&base, FullyCmd::LockKiosk, &password).await,
        FullyReply::Ok { locked: None }
    );
    assert_eq!(
        fully_command(&base, FullyCmd::DeviceInfo, &password).await,
        FullyReply::Ok { locked: Some(true) }
    );

    // "Please login".
    assert_eq!(
        fully_command(&base, FullyCmd::DeviceInfo, &Secret::new("wrong")).await,
        FullyReply::PasswordRejected
    );
    clear_rows().await;
}

// ---------------------------------------------------------------------------
// K-A2 failure cases + K-A5 no leaks, under a capturing tracing subscriber
// ---------------------------------------------------------------------------

/// A minimal `tracing` subscriber that writes every event's and span's
/// fields into one string (no `tracing-subscriber` dev-dependency — no
/// `Cargo.toml` change, §1).
struct Capture(Arc<Mutex<String>>);

struct Fields<'a>(&'a mut String);

impl tracing::field::Visit for Fields<'_> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        let _ = write!(self.0, "{}={:?} ", field.name(), value);
    }
}

impl tracing::Subscriber for Capture {
    fn register_callsite(
        &self,
        _: &'static tracing::Metadata<'static>,
    ) -> tracing::subscriber::Interest {
        tracing::subscriber::Interest::always()
    }

    fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        let mut out = self.0.lock().expect("capture lock");
        let _ = write!(out, "span {} ", span.metadata().name());
        span.record(&mut Fields(&mut out));
        out.push('\n');
        tracing::span::Id::from_u64(1)
    }

    fn record(&self, _: &tracing::span::Id, values: &tracing::span::Record<'_>) {
        let mut out = self.0.lock().expect("capture lock");
        values.record(&mut Fields(&mut out));
        out.push('\n');
    }

    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}

    fn event(&self, event: &tracing::Event<'_>) {
        let mut out = self.0.lock().expect("capture lock");
        let _ = write!(out, "{} ", event.metadata().target());
        event.record(&mut Fields(&mut out));
        out.push('\n');
    }

    fn enter(&self, _: &tracing::span::Id) {}

    fn exit(&self, _: &tracing::span::Id) {}
}

#[tokio::test]
async fn k_a2_k_a5_failure_cases_classify_and_leak_nothing() {
    let _serial = serial().await;
    let captured = Arc::new(Mutex::new(String::new()));
    let _capture =
        tracing::dispatcher::set_default(&tracing::Dispatch::new(Capture(captured.clone())));
    let mut debug_strings = String::new();

    // Wrong password: Fully's "Please login".
    let (addr, fake) = spawn_fake().await;
    let base = stored_base(addr).await;
    let leaky = Secret::new(FAKE_PASSWORD);
    let reply = fully_command(&base, FullyCmd::DeviceInfo, &Secret::new("not-it")).await;
    assert_eq!(reply, FullyReply::PasswordRejected);
    let _ = write!(debug_strings, "{reply:?} {:?} ", kiosk::outcome_of(reply));

    // Non-JSON.
    fake.set_mode(Mode::NonJson);
    let reply = fully_command(&base, FullyCmd::DeviceInfo, &leaky).await;
    assert_eq!(reply, FullyReply::TvError);
    let _ = write!(debug_strings, "{reply:?} {:?} ", kiosk::outcome_of(reply));

    // A 300-char multibyte statustext is clamped for the log, never panics.
    fake.set_mode(Mode::StatusText("é".repeat(300)));
    let reply = fully_command(&base, FullyCmd::UnlockKiosk, &leaky).await;
    assert_eq!(reply, FullyReply::TvError);
    let _ = write!(debug_strings, "{reply:?} ");

    // A 302 is not followed: the redirect target sees zero hits.
    let (target_addr, target) = spawn_fake().await;
    fake.set_mode(Mode::Redirect(format!(
        "http://{target_addr}/?cmd=deviceInfo&password={FAKE_PASSWORD}&type=json"
    )));
    let reply = fully_command(&base, FullyCmd::DeviceInfo, &leaky).await;
    assert_eq!(reply, FullyReply::TvError, "a 302 is not a Fully answer");
    assert_eq!(target.hits(), 0, "the redirect must never be followed");
    let _ = write!(debug_strings, "{reply:?} ");

    // Accept-then-sleep: the TV took the connection and never answered.
    let silent = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind the silent listener");
    let silent_addr = silent.local_addr().expect("silent address");
    let holder = tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((socket, _)) = silent.accept().await {
            held.push(socket);
        }
    });
    seed_rows(silent_addr, FAKE_PASSWORD).await;
    let silent_base = load_stored(db::read_pool().await.expect("read pool"))
        .await
        .expect("read rows")
        .expect("seeded rows load")
        .0;
    let started = Instant::now();
    let reply = fully_command(&silent_base, FullyCmd::DeviceInfo, &leaky).await;
    let elapsed = started.elapsed();
    assert_eq!(reply, FullyReply::Unreachable);
    assert_eq!(kiosk::outcome_of(reply), KioskOutcome::TvUnreachable);
    assert!(
        elapsed < Duration::from_secs(6),
        "the 5 s budget must bound a silent TV, took {elapsed:?}"
    );
    holder.abort();
    let _ = write!(debug_strings, "{reply:?} {:?} ", silent_base);

    // Connection refused (nothing listening any more on the redirect target's
    // port is not guaranteed, so use a freshly closed listener).
    let closed = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind a port to close");
    let closed_addr = closed.local_addr().expect("closed address");
    drop(closed);
    seed_rows(closed_addr, FAKE_PASSWORD).await;
    let closed_base = load_stored(db::read_pool().await.expect("read pool"))
        .await
        .expect("read rows")
        .expect("seeded rows load")
        .0;
    let reply = fully_command(&closed_base, FullyCmd::DeviceInfo, &leaky).await;
    assert_eq!(reply, FullyReply::Unreachable);
    let _ = write!(debug_strings, "{reply:?} {leaky:?} {leaky} ");

    clear_rows().await;
    drop(_capture);

    let log = captured.lock().expect("capture lock").clone();
    assert!(
        log.contains("did not answer") && log.contains("unexpected shape"),
        "the failure cases must actually have been logged (else this proves nothing): {log}"
    );
    for haystack in [&log, &debug_strings] {
        assert!(
            !haystack.contains(FAKE_PASSWORD),
            "the password leaked: {haystack}"
        );
        assert!(
            !haystack.contains("http://") && !haystack.contains("password="),
            "a URL leaked: {haystack}"
        );
    }
    assert!(
        log.contains(&"é".repeat(200)) && !log.contains(&"é".repeat(201)),
        "statustext is echoed clamped to 200 chars: {log}"
    );
}

// ---------------------------------------------------------------------------
// K-A3 — save + command through the public fns
// ---------------------------------------------------------------------------

#[tokio::test]
async fn k_a3_an_empty_password_is_rejected_without_touching_the_tv() {
    let _serial = serial().await;
    clear_rows().await;
    let (addr, fake) = spawn_fake().await;
    let token = auth::issue_session();

    let outcome = kiosk::set_kiosk_admin(token, addr.to_string(), String::new())
        .await
        .expect("answers");
    assert_eq!(outcome, KioskOutcome::PasswordRejected);
    assert_eq!(fake.hits(), 0, "an empty password must never reach the TV");
    let pool = db::read_pool().await.expect("read pool");
    assert!(load_stored(pool).await.expect("read rows").is_none());
}

#[tokio::test]
async fn k_a3_a_typed_loopback_address_is_refused_before_any_request() {
    let _serial = serial().await;
    let (addr, fake) = spawn_fake().await;
    let token = auth::issue_session();

    let outcome = kiosk::set_kiosk_admin(token, addr.to_string(), FAKE_PASSWORD.to_string())
        .await
        .expect("answers");
    assert_eq!(outcome, KioskOutcome::BadAddress);
    assert_eq!(fake.hits(), 0);
}

/// The save half (`save_if_accepted`, what `set_kiosk_admin` runs after its
/// gate, empty-password check and allow-list): a rejected password stores
/// nothing; an accepted one stores both rows. Driven through the plain core
/// because the allow-list rightly refuses the loopback fake as a typed
/// address (`docs/HANDOFF.md` B-6).
#[tokio::test]
async fn k_a3_a_rejected_password_stores_nothing_and_an_accepted_one_stores_both() {
    let _serial = serial().await;
    let (addr, fake) = spawn_fake().await;
    let base = stored_base(addr).await;
    clear_rows().await;
    let pool = db::pool().await.expect("pool");

    let outcome = save_if_accepted(pool, &base, &Secret::new("wrong"))
        .await
        .expect("answers");
    assert_eq!(outcome, KioskOutcome::PasswordRejected);
    assert_eq!(fake.hits(), 1);
    assert_eq!(
        db::get_setting(pool, KIOSK_URL_SETTING).await.unwrap(),
        None
    );
    assert_eq!(
        db::get_setting(pool, KIOSK_PASSWORD_SETTING).await.unwrap(),
        None
    );

    let outcome = save_if_accepted(pool, &base, &Secret::new(FAKE_PASSWORD))
        .await
        .expect("answers");
    assert_eq!(outcome, KioskOutcome::Done { locked: Some(true) });
    assert_eq!(
        db::get_setting(pool, KIOSK_URL_SETTING).await.unwrap(),
        Some(format!("http://{addr}"))
    );
    assert_eq!(
        db::get_setting(pool, KIOSK_PASSWORD_SETTING)
            .await
            .unwrap()
            .as_deref(),
        Some(FAKE_PASSWORD)
    );
    clear_rows().await;
}

#[tokio::test]
async fn k_a3_commands_go_through_the_public_fns_and_status_never_carries_the_password() {
    let _serial = serial().await;
    let token = auth::issue_session();

    clear_rows().await;
    assert_eq!(
        kiosk::kiosk_command(token.clone(), KioskAction::Unlock)
            .await
            .expect("answers"),
        KioskOutcome::NotConfigured
    );
    let status = kiosk::kiosk_admin_status(token.clone())
        .await
        .expect("answers")
        .expect("signed in");
    assert!(!status.configured);

    let (addr, fake) = spawn_fake().await;
    seed_rows(addr, FAKE_PASSWORD).await;

    assert_eq!(
        kiosk::kiosk_command(token.clone(), KioskAction::Unlock)
            .await
            .expect("answers"),
        KioskOutcome::Done {
            locked: Some(false)
        }
    );
    assert!(!fake.locked.load(Ordering::SeqCst));
    assert_eq!(
        kiosk::kiosk_command(token.clone(), KioskAction::Lock)
            .await
            .expect("answers"),
        KioskOutcome::Done { locked: Some(true) }
    );
    assert!(fake.locked.load(Ordering::SeqCst));

    let status = kiosk::kiosk_admin_status(token.clone())
        .await
        .expect("answers");
    let json = serde_json::to_string(&status).expect("serialises");
    assert!(
        !json.contains(FAKE_PASSWORD),
        "status leaked the password: {json}"
    );
    let status = status.expect("signed in");
    assert!(status.configured);
    assert_eq!(status.address, format!("http://{addr}"));
    assert_eq!(status.locked, Some(true));

    // A wrong stored password surfaces as PasswordRejected.
    seed_rows(addr, "stale").await;
    assert_eq!(
        kiosk::kiosk_command(token, KioskAction::Unlock)
            .await
            .expect("answers"),
        KioskOutcome::PasswordRejected
    );
    clear_rows().await;
}

#[tokio::test]
async fn k_a3_two_parents_can_both_command() {
    let _serial = serial().await;
    let (addr, _fake) = spawn_fake().await;
    seed_rows(addr, FAKE_PASSWORD).await;
    let mum = auth::issue_session();
    let dad = auth::issue_session();
    assert_ne!(mum, dad);

    assert_eq!(
        kiosk::kiosk_command(mum, KioskAction::Unlock)
            .await
            .expect("answers"),
        KioskOutcome::Done {
            locked: Some(false)
        }
    );
    assert_eq!(
        kiosk::kiosk_command(dad, KioskAction::Lock)
            .await
            .expect("answers"),
        KioskOutcome::Done { locked: Some(true) }
    );
    clear_rows().await;
}

// ---------------------------------------------------------------------------
// K-A4 — gating
// ---------------------------------------------------------------------------

#[tokio::test]
async fn k_a4_every_fn_without_a_session_answers_not_signed_in_and_never_reaches_the_tv() {
    let _serial = serial().await;
    let (addr, fake) = spawn_fake().await;
    seed_rows(addr, FAKE_PASSWORD).await;

    for auth_token in [String::new(), "not-a-real-session".to_string()] {
        assert_eq!(
            kiosk::kiosk_admin_status(auth_token.clone())
                .await
                .expect("answers"),
            Err(KioskOutcome::NotSignedIn)
        );
        assert_eq!(
            kiosk::set_kiosk_admin(
                auth_token.clone(),
                addr.to_string(),
                FAKE_PASSWORD.to_string()
            )
            .await
            .expect("answers"),
            KioskOutcome::NotSignedIn
        );
        assert_eq!(
            kiosk::kiosk_command(auth_token.clone(), KioskAction::Unlock)
                .await
                .expect("answers"),
            KioskOutcome::NotSignedIn
        );
    }
    assert_eq!(fake.hits(), 0, "no session, no request to the TV");
    clear_rows().await;
}

fn headers(pairs: &[(&str, &str)]) -> axum::http::HeaderMap {
    let mut map = axum::http::HeaderMap::new();
    for (name, value) in pairs {
        map.insert(
            axum::http::HeaderName::from_bytes(name.as_bytes()).expect("header name"),
            value.parse().expect("header value"),
        );
    }
    map
}

#[test]
fn k_a4_the_cookie_gate_refuses_a_cross_site_request() {
    let token = auth::issue_session();
    let cookie = format!("{}={token}", auth::SESSION_COOKIE_NAME);
    let uri = axum::http::Uri::from_static("/api/kiosk_command");

    // Same-origin with a live cookie: allowed.
    assert!(cookie_gate(
        &headers(&[
            ("cookie", &cookie),
            ("host", "10.0.0.246:8443"),
            ("origin", "https://10.0.0.246:8443"),
            ("sec-fetch-site", "same-origin"),
        ]),
        &uri
    ));
    // Cross-site Fetch Metadata: refused even with a live cookie.
    assert!(!cookie_gate(
        &headers(&[("cookie", &cookie), ("sec-fetch-site", "cross-site")]),
        &uri
    ));
    // Foreign Origin: refused even with a live cookie.
    assert!(!cookie_gate(
        &headers(&[
            ("cookie", &cookie),
            ("host", "10.0.0.246:8443"),
            ("origin", "https://evil.example"),
        ]),
        &uri
    ));
    // Same-origin but no / a dead cookie: refused.
    assert!(!cookie_gate(
        &headers(&[("sec-fetch-site", "same-origin")]),
        &uri
    ));
    assert!(!cookie_gate(
        &headers(&[
            ("cookie", &format!("{}=nope", auth::SESSION_COOKIE_NAME)),
            ("sec-fetch-site", "same-origin"),
        ]),
        &uri
    ));
}
