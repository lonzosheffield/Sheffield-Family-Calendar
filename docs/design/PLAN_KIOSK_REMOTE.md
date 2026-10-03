# PLAN v2 — Unlock / Lock the TV kiosk from the phone's Settings (B-6)

**Owner ask (2026-10-03):** "Once the parent signs in, in the settings screen, I want the ability to
unlock kiosk mode. Right now I literally have to go to the remote admin location
(10.0.0.178:2323/home) … sign in with the password … then unlock kiosk mode. I want a shortcut
button that says **Unlock Kiosk**, just below *Offline & install*, shown once signed in as parent."

**Owner decisions (asked 2026-10-03):**
1. The Fully Kiosk admin password is **entered once on the phone** by a signed-in parent and kept by
   the hub (not a config file). The phone never receives it back.
2. Add **Lock Kiosk** beside **Unlock Kiosk**.

**v1 → v2:** folded the red / purple / white reviews of 2026-10-03 (summary in §6).

## 1. Facts this plan rests on

* The kiosk shell is **Fully Kiosk Browser 1.61.2 (PLUS)** on the Insignia (`docs/FIRE_TV.md`
  Branch A, `docs/device.toml` `tv.ip = 10.0.0.178`). Its **Remote Admin** on `:2323` is reachable
  from the hub PC (`curl http://10.0.0.178:2323/` → 200, title *Fully Remote Admin*).
* Fully REST: `GET http://<tv>:2323/?cmd=<cmd>&password=<pw>&type=json`. With no / a wrong
  password it answers `{"status":"Error","statustext":"Please login"}` (observed 2026-10-03).
  Commands: `deviceInfo` (read-only; returns a **device-property object, likely with no `status`
  field**), `unlockKiosk`, `lockKiosk` (`{"status":"OK",…}` on success). The response classifier
  (§2.1) is written to tolerate both shapes; the real bodies are recorded in `docs/FIRE_TV.md` at
  the on-device check (K-A7).
* Phones use **HTTPS :8443**; the TV admin is plain HTTP and cross-origin, so the **hub proxies**
  (phone → `#[server]` fn → hub → TV) and the password stays on the hub.
* Privileged fns take `auth: SessionToken` and call `require_session_or_cookie(&auth)` first
  (`src/server/api/profiles.rs:58`); the phone passes `String::new()` so the cookie is used.
  Settings rows go through `db::get_setting` / `db::set_setting` (`src/server/db.rs`), reads on
  `db::read_pool()` (H-9), writes on `db::pool()`.
* `reqwest 0.12` (rustls, `default-features = false`, server feature only) needs no new features.
  No `Cargo.toml` change, no migration.

## 2. Design

### 2.1 Server — `src/server/api/kiosk.rs` (new)

