//! Unlock / Lock the TV kiosk from a parent phone (B-6,
//! `docs/design/PLAN_KIOSK_REMOTE.md`).
//!
//! The television runs Fully Kiosk Browser (`docs/FIRE_TV.md` Branch A), whose
//! **Remote Admin** answers plain HTTP on `:2323`:
//! `GET http://<tv>:2323/?cmd=<cmd>&password=<pw>&type=json`. A phone cannot
//! call that itself — it is on HTTPS `:8443`, the TV is cross-origin plain
//! HTTP, and the password must never live on the phone — so the hub proxies:
//! phone → `#[server]` fn here → hub → TV (§1).
//!
//! | fn | does |
//! | --- | --- |
//! | [`kiosk_admin_status`] | gate → stored pair (read pool) → best-effort `deviceInfo` → [`KioskStatus`] (**never the password**) |
//! | [`set_kiosk_admin`] | gate → non-empty password → allow-list → `deviceInfo` → on acceptance, store both rows in one transaction |
//! | [`kiosk_command`] | gate → stored pair → `unlockKiosk` / `lockKiosk` → `deviceInfo` for `locked` |
//!
//! **The gate (red #4 / red #6).** Every fn starts with
//! [`super::profiles::require_session_or_cookie`]; on the cookie path
//! (`auth` empty, as the phone always sends it) it also requires
//! [`crate::server::auth::same_origin_or_absent`] on the live request —
//! these fns drive a physical device, so `SameSite=Lax` alone is not enough.
//! Failing the gate is the **answer** [`KioskOutcome::NotSignedIn`], not an
//! error, so the phone can drop to the sign-in form after a hub restart.
//!
//! **The plain core (red #1 / purple #2).** [`fully_command`],
//! [`KioskBase`], [`Secret`] and [`save_if_accepted`] are ordinary `pub`
//! items (precedent: `auth::read_setup_code`), so `tests/kiosk_tests.rs`
//! reaches them with no `cfg(test)` seam. A [`KioskBase`] can only come from
//! [`KioskBase::parse`] (the §2.2 allow-list) or from the stored row
//! ([`load_stored`]) — which is how a test aims the hub at a loopback fake.
//!
//! **Secrets hygiene (§2.3, red #9–11).** The password travels as a
//! [`Secret`] whose `Debug`/`Display` print `***`; every `reqwest::Error` is
//! stripped with `.without_url()` and only its *kind* is logged; no tracing
//! field ever holds a URL or a response body; Fully's `statustext` is
//! clamped by characters ([`clamp_text`]). The password is stored in plain
//! text in `settings` because it must be replayed to Fully — so nightly
//! backups (`src/server/backup.rs`) carry it too (`docs/FIRE_TV.md`).

use dioxus::prelude::*;

use crate::shared::types::{KioskAction, KioskOutcome, KioskStatus, SessionToken};

#[cfg(feature = "server")]
pub use fully::*;

/// The Fully Kiosk client core: allow-list, secret wrapper, HTTP call and
/// response classifier. Server-only — the phone never talks to the TV.
#[cfg(feature = "server")]
mod fully {
    use std::net::Ipv4Addr;
    use std::str::FromStr;
    use std::sync::OnceLock;
    use std::time::Duration;

    use sqlx::SqlitePool;
    use tokio::sync::{Mutex as AsyncMutex, MutexGuard};

    use crate::server::db;
    use crate::shared::types::KioskOutcome;

    /// `settings.key` of the normalised `http://a.b.c.d:port` (B-6).
    pub const KIOSK_URL_SETTING: &str = "kiosk_admin_url";
    /// `settings.key` of Fully's remote-admin password, in plain text — it
    /// must be replayed to Fully on every call (§2.1).
    pub const KIOSK_PASSWORD_SETTING: &str = "kiosk_admin_password";

    /// Fully's Remote Admin port, when the parent types a bare address.
    pub const DEFAULT_KIOSK_PORT: u16 = 2323;

    /// Whole-request budget (§2.1): long enough for a sleepy Fire TV, short
    /// enough that a phone tap never hangs.
    pub const FULLY_TOTAL_TIMEOUT: Duration = Duration::from_secs(5);
    /// TCP connect budget (§2.1).
    pub const FULLY_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
    /// How long a call waits for [`FULLY_GATE`] before answering
    /// [`FullyReply::Busy`] (red #7).
    pub const FULLY_LOCK_WAIT: Duration = Duration::from_secs(5);
    /// Body cap (§2.1): `deviceInfo` is a few KiB; anything past this is not
    /// a Fully reply worth parsing.
    pub const FULLY_BODY_CAP: usize = 64 * 1024;
    /// Character cap for any Fully text the hub echoes into its log (§2.3).
    pub const ECHO_CHAR_CAP: usize = 200;

    // -----------------------------------------------------------------------
    // Secret
    // -----------------------------------------------------------------------

    /// The Fully password (§2.3, red #9). `Debug` and `Display` both print
    /// `***`, so a stray `{:?}` in a log line or a panic message can never
    /// spill it. Only this module can read the value back.
    #[derive(Clone, PartialEq, Eq)]
    pub struct Secret(String);

    impl Secret {
        pub fn new(value: impl Into<String>) -> Self {
            Self(value.into())
        }

        pub fn is_empty(&self) -> bool {
            self.0.is_empty()
        }

        fn expose(&self) -> &str {
            &self.0
        }
    }