Shared wire types in `src/shared/types.rs` (WASM-safe, `Serialize, Deserialize, Clone, PartialEq,
Debug`): `KioskAction { Unlock, Lock }`, `KioskStatus { configured: bool, address: String,
locked: Option<bool> }`, `KioskOutcome { Done { locked: Option<bool> }, NotConfigured,
PasswordRejected, TvUnreachable, TvError, BadAddress, NotSignedIn, Busy }`. `TvError` carries **no**
Fully text (white #6) — the phone shows a fixed sentence.

Settings rows: `kiosk_admin_url` (normalised `http://a.b.c.d:port`) and `kiosk_admin_password`
(plaintext — it must be replayed to Fully). Written **together, in one transaction, only after Fully
accepted the pair**.

`#[server(endpoint = "<same name>")]` fns, bodies under `#[cfg(feature = "server")]`, the
non-server arm `let _ = (…); unreachable!(…)` as in `profiles.rs`:

| fn | does |
| --- | --- |
| `kiosk_admin_status(auth)` | gate → read rows (read pool) → if configured, best-effort `deviceInfo` to fill `locked` → `KioskStatus` (normalised address, **never the password**) |
| `set_kiosk_admin(auth, address, password)` | gate → **password must be non-empty; the stored password is never read here** (red #2: no replaying the saved password to a new address) → validate address → `deviceInfo` → on acceptance store both rows → `KioskOutcome` |
| `kiosk_command(auth, action)` | gate → read stored pair → `unlockKiosk` / `lockKiosk` → then `deviceInfo` to report `locked` → `KioskOutcome` |

**Gate** (every fn, before anything else): `require_session_or_cookie(&auth)`; on the cookie path
also extract `HeaderMap` + `Uri` from `FullstackContext` and require
`auth::same_origin_or_absent` (red #4 — these fns drive a physical device; today ordinary server fns
rely on `SameSite=Lax` alone). Auth failure → `KioskOutcome::NotSignedIn` (red #6), not an error, so
the phone can drop to the sign-in form after a hub restart.

**Plain, non-`#[server]` core** (precedent `auth::read_setup_code`, `auth.rs:240`) so
`tests/kiosk_tests.rs` can reach it without `cfg(test)` (red #1 / purple #2):
`pub async fn fully_command(base: &KioskBase, cmd: FullyCmd, password: &Secret) -> FullyReply`.
`KioskBase` is constructible **only** by `KioskBase::parse(&str)` (the allow-list) or by reading the
stored row — the `#[server]` fns validate at save time; `kiosk_command` replays the stored row, so a
test seeds a `127.0.0.1:<port>` row with `db::set_setting` and drives the **public** fn with a real
`auth::issue_session()` token.

**Response classifier:** body read with `.text()` (capped at 64 KiB), `serde_json::from_str`:
`status == "Error"` and `statustext` contains "login"/"password" (case-insensitive) →
`PasswordRejected`; `status == "OK"` → ok; for `deviceInfo`, a JSON object with no `status:"Error"`
→ ok, and `locked` = its `kioskLocked` boolean **if present** (else `None`, and the phone shows only
the last action — white #2 decided, not left open); anything else / non-JSON / non-2xx → `TvError`.

**HTTP client:** one `OnceLock<reqwest::Client>` built without `expect` (build error → `TvError`):
total timeout 5 s, connect 2 s, `redirect::Policy::none()`, **`.no_proxy()`** (purple #5 — an
`HTTP_PROXY` env var must never receive the password), password via `.query(&[…])`.

**Serialisation (red #7):** a process-wide `tokio::sync::Mutex` around every Fully call, so two
parents' phones / rapid taps reach the TV one at a time; a call that cannot take the lock within
the timeout returns `Busy`.

### 2.2 Address allow-list (red #3)

Hand-parsed, never via `url::Url`: optional `http://` prefix stripped; split on the **last** `:`;
host parsed **only** by `std::net::Ipv4Addr::from_str` (rejects octal `010.…`, hex `0x0a…`, decimal
`167772161`, short forms, trailing dots); must be RFC 1918 (10/8, 172.16/12, 192.168/16); port all
ASCII digits (rejects `+80`), 1–65535, default **2323**; no path, query, `@`, `%`, `[`, whitespace.
The stored/used URL is rebuilt as `format!("http://{ip}:{port}")` from the parsed values, never from
the typed string.

### 2.3 Secrets hygiene (red #9–11)

* Password wrapped in a `Secret(String)` newtype whose `Debug`/`Display` print `***`.
* Every `reqwest::Error` goes through `.without_url()` before it is logged or classified;
  `Response::url()` / `error_for_status()` are never used. No `tracing` field holds a URL or body.
* `statustext` / any echoed text clamped with `chars().take(200)` (never byte-sliced).
* Over `:8080` (plain HTTP) the `Secure` cookie never arrives, so the fns fail closed with
  `NotSignedIn`; documented. Nightly backups (`src/server/backup.rs`) contain the password —
  documented in `docs/FIRE_TV.md`.

### 2.4 Phone — `src/client/components/mobile/settings.rs`

A new `#[component] fn KioskSection()` rendered **only** in `SessionState::Parent`, as the section
directly **below "Offline & install"**, heading **"TV kiosk"** (always rendered inside the Parent
branch, so SSR tests need no fetch). Status is fetched with `use_effect` + `spawn` (does not run in
SSR — purple #6). Reuses the class strings already in this file (no Tailwind rebuild expected; if a
new class is needed, `assets/tailwind.css` is rebuilt with v3.4.17 — purple #7).

* **Not configured** — one line: *"Lets this phone unlock and lock the TV. Enter the password you
  use on the TV's remote-admin page (10.0.0.178:2323)."* Fields: **TV address** (prefilled
  `10.0.0.178:2323`) and **TV remote-admin password** (`type=password`, `autocomplete=off`);
  button **Save**. On success the password field is cleared and the buttons appear.
* **Configured** — two big buttons side by side: **Unlock Kiosk** and **Lock Kiosk** (exact
  strings). A state line when known: *"The TV is locked."* / *"The TV is unlocked."* Below, a
  small link **"Change TV address or password"** that reopens the form (both fields, password
  always required).
* **Single tap, no confirm dialog** — the owner asked for a one-tap shortcut and Lock is one tap
  away; the unlock message carries the reminder instead (white #3/#4, recorded as a decision).
* Buttons disabled while a call is in flight.
* Both parents share the one stored credential: the second phone sees the buttons with no setup;
  changing it on either phone changes it for both (white #7; stated in `docs/FIRE_TV.md`).

**Exact sentences** (tested verbatim):

| outcome | sentence |
| --- | --- |
| Done (unlock) | "Kiosk unlocked on the TV. Tap Lock Kiosk when you're finished." |
| Done (lock) | "Kiosk locked on the TV." |
| Done (save) | "Saved. This phone can now unlock and lock the TV." |
| PasswordRejected | "The TV didn't accept that password. Enter it again." (form reopens) |
| TvUnreachable | "Can't reach the TV. Is it on? If it was restarted, its address may have changed." |
| BadAddress | "That doesn't look like a TV address on your home network." |
| TvError | "The TV answered, but not the way we expected. Try again, or use the remote-admin page." |
| Busy | "The TV is still working on the last request. Try again in a moment." |
| NotConfigured | form shown, no sentence |
| NotSignedIn | session set to `SignedOut`; "Your sign-in expired. Enter the PIN again." |

## 3. Tests (acceptance)

`tests/kiosk_tests.rs` starts `#![cfg(feature = "server")]` and an `init_test_env()` `Once` that sets
`DATABASE_URL` / `FAMILY_HUB_DATA_DIR` to a temp dir first (HS9, as `tests/profiles_tests.rs`).
The fake Fully is an axum router on `127.0.0.1:0` counting hits in an `AtomicUsize`.

* **K-A1 allow-list (unit, in `kiosk.rs`):** accepts `10.0.0.178:2323`, `http://10.0.0.178`,
  `192.168.1.5:8080`, `172.16.0.9`; rejects `https://…`, `localhost`, `127.0.0.1`, `8.8.8.8`,
  `172.32.0.1`, `example.com`, `010.0.0.1`, `0x0a.0.0.1`, `167772161`, `10.0.0.178.`,
  `[::ffff:10.0.0.1]`, `10.0.0.178:0`, `10.0.0.178:+80`, `10.0.0.178:70000`, `10.0.0.178/home`,
  `user@10.0.0.178`, `10.0.0.178%25eth0`, ` 10.0.0.178`, empty. Normalised output asserted.
* **K-A2 protocol (fake Fully):** exact `cmd`, `password`, `type=json` received; `OK` → Done;
  *Please login* → PasswordRejected; device-property object → ok with `locked` read from
  `kioskLocked`; non-JSON → TvError; a 302 is **not** followed (target counts zero hits); an
  accept-then-sleep listener → TvUnreachable with elapsed < 6 s.
* **K-A3 save + command through the public fns** (token from `auth::issue_session()`):
  `set_kiosk_admin` with a rejected password stores nothing; with an empty password → rejected and
  the fake gets zero hits; seeded loopback row + `kiosk_command(Unlock/Lock)` → Done; the
  serialised `KioskStatus` JSON does not contain the password; two distinct sessions can both
  command (two-parent case).
* **K-A4 gating:** each fn with `auth = ""` and no request context → `NotSignedIn`, fake gets
  **zero** hits; a cross-site `HeaderMap` (`sec-fetch-site: cross-site`, foreign `Origin`) is
  refused by the gate helper (unit).
* **K-A5 no leaks:** a test `tracing` subscriber captures all output during K-A2's failure cases;
  neither it nor any `KioskOutcome`/`Debug` string contains the password; `Secret`'s `Debug` is
  `***`; a 300-char multibyte `statustext` clamps without panicking.
* **K-A6 SSR (`settings.rs` tests):** `Parent` renders "TV kiosk" **after** "Offline & install";
  `SignedOut` / `FirstRun` render none of "TV kiosk", "Unlock Kiosk", "Lock Kiosk"; the sentence
  table maps every outcome to exactly the string in §2.4 (pure fn, unit-tested).
* **K-A7 on device (Boss + owner, after reinstall):** on a parent phone over HTTPS:8443 →
  Settings → TV kiosk → enter the password → *Saved…*; **Unlock Kiosk** → Fully's kiosk lock is
  off on the TV; **Lock Kiosk** → back on; wrong password → the PasswordRejected sentence; TV
  unplugged → the TvUnreachable sentence; a signed-out phone shows no TV kiosk section. Record the
  three real Fully response bodies (password redacted) in `docs/FIRE_TV.md`.
* **Gate:** `cargo fmt --check`, `cargo clippy --features server -- -D warnings`,
  `cargo test --features server` (whole suite incl. `docs_tests`), WASM client build per
  `docs/DEV_WINDOWS.md`, CI's stale-CSS check.

## 4. Files

`src/server/api/kiosk.rs` (new) · `src/server/api/mod.rs` (`pub mod kiosk;`, `pub use`, table row)
· `src/shared/types.rs` · `src/client/components/mobile/settings.rs` · `tests/kiosk_tests.rs` (new)
· `docs/FIRE_TV.md` (new subsection after Branch A's last step — no renumbering, Branch headings and
required strings kept, only existing paths backticked) · `docs/PWA.md` (one line) ·
`docs/BACKLOG.md` (`## B-6 — … (owner, 2026-10-03)` + DONE block) · `docs/HANDOFF.md` (the two new
settings keys) · `assets/tailwind.css` only if a new class is unavoidable. No `Cargo.toml`, no
migration.

## 5. Execution and the owner's step

Branch `feature/kiosk-remote`; one implementer (Opus) → independent QA (Sonnet) against §3 →
Boss merges to `main`. **Owner step after merge** — the reinstall in `docs/OWNER_CHECKLIST.md`
(the elevated PowerShell block under its re-install heading; the Boss pastes the exact commands in
the hand-off message). Expected result: `http://10.0.0.246:8080/health` → `"db": true`; on a parent
phone, pull-to-refresh the hub app, Settings shows **TV kiosk** under *Offline & install*. Keep the
TV on, then do K-A7.

Out of scope: other Fully commands (screen on/off, reload, load URL — trivially addable later),
auto re-lock timer, any TV/D-pad surface.

## 6. Review fold-in (2026-10-03)

Red: test-hook design (#1), password-replay on address change (#2), IPv4 parse bypasses (#3), CSRF
(#4), `deviceInfo` shape (#5), expired session (#6), TV request serialisation (#7), char-safe clamp
(#8), redacted `Secret` + log capture (#9), `:8080` fail-closed (#10), backups (#11) — all adopted.
Purple: `auth: SessionToken` signatures, shared types placement, pools/helpers, `.no_proxy()`,
SSR-safe fetch, Tailwind, docs_tests constraints, HS9 test env, file list — all adopted.
White: exact strings, visible locked state (best-effort), plain-English setup copy, verbatim error
sentences, two-parent behaviour, on-device acceptance, explicit owner step — adopted; **confirm
dialog declined** (owner asked for one tap; reminder text instead); "hide the address field" partly
adopted (prefilled, editable).