    impl std::fmt::Debug for Secret {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("***")
        }
    }

    impl std::fmt::Display for Secret {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("***")
        }
    }

    // -----------------------------------------------------------------------
    // Address allow-list (§2.2, red #3)
    // -----------------------------------------------------------------------

    /// Where Fully's Remote Admin lives. Constructible **only** through
    /// [`KioskBase::parse`] (the allow-list a parent's typed address must
    /// pass) or [`load_stored`] (the row `set_kiosk_admin` wrote after that
    /// allow-list passed). The URL is always rebuilt from the parsed
    /// `Ipv4Addr` and `u16` ([`KioskBase::url`]), never from typed text.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub struct KioskBase {
        ip: Ipv4Addr,
        port: u16,
    }

    impl KioskBase {
        /// The §2.2 allow-list. Hand-parsed, never via `url::Url`:
        ///
        /// * optional `http://` prefix stripped (so `https://…` keeps its
        ///   `/` and is refused);
        /// * any `/`, `?`, `#`, `@`, `%`, `[`, `]`, whitespace or non-ASCII
        ///   refused outright — no path, query, userinfo, zone id or IPv6;
        /// * split on the **last** `:`; the host parsed **only** by
        ///   `Ipv4Addr::from_str`, which refuses octal (`010.…`), hex
        ///   (`0x0a…`), bare decimal (`167772161`), short forms and a
        ///   trailing dot;
        /// * the host must be RFC 1918 (`Ipv4Addr::is_private`: 10/8,
        ///   172.16/12, 192.168/16) — never loopback, never public;
        /// * the port all ASCII digits (refuses `+80`), 1–65535, default
        ///   [`DEFAULT_KIOSK_PORT`].
        pub fn parse(input: &str) -> Option<Self> {
            let rest = input.strip_prefix("http://").unwrap_or(input);
            if rest.is_empty()
                || rest.chars().any(|c| {
                    !c.is_ascii_graphic() || matches!(c, '/' | '?' | '#' | '@' | '%' | '[' | ']')
                })
            {
                return None;
            }
            let (host, port) = match rest.rsplit_once(':') {
                Some((host, port)) => (host, parse_port(port)?),
                None => (rest, DEFAULT_KIOSK_PORT),
            };
            let ip = Ipv4Addr::from_str(host).ok()?;
            ip.is_private().then_some(Self { ip, port })
        }

        /// Read back a row this module wrote: exactly `http://<ipv4>:<port>`.
        /// Any IPv4 is accepted here — the row is only ever written after
        /// [`KioskBase::parse`] passed (or by someone who can already write
        /// the hub's database) — which is the seam the tests use to aim the
        /// hub at a loopback fake Fully (§2.1).
        pub(super) fn from_stored(row: &str) -> Option<Self> {
            let rest = row.strip_prefix("http://")?;
            let (host, port) = rest.rsplit_once(':')?;
            Some(Self {
                ip: Ipv4Addr::from_str(host).ok()?,
                port: parse_port(port)?,
            })
        }

        /// The normalised base URL, rebuilt from the parsed values.
        pub fn url(&self) -> String {
            format!("http://{}:{}", self.ip, self.port)
        }
    }

    fn parse_port(text: &str) -> Option<u16> {
        if text.is_empty() || text.len() > 5 || !text.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        text.parse::<u16>().ok().filter(|port| *port != 0)
    }

    // -----------------------------------------------------------------------
    // Stored pair
    // -----------------------------------------------------------------------

    /// The saved address + password, if both rows are present and the
    /// address row still parses. Callers pass `db::read_pool()` (H-9).
    pub async fn load_stored(
        pool: &SqlitePool,
    ) -> Result<Option<(KioskBase, Secret)>, sqlx::Error> {
        let Some(url) = db::get_setting(pool, KIOSK_URL_SETTING).await? else {
            return Ok(None);
        };
        let Some(password) = db::get_setting(pool, KIOSK_PASSWORD_SETTING).await? else {
            return Ok(None);
        };
        if password.is_empty() {
            return Ok(None);
        }
        Ok(KioskBase::from_stored(&url).map(|base| (base, Secret::new(password))))
    }

    /// Write both rows **together, in one transaction** (§2.1) — the same
    /// upsert `db::set_setting` runs, but on one `Transaction` so a parent
    /// can never end up with a new address beside an old password.
    async fn store_pair(
        pool: &SqlitePool,
        base: &KioskBase,
        password: &Secret,
    ) -> Result<(), sqlx::Error> {
        let mut tx = pool.begin().await?;
        for (key, value) in [
            (KIOSK_URL_SETTING, base.url()),
            (KIOSK_PASSWORD_SETTING, password.expose().to_string()),
        ] {
            sqlx::query(
                "INSERT INTO settings (key, value, updated_at) \
                 VALUES (?1, ?2, CURRENT_TIMESTAMP) \
                 ON CONFLICT (key) DO UPDATE SET \
                     value = excluded.value, updated_at = CURRENT_TIMESTAMP",
            )
            .bind(key)
            .bind(value)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await
    }

    /// The save half of `set_kiosk_admin`, after the gate, the empty-password
    /// check and the allow-list: ask Fully `deviceInfo` with **this**
    /// password, and only if Fully accepts it store the pair (one
    /// transaction, `db::pool()`). The stored password is never read here
    /// (red #2 — no replaying the saved password to a new address). Plain
    /// `pub` so `tests/kiosk_tests.rs` can drive it with a loopback base,
    /// which the allow-list (rightly) refuses to produce.
    pub async fn save_if_accepted(
        pool: &SqlitePool,
        base: &KioskBase,
        password: &Secret,
    ) -> Result<KioskOutcome, sqlx::Error> {
        if password.is_empty() {
            return Ok(KioskOutcome::PasswordRejected);
        }
        match fully_command(base, FullyCmd::DeviceInfo, password).await {
            FullyReply::Ok { locked } => {
                store_pair(pool, base, password).await?;
                tracing::info!("saved the TV's remote-admin address and password");
                Ok(KioskOutcome::Done { locked })
            }
            other => Ok(outcome_of(other)),
        }
    }

    // -----------------------------------------------------------------------
    // Fully protocol
    // -----------------------------------------------------------------------

    /// The three Fully Remote Admin commands B-6 sends. Anything else (screen
    /// on/off, load URL …) is out of scope (§5).
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum FullyCmd {
        DeviceInfo,
        UnlockKiosk,
        LockKiosk,
    }

    impl FullyCmd {
        pub fn as_str(self) -> &'static str {
            match self {
                Self::DeviceInfo => "deviceInfo",
                Self::UnlockKiosk => "unlockKiosk",
                Self::LockKiosk => "lockKiosk",
            }
        }
    }

    /// What one Fully call came back as — the hub-side vocabulary that
    /// [`outcome_of`] maps onto the phone's [`KioskOutcome`].
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum FullyReply {
        /// `status: "OK"`, or a `deviceInfo` object; `locked` from
        /// `kioskLocked` when Fully included it.
        Ok { locked: Option<bool> },
        /// `status: "Error"` with a login / password `statustext`.
        PasswordRejected,
        /// No answer: refused, timed out, reset.
        Unreachable,
        /// An answer, but not one this module recognises (incl. non-2xx).
        TvError,
        /// [`FULLY_GATE`] was not free within [`FULLY_LOCK_WAIT`].
        Busy,
    }

    /// `FullyReply` → the phone's outcome. `Ok` keeps its `locked`.
    pub fn outcome_of(reply: FullyReply) -> KioskOutcome {
        match reply {
            FullyReply::Ok { locked } => KioskOutcome::Done { locked },
            FullyReply::PasswordRejected => KioskOutcome::PasswordRejected,
            FullyReply::Unreachable => KioskOutcome::TvUnreachable,
            FullyReply::TvError => KioskOutcome::TvError,
            FullyReply::Busy => KioskOutcome::Busy,
        }
    }

    /// The §2.1 response classifier — pure, so both shapes Fully may send
    /// (`{"status":…}` and `deviceInfo`'s bare property object, red #5) are
    /// unit-tested without a TV.
    pub fn classify(cmd: FullyCmd, body: &str) -> FullyReply {
        let Ok(serde_json::Value::Object(object)) = serde_json::from_str::<serde_json::Value>(body)
        else {
            return FullyReply::TvError;
        };
        let locked = object.get("kioskLocked").and_then(|v| v.as_bool());
        match object.get("status").and_then(|v| v.as_str()) {
            Some("Error") => {
                let text = object
                    .get("statustext")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_lowercase();
                if text.contains("login") || text.contains("password") {
                    FullyReply::PasswordRejected
                } else {
                    FullyReply::TvError
                }
            }
            Some("OK") => FullyReply::Ok { locked },
            None if cmd == FullyCmd::DeviceInfo => FullyReply::Ok { locked },
            _ => FullyReply::TvError,
        }
    }

    /// Clamp text the hub echoes into its own log to [`ECHO_CHAR_CAP`]
    /// **characters** (red #8) — `chars().take`, never a byte slice, so a
    /// multibyte `statustext` cannot panic on a char boundary.
    pub fn clamp_text(text: &str) -> String {
        text.chars().take(ECHO_CHAR_CAP).collect()
    }

    /// Serialises every Fully call process-wide (red #7): two parents' phones
    /// or a double tap reach the TV one at a time. Same shape as
    /// `auth::PIN_GATE`.
    static FULLY_GATE: AsyncMutex<()> = AsyncMutex::const_new(());

    /// Take [`FULLY_GATE`], or `None` after [`FULLY_LOCK_WAIT`].
    pub(super) async fn take_gate() -> Option<MutexGuard<'static, ()>> {
        tokio::time::timeout(FULLY_LOCK_WAIT, FULLY_GATE.lock())
            .await
            .ok()
    }

    /// One client for every Fully call, built without `expect` (a build
    /// failure answers `TvError`). `.no_proxy()` (purple #5): an
    /// `HTTP_PROXY` in the service's environment must never receive the
    /// password. Redirects are **not** followed. Idle pooling is off: these
    /// calls are rare, and a pooled connection must not outlive the runtime
    /// that opened it (every `#[tokio::test]` has its own).
    static FULLY_CLIENT: OnceLock<Option<reqwest::Client>> = OnceLock::new();

    fn fully_client() -> Option<&'static reqwest::Client> {
        FULLY_CLIENT
            .get_or_init(|| {
                reqwest::Client::builder()
                    .timeout(FULLY_TOTAL_TIMEOUT)
                    .connect_timeout(FULLY_CONNECT_TIMEOUT)
                    .redirect(reqwest::redirect::Policy::none())
                    .no_proxy()
                    .pool_max_idle_per_host(0)
                    .build()
                    .map_err(|err| {
                        let err = err.without_url();
                        tracing::warn!(
                            kind = transport_kind(&err),
                            "could not build the Fully Kiosk HTTP client"
                        );
                    })
                    .ok()
            })
            .as_ref()
    }

    /// A URL-free, body-free label for a transport error — the only thing
    /// about a `reqwest::Error` this module ever logs (§2.3).
    fn transport_kind(err: &reqwest::Error) -> &'static str {
        if err.is_timeout() {
            "timeout"
        } else if err.is_connect() {
            "connect"
        } else if err.is_redirect() {
            "redirect"
        } else if err.is_body() || err.is_decode() {
            "body"
        } else if err.is_builder() {
            "builder"
        } else {
            "request"
        }
    }

    /// One Fully call **already holding [`FULLY_GATE`]** — what
    /// `kiosk_command` uses to send the command and its follow-up
    /// `deviceInfo` under a single lock.
    pub(super) async fn send_locked(
        base: &KioskBase,
        cmd: FullyCmd,
        password: &Secret,
    ) -> FullyReply {
        let Some(client) = fully_client() else {
            return FullyReply::TvError;
        };
        let command = cmd.as_str();
        let response = match client
            .get(format!("{}/", base.url()))
            .query(&[
                ("cmd", command),
                ("password", password.expose()),
                ("type", "json"),
            ])
            .send()
            .await
        {
            Ok(response) => response,
            Err(err) => {
                let err = err.without_url();
                tracing::warn!(
                    command,
                    kind = transport_kind(&err),
                    "the TV's remote admin did not answer"
                );
                return FullyReply::Unreachable;
            }
        };

        let status = response.status();
        if !status.is_success() {
            tracing::warn!(
                command,
                status = status.as_u16(),
                "the TV's remote admin answered with a non-success status"
            );
            return FullyReply::TvError;
        }

        let body = match read_capped(response).await {
            Ok(body) => body,
            Err(reply) => {
                tracing::warn!(command, "the TV's remote admin reply could not be read");
                return reply;
            }
        };

        let reply = classify(cmd, &body);
        match reply {
            FullyReply::TvError => tracing::warn!(
                command,
                statustext = %clamp_text(&statustext_of(&body)),
                "the TV's remote admin answered in an unexpected shape"
            ),
            FullyReply::PasswordRejected => {
                tracing::warn!(command, "the TV's remote admin refused the password")
            }
            _ => {}
        }
        reply
    }

    /// Read the body chunk by chunk, refusing anything past
    /// [`FULLY_BODY_CAP`] (§2.1's "`.text()`, capped at 64 KiB").
    async fn read_capped(mut response: reqwest::Response) -> Result<String, FullyReply> {
        let mut body: Vec<u8> = Vec::new();
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) => {
                    if body.len() + chunk.len() > FULLY_BODY_CAP {
                        return Err(FullyReply::TvError);
                    }
                    body.extend_from_slice(&chunk);
                }
                Ok(None) => return Ok(String::from_utf8_lossy(&body).into_owned()),
                Err(err) if err.is_timeout() => return Err(FullyReply::Unreachable),
                Err(_) => return Err(FullyReply::TvError),
            }
        }
    }

    /// Fully's `statustext`, if the body has one — only ever logged through
    /// [`clamp_text`].
    fn statustext_of(body: &str) -> String {
        serde_json::from_str::<serde_json::Value>(body)
            .ok()
            .and_then(|value| {
                value
                    .get("statustext")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            })
            .unwrap_or_default()
    }

    /// One Fully call through [`FULLY_GATE`] (red #7): `Busy` if another
    /// call holds it for longer than [`FULLY_LOCK_WAIT`].
    pub async fn fully_command(base: &KioskBase, cmd: FullyCmd, password: &Secret) -> FullyReply {
        let Some(_serial) = take_gate().await else {
            return FullyReply::Busy;
        };
        send_locked(base, cmd, password).await
    }

    /// The cookie half of the gate, on an explicit request (red #4): a valid
    /// `fh_session` cookie **and** a same-origin (or origin-less) request.
    /// What [`super::gate`] applies to the live request when the phone sends
    /// an empty `auth`; `pub` so the cross-site refusal is unit-tested.
    pub fn cookie_gate(headers: &axum::http::HeaderMap, uri: &axum::http::Uri) -> bool {
        let signed_in = crate::server::auth::session_from_headers(headers)
            .is_some_and(|token| crate::server::auth::is_valid_session(&token));
        signed_in && crate::server::auth::same_origin_or_absent(headers, uri)
    }
}

/// Every fn's first step (§2.1 "Gate"): `require_session_or_cookie`, and on
/// the cookie path the same-origin check against the live request. `false`
/// becomes [`KioskOutcome::NotSignedIn`]. A direct in-process call with an
/// empty `auth` has no request underneath it (`FullstackContext` hands back
/// a dummy one with no cookie), so it fails closed.
#[cfg(feature = "server")]
async fn gate(auth: &str) -> bool {
    use dioxus::prelude::dioxus_fullstack::FullstackContext;

    if super::profiles::require_session_or_cookie(auth)
        .await
        .is_err()
    {
        return false;
    }
    if !auth.is_empty() {
        return true;
    }
    let Ok(headers) = FullstackContext::extract::<axum::http::HeaderMap, _>().await else {
        return false;
    };
    let Ok(uri) = FullstackContext::extract::<axum::http::Uri, _>().await else {
        return false;
    };
    cookie_gate(&headers, &uri)
}

/// Settings' **TV kiosk** section: is a TV saved, where, and (best-effort)
/// is Fully's kiosk lock on? Never returns the password (§2.1).
///
/// `Ok(Err(KioskOutcome::NotSignedIn))` when the gate fails (red #6) — the
/// only `Err` this inner result ever carries; see `docs/HANDOFF.md` B-6.
#[server(endpoint = "kiosk_admin_status")]
pub async fn kiosk_admin_status(
    auth: SessionToken,
) -> Result<Result<KioskStatus, KioskOutcome>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        if !gate(&auth).await {
            return Ok(Err(KioskOutcome::NotSignedIn));
        }
        let pool = crate::server::db::read_pool()
            .await
            .map_err(super::to_server_error)?;
        let Some((base, password)) = load_stored(pool).await.map_err(super::to_server_error)?
        else {
            return Ok(Ok(KioskStatus {
                configured: false,
                address: String::new(),
                locked: None,
            }));
        };
        let locked = match fully_command(&base, FullyCmd::DeviceInfo, &password).await {
            FullyReply::Ok { locked } => locked,
            _ => None,
        };
        Ok(Ok(KioskStatus {
            configured: true,
            address: base.url(),
            locked,
        }))
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = auth;
        unreachable!("server function bodies only run on the server")
    }
}

/// Save the TV's remote-admin address and password — only once Fully has
/// accepted the pair (§2.1). The password must be non-empty and the stored
/// one is never read here (red #2); the address must pass the §2.2
/// allow-list.
#[server(endpoint = "set_kiosk_admin")]
pub async fn set_kiosk_admin(
    auth: SessionToken,
    address: String,
    password: String,
) -> Result<KioskOutcome, ServerFnError> {
    #[cfg(feature = "server")]
    {
        if !gate(&auth).await {
            return Ok(KioskOutcome::NotSignedIn);
        }
        let password = Secret::new(password);
        if password.is_empty() {
            return Ok(KioskOutcome::PasswordRejected);
        }
        let Some(base) = KioskBase::parse(&address) else {
            return Ok(KioskOutcome::BadAddress);
        };
        let pool = crate::server::db::pool()
            .await
            .map_err(super::to_server_error)?;
        save_if_accepted(pool, &base, &password)
            .await
            .map_err(super::to_server_error)
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (auth, address, password);
        unreachable!("server function bodies only run on the server")
    }
}

/// **Unlock Kiosk** / **Lock Kiosk**: replay the stored pair to Fully, then
/// ask `deviceInfo` for the resulting `locked` — both under one hold of the
/// process-wide Fully gate (red #7).
#[server(endpoint = "kiosk_command")]
pub async fn kiosk_command(
    auth: SessionToken,
    action: KioskAction,
) -> Result<KioskOutcome, ServerFnError> {
    #[cfg(feature = "server")]
    {
        if !gate(&auth).await {
            return Ok(KioskOutcome::NotSignedIn);
        }
        let pool = crate::server::db::read_pool()
            .await
            .map_err(super::to_server_error)?;
        let Some((base, password)) = load_stored(pool).await.map_err(super::to_server_error)?
        else {
            return Ok(KioskOutcome::NotConfigured);
        };
        let Some(_serial) = fully::take_gate().await else {
            return Ok(KioskOutcome::Busy);
        };
        let cmd = match action {
            KioskAction::Unlock => FullyCmd::UnlockKiosk,
            KioskAction::Lock => FullyCmd::LockKiosk,
        };
        match fully::send_locked(&base, cmd, &password).await {
            FullyReply::Ok { .. } => {
                tracing::info!(command = cmd.as_str(), "the TV accepted a kiosk command");
                let locked = match fully::send_locked(&base, FullyCmd::DeviceInfo, &password).await
                {
                    FullyReply::Ok { locked } => locked,
                    _ => None,
                };
                Ok(KioskOutcome::Done { locked })
            }
            other => Ok(outcome_of(other)),
        }
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (auth, action);
        unreachable!("server function bodies only run on the server")
    }
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;

    /// **K-A1** — every address §3 lists, accepted with its normalised form.
    #[test]
    fn the_allow_list_accepts_private_ipv4_and_normalises_it() {
        for (typed, normalised) in [
            ("10.0.0.178:2323", "http://10.0.0.178:2323"),
            ("http://10.0.0.178", "http://10.0.0.178:2323"),
            ("192.168.1.5:8080", "http://192.168.1.5:8080"),
            ("172.16.0.9", "http://172.16.0.9:2323"),
        ] {
            let base = KioskBase::parse(typed).unwrap_or_else(|| panic!("{typed:?} refused"));
            assert_eq!(base.url(), normalised, "{typed:?}");
        }
    }

    /// **K-A1** — every bypass red #3 named, refused.
    #[test]
    fn the_allow_list_refuses_everything_else() {
        for typed in [
            "https://10.0.0.178:2323",
            "localhost",
            "127.0.0.1",
            "8.8.8.8",
            "172.32.0.1",
            "example.com",
            "010.0.0.1",
            "0x0a.0.0.1",
            "167772161",
            "10.0.0.178.",
            "[::ffff:10.0.0.1]",
            "10.0.0.178:0",
            "10.0.0.178:+80",
            "10.0.0.178:70000",
            "10.0.0.178/home",
            "user@10.0.0.178",
            "10.0.0.178%25eth0",
            " 10.0.0.178",
            "",
        ] {
            assert_eq!(KioskBase::parse(typed), None, "{typed:?} must be refused");
        }
    }

    #[test]
    fn the_classifier_reads_both_fully_shapes() {
        assert_eq!(
            classify(
                FullyCmd::UnlockKiosk,
                r#"{"status":"OK","statustext":"Unlocking kiosk"}"#
            ),
            FullyReply::Ok { locked: None }
        );
        assert_eq!(
            classify(
                FullyCmd::DeviceInfo,
                r#"{"status":"Error","statustext":"Please login"}"#
            ),
            FullyReply::PasswordRejected
        );
        assert_eq!(
            classify(
                FullyCmd::DeviceInfo,
                r#"{"kioskLocked":true,"appVersionName":"1.61.2"}"#
            ),
            FullyReply::Ok { locked: Some(true) }
        );
        assert_eq!(
            classify(FullyCmd::DeviceInfo, r#"{"appVersionName":"1.61.2"}"#),
            FullyReply::Ok { locked: None }
        );
        // A bare property object is only an answer to `deviceInfo`.
        assert_eq!(
            classify(FullyCmd::LockKiosk, r#"{"kioskLocked":true}"#),
            FullyReply::TvError
        );
        assert_eq!(
            classify(
                FullyCmd::LockKiosk,
                r#"{"status":"Error","statustext":"No"}"#
            ),
            FullyReply::TvError
        );
        assert_eq!(
            classify(FullyCmd::DeviceInfo, "<html>hi</html>"),
            FullyReply::TvError
        );
        assert_eq!(classify(FullyCmd::DeviceInfo, "[1,2]"), FullyReply::TvError);
    }

    /// **K-A5** — `Secret` never prints itself; the clamp is char-safe.
    #[test]
    fn secrets_print_as_stars_and_the_clamp_is_char_safe() {
        let secret = Secret::new("hunter2");
        assert_eq!(format!("{secret:?}"), "***");
        assert_eq!(format!("{secret}"), "***");
        assert_eq!(format!("{:?}", Some(secret.clone())), "Some(***)");

        let long: String = "é".repeat(300);
        let clamped = clamp_text(&long);
        assert_eq!(clamped.chars().count(), ECHO_CHAR_CAP);
        assert_eq!(clamp_text("short"), "short");
    }

    #[test]
    fn a_stored_row_round_trips_but_only_in_its_exact_shape() {
        let base = KioskBase::parse("10.0.0.178").expect("allowed");
        assert_eq!(KioskBase::from_stored(&base.url()), Some(base));
        assert_eq!(KioskBase::from_stored("10.0.0.178:2323"), None);
        assert_eq!(KioskBase::from_stored("http://tv.local:2323"), None);
        assert_eq!(KioskBase::from_stored("http://10.0.0.178"), None);
    }
}
