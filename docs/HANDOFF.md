# HANDOFF — requests from task agents to the Boss

One section per request. The Boss applies these between waves (PLAN v2 §4,
`docs/reviews/PURPLE_TEAM.md` §P4). Agents append; they do not edit files they
do not own.

---

## From T0.4 (Dioxus 0.7.10 migration) → T0.6 (`build_router()` / `main.rs`)

### H-1. `serve_static_assets()` panics when `<exe dir>/public` does not exist

`dioxus_server::server::serve_dir_cached` (`server.rs:408`) does

```rust
let dir = std::fs::read_dir(directory)
    .unwrap_or_else(|e| panic!("Couldn't read public directory at {:?}: {}", &directory, e));
```

so `serve_dioxus_application(...)` — which calls `serve_static_assets()` — is a
hard panic whenever the public directory is missing. `dx build` always creates
it, so the release path is fine, but **a bare `cargo run --features server`
crashes at startup**. `tests/http_tests.rs::init_test_env` works around it by
pointing `DIOXUS_PUBLIC_PATH` at an empty temp directory.

Request: when T0.6 builds `build_router()`, make the public directory
resolution explicit — create the directory if missing, or resolve it from
`FamilyHubConfig` (T0.5) — so the service binary (T3.1) can never die on this.

### H-2. `/m` and `/mobile` both exist as routes

T0.4 added a `MobileShort` route at `/m` (PLAN v2 D3′, and Gate-2 assertion 6
requires `GET /m` → 200) alongside the pre-existing `/mobile`. Both render the
same component. T0.6 owns the routing table for `/tv` and `/m`; decide there
whether `/mobile` becomes a 308 to `/m` or is dropped. Note that
`tests/http_tests.rs::http_mobile_serves_routine_only_view` is a **T0.3
acceptance assertion** and must not be weakened without a Boss commit to
`PLAN.md`.

### H-3. Server-function endpoints are now explicit

Dioxus 0.7 hashes implicitly-named server-function endpoints with
`CARGO_MANIFEST_DIR`, so the wire path would change whenever the repo moves.
T0.4 pinned every `#[server]` fn to an explicit `endpoint = "<fn name>"`, giving
stable paths `/api/today`, `/api/get_daily_routine`,
`/api/toggle_routine_task`, `/api/create_photo_task`, `/api/get_custom_tasks`,
`/api/toggle_custom_task`, `/api/get_today_events`,
`/api/list_screensaver_images`. T2.2's `sw.js` network-first rule can match on
the `/api/` prefix. Any server fn added later should keep the convention.

### H-4. `Dioxus.toml` lost its `[web.resource]` block

`dx` 0.7.10 rejects `[web.resource]` without a `dev` field
(`missing field 'dev'`). The block was empty, so T0.4 deleted it. If styles or
scripts ever need declaring there, the 0.7 schema requires `dev = []` too.

---

## From T0.5 (`FamilyHubConfig`) → Boss (`Cargo.toml` micro-commit)

### H-5. `dioxus-cli-config` is now an unused dependency

T0.5 removed `main.rs`'s only call to
`dioxus_cli_config::fullstack_address_or_localhost()` (replaced by
`FamilyHubConfig::http_addr`, per PLAN v2 T0.5 / PURPLE_TEAM.md finding 10).
Nothing in `src/` references the `dioxus-cli-config` crate any more. It's
harmless to leave (an unused *optional* dependency doesn't trip clippy), but
Boss may want to drop the `dioxus-cli-config = { ... }` line and its
`dep:dioxus-cli-config` feature entry in a between-wave `Cargo.toml`
micro-commit (T0.5 does not own `Cargo.toml` — PURPLE_TEAM.md §P4).

**Applied by Boss (Wave 0-d close):** the `dioxus-cli-config` dependency and its
`dep:dioxus-cli-config` feature entry were removed from `Cargo.toml`; the crate
remains in `Cargo.lock` only as a transitive dependency of `dioxus-server`.

### H-6. `src/server/config.rs` ships a hand-rolled TOML subset, not the `toml` crate

`familyhub.toml` only needed flat `key = "value"` pairs for T0.5's three
settings, and `Cargo.toml` (owned by T0.2/T0.4) was not available to add a
dependency to. `FamilyHubConfig` therefore parses a minimal subset itself
(`TomlValues` in `src/server/config.rs`): comments, blank lines, `[section]`
headers namespaced as `section.key`, and `key = "value"` / `key = value`
lines — no arrays, no multiline strings, no nested inline tables. This is
almost certainly *not* enough for T1.3's `[certs]` block or T1.8's `[acme]`
block if either needs richer structure (e.g. a list of SANs, or nested
provider config). If so, request the `toml` crate (`toml = "0.9"` or similar,
pulling in `toml_edit`/`toml_parser`, both already resolved transitively in
`Cargo.lock` via other dependencies) via a `Cargo.toml` micro-commit, and
swap `TomlValues` for real deserialization at that point — `FamilyHubConfig`'s
public API (`data_dir`, `http_addr`, `tls_addr`, the `*_dir()` helpers) does
not need to change to make that swap.

---

## From T0.6 (`build_router()` / `main.rs`) — resolving H-1 and H-2

### H-1 — resolved

`src/server/router.rs::run` now calls a private `ensure_public_dir_exists()`
before `build_router` (and therefore `ServeConfig::new()`/
`serve_dioxus_application`) ever runs: it resolves the same path
`dioxus_server::server::public_path()` would (`DIOXUS_PUBLIC_PATH` env var,
else `<exe dir>/public`) and `create_dir_all`s it, logging a warning instead
of panicking if that somehow fails. Covered by
`router::tests::ensure_public_dir_exists_creates_the_directory`. A bare
`cargo run --features server` (and, later, T3.1's service binary) can no
longer die on a missing public directory.

### H-2 — resolved (decision: keep `/mobile` as-is, do not redirect or drop)

`build_router` does not add an axum-level route for `/mobile`; it keeps
falling through to Dioxus's `Route::Mobile` exactly as before T0.6, still
answering 200 with the routine-only view. Reasoning: H-2 itself flagged
`tests/http_tests.rs::http_mobile_serves_routine_only_view` as a **protected
T0.3 acceptance assertion** that must not be weakened without a Boss commit
to `PLAN.md`, and nothing in PLAN v2 §3/D3′ actually calls for dropping or
redirecting `/mobile` — only for `/tv` and `/m` to exist, which they now do.
Redirecting or dropping `/mobile` would have broken that protected test on
T0.6's own authority, which the autonomy policy (`PURPLE_TEAM.md` §P5.1.4)
reserves for a Boss decision. If a future task wants `/mobile` gone, it
should go through that channel rather than reopen this here.

---

## Boss — Wave 0-e close (T0.6, T0.7, T0.8 merged)

- **T0.6:** merged as-is. H-1 and H-2 are closed as recorded above; no new
  requests.
- **T0.7 / `Cargo.toml`:** T0.7 does not own `Cargo.toml` (§P4) but its two-line
  change (`[workspace] members = ["xtask"]`, `image` as a dev-dependency for
  the decoder assertions) was inseparable from the task and no other wave 0-e
  task touched the file. Accepted in the T0.7 squash as the serialized Boss
  micro-change. `cargo fmt`/`clippy` now also cover the `xtask` member.
- **T0.7 fix-up:** the squash rendered the maskable icons identical to the
  regular ones and did not assert the ≥ 10 % safe zone the acceptance row
  requires. Boss added the 10 % inset in `xtask` and
  `tests/docs_tests.rs::test_maskable_icons_have_ten_percent_safe_zone_padding`
  (commit `T0.7 (Boss fix-up)`). Regular icons regenerate byte-identically.
- **T0.8 / `assets/tailwind.css`:** `assets/**` is T0.7-owned, so T0.8's
  regenerated CSS was dropped from its squash and rebuilt by the Boss on the
  merged tree instead (`chore(assets): Rebuild tailwind.css …`). Any later
  task whose `src/` change adds a Tailwind token must rebuild the CSS in the
  same branch or the CI fail-on-diff step will go red — treat
  `assets/tailwind.css` as a build artifact that follows `src/`, not as a
  T0.7-owned asset, from here on.
- **T0.8 fix-up:** `tailwindcss.exe --version` exits 9 on the pinned
  standalone binary; the CI install step now probes `--help` for the
  `tailwindcss v3.4.17` banner instead (commit `T0.8 (Boss fix-up)`).
- **H-6 (`toml` crate):** still open — no wave 0-e task needed it. Left for
  T1.3/T1.8 to raise if the hand-rolled subset proves insufficient.

---

## From T1.1 (migrations & storage) → T1.2, T1.5, T1.6, T2.3, T2.4, T1.7

T1.1 owns `migrations/**` and `src/server/db.rs` only. Everything below is a
seam it deliberately left for the tasks that own the calling code — no other
file was edited.

### H-7. The midnight tick must call `db::on_midnight_tick()` (T1.2)

D4 puts `wal_checkpoint(TRUNCATE)` on the midnight tick, but T1.2 owns the tick
(`src/server/api/realtime.rs`). T1.1 exposes the hook instead:

```rust
// once per rollover, after DayRolled is broadcast
if let Err(err) = family_calendar::server::db::on_midnight_tick().await {
    tracing::warn!(%err, "midnight WAL checkpoint failed");
}
```

`on_midnight_tick()` resolves the process pools itself and checkpoints the
write pool. Without this call the `-wal` sidecar grows unbounded on a box that
is never rebooted — the checkpoint is implemented and unit-tested here
(`storage_tests::pragmas_are_wal_normal_and_thirty_second_busy_timeout` asserts
the file is truncated to 0 bytes), it just has nothing calling it yet.

### H-8. The DST-safe scheduling helpers live in `db` (T1.2, T2.4)

`db::resolve_local(&tz, naive)` and `db::next_local_midnight(&tz, &after)` are
the `.earliest()` helpers D5 requires. They are generic over
`chrono::TimeZone`, so they work with `chrono::Local` today and with a
`chrono_tz::Tz` if that crate is ever added. `next_local_midnight` also handles
zones that skip midnight itself (it walks forward to the first wall-clock time
that exists) so the tick cannot stall. They are in `db.rs` because that is the
file T1.1 owns; if T1.2 would rather they lived in a `src/server/time.rs`, move
them there in the T1.2 branch and leave a `pub use` behind.

### H-9. Reads should move to `db::read_pool()` (T1.5, T2.4, T1.7)

There are now two pools. `db::pool()` keeps its old meaning and is the **write**
pool (max 1 connection), so every existing call site still compiles and is
still correct. `db::read_pool()` (max 5) is the one that stops a `SELECT`
queueing behind a write. T1.1 did not rewrite the call sites in `src/server/api.rs`
because it does not own that file. Whoever splits/owns the API modules should
point the read-only server fns (`daily_routine`, `custom_tasks`, calendar
queries, `/health`) at `db::read_pool()`.

### H-10. Whiteboard storage is already written (T2.3, T1.6)

T2.3 owns `src/client/components/whiteboard.rs` but is not listed as an editor
of `db.rs`, so T1.1 landed the server side it needs:
`db::insert_stroke`, `db::board_snapshot`, `db::clear_board`,
`db::undo_last_stroke`, and `db::DEFAULT_BOARD_ID`. Semantics:

- one row per **stroke**, `points` a JSON array of normalised `[x, y]` pairs;
- `seq` is dense and monotonic per board and is what `Snapshot` orders by — it
  is *not* a lifetime-unique id (an undo frees its number, and compaction
  restarts the sequence on an emptied board);
- `clear_board` stamps the `cleared_at` watermark on every live stroke rather
  than deleting: the next `Snapshot` is empty while the rows survive for T1.6's
  compaction pass, which should `DELETE FROM whiteboard_strokes WHERE
  cleared_at IS NOT NULL` and then trim to the newest 2,000 live strokes.

If T2.3 or T1.6 needs another query shape, add it to `db.rs` in your own branch
(you are the later editor per §P4) rather than duplicating SQL elsewhere.

### H-11. Migration numbers after `0003`

`0001_init` and `0002_core` are T1.1's; `0003_profiles` is T1.4's, as assigned.
`0002_core` deliberately does **not** put foreign keys on the `user_id` columns
of `events` — `profiles` does not exist until `0003`, and T1.4 is the task that
drops v1's two `CHECK (user_id BETWEEN 1 AND 4)` constraints in favour of real
FKs. T1.4 should add the `events.user_id` FK in the same migration if it wants
one. Anything past `0003` needs a Boss-assigned number.

### H-12. `tests/fixtures/family_v1.db` is a committed binary fixture

The v1 database that acceptance (b) migrates is committed at
`tests/fixtures/family_v1.db` (394 routine log rows, 4 custom tasks, no
`_sqlx_migrations` table, `delete` journalling — exactly what the pre-migration
build produced). It is regenerated by the `#[ignore]`d
`storage_tests::generate_v1_fixture`, which restates the v1 DDL verbatim:

```
cargo test --features server --test storage_tests -- --ignored generate_v1_fixture
```

Do not "fix" that test to use the current schema — its whole value is that it
does not.

### H-13. New root file: `.gitattributes` (pins the migration SQL to LF)

The repo is developed with `core.autocrlf=true`, and `sqlx::migrate!` checksums
the raw bytes of each migration. Without a rule, a fresh checkout would produce
CRLF migration files whose checksums differ from the LF ones already recorded
in `_sqlx_migrations` on the owner's `family.db`, and every subsequent boot
would fail with a migration version mismatch. T1.1 therefore added a root
`.gitattributes` containing exactly one rule, forcing `eol=lf` on
`migrations/`'s `.sql` files. No task owned that file; flagging it here as the
Boss micro-change it effectively is. Later migrations inherit the rule
automatically.

## T1.2 — Realtime protocol v2 (`docs/PROTOCOL.md`)

### Requests for files T1.2 does not own

- **H-7 · `src/server/router.rs` (T0.6 → T1.3 → T2.5): start the midnight tick
  explicitly.** The DST-safe tick lives in
  `server::api::realtime::spawn_midnight_tick()` and is started, once per
  process, by `ensure_background_tasks()` from the `/ws` handler — so it does
  run, but only from the first WebSocket upgrade. Its proper home is one line
  at the top of `router::run`, next to `calendar::spawn_polling_task()`:

  ```rust
  crate::server::api::realtime::ensure_background_tasks();
  ```

  T1.2 did not add it because `router.rs` is T0.6-owned and T1.3 is editing it
  in the same wave. Boss to apply between waves, or T1.3 to fold in.

- **H-8 · `MaximizedView` needs a `Screensaver` variant for T2.7.** PLAN §3
  T2.7 calls for a scheduled `SetView(Screensaver)`, but `MaximizedView`
  (`shared/types.rs`, aliased as `View` by the protocol) has only
  `None`/`Routine`/`Calendar`/`Whiteboard`, and adding a variant forces a new
  arm in two exhaustive matches in `src/client/components/dashboard.rs`, which
  is T2.1's file. T1.2 left the enum alone rather than edit a file it does not
  own; T2.1 should add the variant when it rebuilds the kiosk views.

### Files outside T1.2's ownership that had to change to keep the tree building

The v1 `WsMessage` enum is gone (§P2c splits it into `ClientMessage` /
`ServerMessage`), so every call site had to move with it. These are mechanical
call-site ports, not feature work, and none is in another wave-1-a task's file
set:

| File | Owner | Change |
| --- | --- | --- |
| `src/server/calendar.rs` | T2.4 | `publish(WsMessage::CalendarUpdated { events })` → `publish(ServerMessage::CalendarUpdated { date })`. v2 does not push the payload; clients refetch. |
| `src/client/components/calendar.rs` | T2.4 | Reads `bus.calendar_version` and refetches through `get_today_events()` instead of rendering a pushed payload. The `Loading`/`Empty`/`Error` state machine (W3) is still T2.4's. |
| `src/client/components/whiteboard.rs` | T2.3 | Sends `ClientMessage::Draw`/`ClearBoard`, uses `StrokeBatcher` for the ≤ 30 msg/s cap, drains `bus.inbound_strokes`, replays `Snapshot`. Persistence, `cleared_at`, undo-own-last and `ResizeObserver` remain T2.3's. |
| `tests/http_tests.rs` | T0.3 | The two `ws_*` tests were **ported, not weakened**, exactly as T0.4 ported them to Dioxus 0.7: the same assertions (a stroke from client A reaches client B; a server `publish` reaches a client) over the v2 envelope, plus a new `origin` check. |

### Deviations from §P2c, and why

1. **`ClientId` is `ClientId(String)`, not `ClientId(Uuid)`.** `uuid` is a
   server-only optional dependency and `shared/types.rs` also compiles to
   wasm. The server still mints a v4 UUID; only the Rust type differs, the
   wire bytes do not. Enabling `uuid` for the web feature means editing
   `Cargo.toml`, which §P4 reserves for a serialized Boss micro-commit — and
   T1.3 is editing it in the same wave.
2. **Dates and timestamps on the wire are `String`, not `NaiveDate` /
   `DateTime<Local>`,** for the same reason (`chrono` is server-only). The
   serialised form is identical (`YYYY-MM-DD`, RFC 3339).
3. **The stroke flush interval is 34 ms, not 33 ms.** §P2c says "33 ms" and
   "hard cap ≤ 30 messages/second" in the same paragraph, but 1000/33 = 30.3/s
   breaks the cap it is meant to enforce. 34 ms gives 29.4/s. Recorded in
   `docs/PROTOCOL.md` §6.
4. **The DST test models `America/New_York` and `Europe/London` with a
   hand-written `chrono::TimeZone`** instead of adding `chrono-tz`
   (`Cargo.toml` again). It derives ambiguity and gaps from the UTC rule, so
   `next_midnight`'s `.earliest()` is exercised against real
   `MappedLocalTime::{None, Ambiguous}` values rather than a lookup table. If
   Boss would rather add `chrono-tz 0.10` as a dev-dependency, the test's
   `dst` module can be deleted and the two constants swapped for `Tz` values.

### Seams left for later tasks

- **T1.4:** `server::api::realtime::session::is_valid` is the *only* place the
  hub validates a parent session — replace that one body with the argon2id /
  30-day session store and `SetView` + `SetActiveProfile` are done. A new
  `src/server/api/profiles.rs` is waiting, with `publish_profiles_updated()`
  already wired to `ServerMessage::ProfilesUpdated`.
- **T1.5:** `src/server/api/routine.rs` is yours. `toggle_custom_task` still
  does **not** publish `TasksUpdated` — G22 is explicitly T1.5's row, and the
  broadcast needs the owning `user_id`, which the current signature does not
  carry. The message and its `{user_id, date}` scoping already exist.
- **T2.3:** `record_stroke` / `clear_board` / `snapshot` in
  `server::api::realtime` are an in-memory store with the exact signatures and
  `seq` contract `docs/PROTOCOL.md` §5 freezes. Swap the bodies for
  `whiteboard_strokes` rows.
- **T1.6 / T2.4:** register work on the midnight tick with
  `realtime::on_day_rolled(hook)` rather than editing the loop.
- **T1.7:** `realtime::connected_clients()` returns the live WebSocket count
  for `/health`; `ServerMessage::Health { stale, last_update }` is the badge
  message and `RealtimeBus::{connected, stale}` the client-side signals.

## From T1.3 (TLS + PKI + dual listener + mDNS + QR) → Boss

### H-7. `CryptoProvider::install_default` is the first line of `run`, not of `main`

PURPLE_TEAM.md §P5.4 (rustls row) and the T1.3 task line both word the
requirement as "call `CryptoProvider::install_default(...)` as the **first
line of `main()`**". §P4's ownership table freezes `src/main.rs` after T0.6
("Later editors: **none** (frozen)"), and §P5.1.4 reserves changing a
protected file to a Boss decision, so T1.3 did not edit it.

The call is instead the first statement of `server::router::run()` — the
only thing the server `main` does after building the tokio runtime, and
therefore the first line that executes before anything can touch rustls. It
is idempotent (`install_default` returns `Err` when a provider is already
installed, which this treats as the no-op it is), so test binaries call it
too via `tls::install_crypto_provider()`.

This matters because `reqwest` also links rustls: with a second provider in
the tree, `CryptoProvider::get_default()` panics rather than choosing. The
current tree resolves to `ring` only (`cargo tree` shows no `aws-lc-rs`), so
the install is belt-and-braces today and load-bearing the moment any
dependency pulls `aws-lc-rs` in.

**If Boss wants the literal wording**, the one-line change is to add
`family_calendar::server::tls::install_crypto_provider();` as the first
statement of the `#[cfg(feature = "server")] fn main()` in `src/main.rs`.
It is safe to have in both places. `main.rs` would grow from 18 to 20 lines,
still inside T0.6's `< 25` assertion.

### H-8. `Cargo.toml`: T1.3's crate additions (serialized Boss micro-change)

§P4 routes later crate additions through a Boss micro-commit between waves,
but T1.3 cannot exist without its crates, and no other wave 1-a task
(T1.1 migrations, T1.2 realtime) needs `Cargo.toml` — the same reasoning
Boss applied to T0.7 at the wave 0-e close. Added, all pinned exactly per
§P5.4:

| Crate | Pin / features | Why |
| --- | --- | --- |
| `rustls` | `=0.23.43`, `default-features = false`, `["ring","tls12","logging","std"]` | §P5.4 verbatim. `ring`, never `aws-lc-rs` (needs CMake + NASM on Windows). |
| `tokio-rustls` | `=0.26.4`, `default-features = false`, `["ring","tls12","logging"]` | **Trap:** its default features are `["logging","tls12","aws_lc_rs"]` — leaving defaults on drags aws-lc-rs in. |
| `hyper-util` | `=0.1.20`, `["server","server-auto","http1","http2","tokio","service"]` | The auto h1/h2 server; `service` gives `TowerToHyperService`, which is the whole axum-to-hyper adapter. Not `axum-server`. |
| `hyper` | `=1.11.1`, `["server","http1","http2"]` | Already in the tree via axum; named explicitly because `tls.rs` uses it directly. |
| `rcgen` | `=0.14.10`, `default-features = false`, `["ring","pem","x509-parser"]` | §P5.4 verbatim. |
| `x509-parser` | `=0.18.1` | Reads `not_before`/`not_after`/SANs back **out of the stored certificate** rather than trusting what the issuer believed it wrote. Same version rcgen resolves, so no duplicate. |
| `time` | `0.3`, `["std"]` | rcgen's public API takes `time::OffsetDateTime`. |
| `mdns-sd` | `=0.21.0` | §P5.4 verbatim. |
| `if-addrs` | `=0.15.0` | Host IPv4 enumeration for the SAN list and the A record. `mdns-sd` already pulls this exact version, so it adds no new subtree. |
| `fast_qr` | `=0.13.1`, `default-features = false`, `["svg"]` | **Not** feature-gated: the QR component compiles for `web` (wasm32) as well as `server`. Its `image` feature would drag `resvg` into the shipped binary, so only `svg` is on. |
| dev: `resvg` 0.45, `rqrr` 0.10 | | Acceptance (g) rasterises the component's own SVG (`resvg`, whose `tiny_skia` re-export is the pixmap) and decodes it with an independent reader, rather than round-tripping the encoder against itself. |

Assertion (f) needed no crate at all: mdns-sd answers a querier whose source
port is not 5353 with an RFC 6762 §6.7 legacy-unicast reply, so a plain
`std::net::UdpSocket` and a hand-built 33-byte DNS query are enough — no
`SO_REUSEADDR` bind on 5353, and therefore no `socket2`.

`cargo tree -d --features server | Select-String '^(axum|tower-http|hyper) v'`
is still empty, so T0.8's CI duplicate check is unaffected.

### H-9. `assets/tailwind.css` rebuilt in this branch

Per the wave 0-e note, `assets/tailwind.css` follows `src/`. The new
`src/client/components/qr.rs` introduces Tailwind tokens, so the CSS was
rebuilt in this branch with the pinned standalone binary
(`tailwindcss.exe -i input.css -o assets/tailwind.css --minify`, v3.4.17)
and the diff is committed. CI's fail-on-diff step is clean.

### H-10. `build_router` is unchanged for `/m`; the 308 lives in `build_http_router`

D3' wants `GET http://…:8080/m` to be a 308 while `GET https://…:8443/m` is
a 200. Both of T0.3's and T0.6's protected assertions
(`http_tests.rs::http_m_serves_routine_only_view`,
`router_tests.rs::m_route_serves_the_phone_routine_view`) call
`build_router(&config)` directly and expect 200 — so making `build_router`
itself redirect would have broken a protected test on T1.3's own authority.

Resolution: `build_router` stays the shared router (200 on `/m`, unchanged
behaviour for every existing test) and a new `build_http_router(&config)`
wraps it in the plain-HTTP origin's single upgrade rule. `run()` serves
`build_http_router` on `:8080` and `build_router` on `:8443`. The upgrade
set is `/m`, `/m/*`, `/mobile`, `/mobile/*`, `/manifest.webmanifest`,
`/sw.js`; `/mobile` is included because it is a phone view and matches D3''s
literal `/m*`, and no existing assertion covers `/mobile` on the HTTP
*origin* (only on `build_router`). Everything the TV needs — `/tv`, `/ws`,
`/assets`, `/uploads`, `/ca.crt`, `/health` — is served on plain HTTP, never
redirected.

### H-11. `certs.mode` is read but has only one legal value until T1.8

`CertSource::from_mode(None)` is what `run()` calls today. The hand-rolled
TOML subset in `src/server/config.rs` (H-6) was **not** replaced:
`[certs] mode = "self_signed"` is a flat string and `TomlValues` already
namespaces it as `certs.mode`, so the `toml` crate is still not needed.
T1.8 should wire `FamilyHubConfig` to surface `certs.mode` and pass it to
`CertSource::from_mode`, which already rejects anything unknown with a clear
startup error — PURPLE §P3 T1.8 assertion (d).

### H-12. Private-key ACLs use `icacls.exe`, not the `windows` crate

`<data>\pki\ca.key` and `leaf.key` get `/inheritance:r` plus grants to
SYSTEM, Administrators and the running user, applied only when a key is
newly written. `icacls.exe` is a Windows built-in — the OS's own ACL API
surfaced as a command, not a new project dependency, so `docs/NON_RUST.md`
is unchanged. The alternative was the `windows` crate's
`SetNamedSecurityInfoW` for one call. Failure is logged and never fatal: a
hub that cannot tighten an ACL must still boot. T3.1's elevated `install`
subcommand is where the data directory's inherited permissions get set
properly, and it will shell to `netsh advfirewall` on the same reasoning
(PLAN v2 T3.1's own acceptance test asserts against `netsh` output).

**Boss decision requested:** if you count an OS built-in invoked at runtime
as a declared exception, `docs/NON_RUST.md` needs a row for `icacls.exe`
(and, at T3.1, `netsh.exe` / `sc.exe`). T1.3 did not add one because
`docs/NON_RUST.md` is T0.1-owned and this is an OS API call rather than a
component of the stack — but it is a one-line call to make either way, and
the alternative (dropping the ACL narrowing) would leave §P5.5 default 7's
"CA key ACL-restricted" unimplemented until T3.1.

### H-13. For T1.6 — what must stay out of backups

`<data>\pki\` contains `ca.crt`, `ca.key`, `leaf.crt`, `leaf.key`. T1.6's
assertion (f) is "`backups/` contains no `.key` file": excluding the whole
`pki/` directory satisfies it and is the right rule — the CA key is the one
secret whose loss would let someone mint certificates for the hub, and it is
regenerable (re-install the new `/ca.crt` on the phones) where the database
is not.

### H-14. For T1.7 — `/health` cert fields

`pki::parse_certificate_validity(&pem)` returns
`(not_before, not_after, SAN set)` read out of the certificate itself, and
`IssuedLeaf::days_remaining()` is the same number the renewal predicate
uses. `router::pki_for(&config.pki_dir())` hands back the *same*
`Arc<SelfSignedCa>` the HTTPS listener and `/ca.crt` are using, so
`/health`'s `days_to_expiry` cannot drift from the leaf actually being
served (T1.7's assertion "expiry matches leaf"). `/health` itself is still
T0.6's stub — T1.7 owns `src/server/health.rs` and replaces
`router::health_stub` when it lands.

---

## Boss — wave 1-a close (T1.1, T1.2, T1.3 squash-merged to `main`)

All three tasks passed review (no acceptance test weakened, no undeclared
non-Rust component, no secrets committed) and each merge ran the full
baseline green. Decisions on the requests above:

- **T1.1 H-7 / T1.2 (applied):** `spawn_midnight_tick` in
  `src/server/api/realtime.rs` now calls `db::on_midnight_tick()` after
  `run_day_rolled`, so the D4 `wal_checkpoint(TRUNCATE)` runs once a day.
- **T1.2 H-7 (applied):** `router::run` calls
  `realtime::ensure_background_tasks()` right after `spawn_polling_task()`;
  the tick no longer waits for the first WebSocket upgrade.
- **T1.3 H-7 (applied):** `family_calendar::server::tls::install_crypto_provider()`
  is now literally the first statement of the server `main()` (`src/main.rs`
  is 20 lines; Boss-only edit to the frozen file). The call in `router::run`
  stays — it is idempotent and keeps the tests' direct `run` path safe.
- **T1.3 H-8 (ratified):** the `Cargo.toml` crate additions are accepted as
  the serialized wave 1-a micro-change; all pins are §P5.4-exact.
- **T1.3 H-12 (decided):** an OS built-in invoked at runtime counts as a
  declared exception. `docs/NON_RUST.md` gains an `icacls.exe` row; T3.1 adds
  `netsh.exe` / `sc.exe` rows on the same basis.
- **T1.1 H-13 (ratified):** `.gitattributes` pinning `migrations/*.sql` to LF.
- **T1.1 H-11:** migration numbers past `0003` are assigned by Boss on
  request; none assigned yet.
- **T1.2 H-8 (deferred to T2.1):** `View::Screensaver` is added by T2.1 when
  it rebuilds the kiosk views (two exhaustive matches in `dashboard.rs`).
- **T1.2 optional `chrono-tz` dev-dep:** not taken; the hand-written zone
  model in `tests/realtime_tests.rs` passes and adds no dependency.
- **T1.1 H-9 (left to T1.5/T2.4/T1.7):** read-only server fns move to
  `db::read_pool()` in their owners' waves.
- **`assets/tailwind.css`** rebuilt once more on the merged tree (T1.2's and
  T1.3's tokens together), per the wave 0-e rule.

Wave 1-b (T1.4, T1.5, T1.6, T1.7) may start from this `main`.

---

## From T1.4 (profiles + settings + parent PIN)

### H-15. `Cargo.toml`: added `argon2 = { version = "=0.6.0", optional = true }` + `dep:argon2` under the `server` feature

Exactly the PURPLE §P5.4 pin, default features only (`alloc`, `getrandom`,
`password-hash` — confirmed via `cargo add argon2@=0.6.0 --dry-run`), which
is everything T1.4 needs: `PasswordHasher`/`PasswordVerifier` plus a
re-exported `rand_core::OsRng` for salt generation and the setup-code random
digits, no extra crate required. Same situation T1.3 H-8 already
ratified for its own wave-1-a additions: T1.4 needed a crate `Cargo.toml`
did not have and there is no separate Boss pass to apply a HANDOFF request
before this task's own acceptance tests must compile and pass, so the
addition was made directly, is scoped to one new optional line + one feature
entry, and is recorded here for the same after-the-fact ratification T1.3's
was given.

### H-16. `src/server/api/realtime.rs`'s `session` module — replaced per its own doc comment

That module's doc comment reserved exactly this: "T1.4 owns the real
implementation ... T1.4 replaces one function body and nothing else." In
practice five bodies changed (`issue`, `insert`, `revoke`, `revoke_all`,
`is_valid`) because the backing store itself moved (a plain `HashSet` with no
expiry → `src/server/auth.rs`'s real 30-day session store), which needed all
five to change together to stay consistent — the three now-dead private
helpers (`SESSIONS`, `sessions()`, `with()`) were removed with them to avoid
`dead_code` under `-D warnings`. Every public signature is unchanged, so
`tests/realtime_tests.rs`'s existing `session::issue()` /
`session::revoke_all()` / `session::is_valid()` calls needed no update.

### H-17. `migrations/0003_profiles.sql` landed — `tests/storage_tests.rs`'s hardcoded migration-version constants bumped 2 → 3

Three assertions in T1.1's `tests/storage_tests.rs`
(`fresh_database_runs_every_embedded_migration`,
`v1_database_is_baselined_and_every_log_row_survives`,
`vacuum_into_backup_restores_to_identical_row_counts`) hardcoded
`db::migration_version(...) == Some(2)` and `vec![1, 2]` for the applied
migration list — correct before `0003_profiles.sql` existed, and an
unavoidable, purely mechanical consequence of adding the next migration
PURPLE_TEAM.md §P4 already named ("`0003_profiles` (T1.4)"). Updated to
`Some(3)` / `vec![1, 2, 3]`; nothing about what those assertions actually
verify (correct baselining, zero data loss, restore round-trips cleanly)
changed. `tests/db_tests.rs:148-157`'s CHECK-constraint test was replaced
with an FK-violation test exactly as the task description instructs (W5);
`daily_routine_logs`/`custom_tasks` both got a matching new test.

### H-17b. `tests/http_tests.rs::http_toggle_routine_task_error_is_structured_not_a_panic` (T0.3/T0.4) — same mechanical fix

`user_id = 99` triggers this test's constraint violation (it exists to prove
the error travels as structured JSON, not a panic — the specific constraint
is incidental to what it verifies). The message text changed from `"CHECK
constraint failed"` to `"FOREIGN KEY constraint failed"` for the same reason
as H-17; the assertion was widened from `.contains("CHECK")` to
`.contains("CONSTRAINT")`, which matches either wording and still requires a
real constraint-violation message, not merely any error.

### H-18. First-run setup code: plain Rust fn, not a `#[server]` endpoint

`auth::ensure_setup_code`/`auth::read_setup_code` are ordinary `pub async
fn`s in `src/server/auth.rs`, reachable from Rust (this crate's own future TV
component, and this task's own tests) but **not** wired to any HTTP route or
`#[server]` fn — exposing the code itself over the network would defeat the
point of gating first-run PIN setup on physical access to the server's
log/file/TV. `api::profiles::parent_setup_status()` is the only network-
reachable endpoint touching this, and it returns only `{ pin_set: bool }`,
never the code. **Request for whichever task wires up the TV's first-run
screen (T2.1) or a phone setup flow (T2.2):** call
`auth::ensure_setup_code`/`auth::read_setup_code` directly from server-
rendered code, or add a dedicated `#[server]` fn if a client-side fetch turns
out to be needed — either way, T1.4 deliberately left that call site to
whichever task actually builds the UI that needs it, since neither
`src/client/components/tv/**` nor `src/client/components/mobile/**` is a
T1.4-owned file.

### H-19. Session tokens are bearer values, not cookies, for this wave

> **CLOSED** by `phase-qa2/T2.2` (QA round 2, Q2-02) — see "T2.2 — QA
> round 2" at the end of this file.

PLAN v2 §2 D3′/PURPLE §P5.5 default 31 describe the parent session as an
`HttpOnly`/`Secure`/`SameSite=Lax` cookie. `src/server/router.rs` (where
`serve_dioxus_application` mounts every `#[server]` fn) is T0.6→T1.3→T2.5
territory, not T1.4's, so this task could not wire response-header/cookie
delivery without editing a file it does not own. What landed instead: real
argon2id PIN hashing, a real 30-day-expiry session store
(`src/server/auth.rs`), and every privileged fn (`api::profiles::*`)
verifying that token server-side before doing anything — the token itself is
returned as the `#[server]` fn's `Ok` value (a plain `SessionToken` /
`String`), the same way the existing WS protocol already carries `auth:
Option<SessionToken>` as an explicit value on `SetView`/`SetActiveProfile`
rather than a cookie. **Request for whoever builds the phone login flow
(T2.2) or touches `router.rs` next:** if the cookie attributes matter for
that surface (e.g. survivability across a PWA reload without replumbing
client-side storage), add a small login HTTP route in `router.rs` that calls
`auth::verify_pin`/`auth::set_initial_pin` and sets the `Set-Cookie` header
itself, or confirm client-side storage of the bearer token is an accepted
substitute — either is a small, contained change once `router.rs` is back in
that task's own wave.

## From T1.5 (date correctness + authz + missing broadcasts) → T2.5

`toggle_routine_task` and `toggle_custom_task` (`src/server/api/routine.rs`)
now take two new **required** parameters, `date: String` (`YYYY-MM-DD`,
validated within ±1 day of the server's clock — `db::date_within_window`)
and `idempotency_key: String` (claimed via `db::claim_mutation`, so a
replayed call is a no-op the second time). `toggle_custom_task` also gained
`user_id: u32` up front — it must equal the task's owner
(`db::custom_task_owner`) or the call errors and writes nothing.

New signatures:

```rust
toggle_routine_task(user_id: u32, template_id: u32, completed: bool, date: String, idempotency_key: String)
toggle_custom_task(user_id: u32, task_id: u32, completed: bool, date: String, idempotency_key: String)
```

`toggle_custom_task` now publishes `ServerMessage::TasksUpdated { user_id,
date }` on a real change (G22/W1 — the v1 endpoint never did). `db.rs` grew
`date_within_window`, `claim_mutation` and `custom_task_owner`; `get_daily_routine`
and `get_custom_tasks` now read through `db::read_pool()` per H-9.

`src/client/components/routine.rs`'s `Routine` component (T1.5 landed the
date/authz call-site changes here first, per §P4) now resolves "today" via a
`RoutineDateState` state machine (`Loading`/`Ready(date)`/`Error`) instead of
`today().await.unwrap_or_default()` — see `RoutineDateState::resolve` and
`new_idempotency_key()`, both `pub`. When `RoutineDateState::Error`, `Routine`
renders an explicit "can't reach the hub" panel instead of the routine list —
**T2.5, when you touch this file's UI, please keep the `Error` branch and the
`mutation_date` plumbing rather than reverting to a bare fetch-and-unwrap.**
`CustomTaskRow`'s `on_toggle` now passes `task.user_id` as the owning
`user_id` T1.5's ownership check requires — the client already has it from
`get_custom_tasks`'s `CustomTaskView`.

`tests/http_tests.rs`'s two `toggle_routine_task` wire tests
(`http_toggle_routine_task_round_trip_mutates_db`,
`http_toggle_routine_task_error_is_structured_not_a_panic`) were updated in
this branch to include `date`/`idempotency_key` in their JSON bodies — they
predate T1.5 and would otherwise fail to deserialize. T1.5's own acceptance
suite is `tests/routine_tests.rs`.

## T1.7 — `/health` JSON + the TV staleness badge state machine

`src/server/health.rs` is T1.7's sole file (`docs/reviews/PURPLE_TEAM.md`
§P4). Resolving H-14 above: `router::health_stub` is gone, `/health` now
calls `health::health_handler(config)`, and `router::pki_for` is `pub(crate)`
so the handler reads the exact `Arc<SelfSignedCa>` the HTTPS listener is
serving (one line of `router.rs`, plus one call to
`health::mark_started()` at the top of `router::run` for an accurate
`uptime_seconds` — both inside the edit `docs/HANDOFF.md`'s own T1.3 H-14
note already told this task to expect).

### Requests for files T1.7 does not own

- **For T2.4 (`src/server/calendar.rs`): call `health::record_google_poll_success`.**
  `/health`'s `last_google_poll` is a process-wide `Option<String>` T1.7
  exposes a setter for (`crate::server::health::record_google_poll_success(chrono::Local::now())`)
  but cannot call itself — `calendar.rs` is T2.4's file. Call it once per
  successful poll, inside `store_events` (or right after `fetch_today` returns
  `Ok`) in `spawn_polling_task`'s loop. Until T2.4 lands this, `/health`
  honestly reports `last_google_poll: null` — which is also *correct* today,
  since PURPLE §P5.5 default 24 ("no Google service account assumed") means
  `spawn_polling_task` returns immediately without ever polling in this run
  (A4: no credentials exist).
- **For T2.1 (`src/client/components/tv/**`): consume `health::StalenessTracker`
  for the disconnected badge.** D8's "permanent 'updated HH:MM' + red
  disconnected badge after 90 s of silence" is two conditions ORed together —
  "the socket is down" (already `RealtimeBus::connected`, T1.2) and "data is
  older than 90 s" (new: `health::StalenessTracker`, pure and unit-tested in
  `src/server/health.rs`, no socket/clock/Dioxus dependency). `health.rs` is
  `#[cfg(feature = "server")]`-gated (`server/mod.rs`), so it does not compile
  for the `web`/wasm32 target the TV kiosk view actually runs on — T2.1 should
  either port `StalenessTracker` verbatim into its own client-side module (it
  is ~15 lines, two methods, no dependencies beyond `std::time::Instant`) or,
  if Boss would rather there be exactly one copy, move it into
  `src/shared/types.rs` (T1.2's file, later edited by T1.4 — a third owner
  would need a Boss decision) using `web_time::Instant` in place of
  `std::time::Instant` for wasm portability (`web_time` is not yet in
  `Cargo.toml`). T1.7 did not make that call unprompted since it touches a
  file it does not own either way.

### Deviations / notes

- **Disk free via a direct `GetDiskFreeSpaceExW` FFI call, not a new crate or
  a shelled-out command.** Same reasoning Boss ratified for T1.3's
  `icacls.exe` (H-12 above: an OS built-in invoked at runtime is a declared
  exception, not an undeclared non-Rust component) — except this is a raw
  `extern "system"` call into `kernel32.dll`, not even a spawned process, so
  it is arguably *more* clearly "the OS's own API" than icacls was. No
  `docs/NON_RUST.md` row requested for the same reason T1.3's wasn't: it's an
  OS API call, not a component of the stack. Boss may still want a row for
  symmetry with the icacls/netsh precedent — flagging it here rather than
  deciding it, since `docs/NON_RUST.md` is T0.1-owned.
- **`/health`'s HTTP status doubles as the `db` signal**: 200 when the
  database answered a real `SELECT 1` against `db::read_pool()`, 503
  otherwise, so a monitor that only reads the status line still sees the hub
  is unwell. Every other key stays populated and typed even when `db` is
  false — a dead database does not blank the rest of the report.
- **`tests/health_pool_closed_tests.rs` is a separate test binary** from
  `tests/health_tests.rs`, solely to close the process-wide `db::pools()`
  `OnceCell` (H-9) without taking down any other test that shares a binary —
  see that file's own doc comment.

---

## Boss decisions at the wave 1-b close (T1.4, T1.5, T1.7 merged)

Merged in order T1.4 → T1.5 → T1.7 (squash), baseline green after each.
T1.6 was not part of this closeout and its branch/worktree are untouched.

- **H-15 (`argon2 = "=0.6.0"` in `Cargo.toml`): ratified** as this wave's
  serialized Cargo.toml micro-commit — it is exactly the §P5.4 pin and no
  other wave 1-b task touched `Cargo.toml`.
- **H-16 (`api/realtime.rs::session` delegating to `auth.rs`): ratified** —
  the seam was reserved for T1.4 by T1.2's own doc comment; public
  signatures unchanged, `tests/realtime_tests.rs` unmodified and green.
- **H-17 / H-17b (test constant bumps, `CHECK` → `CONSTRAINT`): ratified** as
  mechanical and non-weakening. Boss applied the same bump to
  `tests/health_tests.rs` (`migration_version` 2 → 3), which T1.7 wrote
  against the pre-0003 tree in the same wave.
- **T1.7 → T2.4 (`record_google_poll_success`): applied by Boss** — one call
  in `src/server/calendar.rs`'s poll loop after `store_events`. T2.4 keeps
  it when it rewrites that loop.
- **T1.7 `GetDiskFreeSpaceExW` FFI: `docs/NON_RUST.md` row added** for
  symmetry with the `icacls.exe` precedent (OS built-in at runtime).
- **T1.7 → T2.1 (`StalenessTracker` on the TV): decided — T2.1 ports it
  client-side** into `src/client/components/tv/**` (its own files). No third
  owner on `src/shared/types.rs`, no `web_time` crate. `std::time::Instant`
  is unavailable on wasm32 anyway, so the port takes its injected "now" as
  milliseconds from the same performance-clock source
  `src/client/components/routine.rs::new_idempotency_key` already uses;
  `src/server/health.rs`'s struct + 4 unit tests remain the reference
  semantics (on past 90 s, off within 2 s). The badge is
  `!RealtimeBus::connected || tracker.is_stale(now)`.
- **H-18 (first-run setup code display): left for T2.1** (TV first-run
  screen) — call `auth::ensure_setup_code`/`read_setup_code` from
  server-rendered code, per T1.4's note. Not network-exposed; that stays.
- **H-19 (session as bearer value, not cookie): open, scheduled for the
  2-a boundary.** PLAN D3′ / §P5.5 default 31 (HttpOnly/Secure/Lax cookie,
  HTTPS origin only) remains the target. `router.rs` is T2.5's in wave 2-a
  and T2.2 (phone login flow) runs in the same wave, so neither can add the
  login route without a conflict: Boss will apply the `Set-Cookie` login
  route in `router.rs` as a micro-commit at the 2-a/2-b boundary (or fold it
  into T2.5's router edit if T2.5's brief is amended). Until then T2.2 holds
  the bearer token client-side and passes it as `auth: Option<SessionToken>`
  exactly as the WS protocol already does.

Wave 2-a (T2.1–T2.5) may start from this `main` once T1.6 is also closed.

---

## From T2.1 (Fire TV kiosk / 10-foot UI) → Boss, T1.2, T1.4, T2.2, T2.3, T3.4

T2.1 owns `src/client/components/tv/**` (PURPLE §P4) and stayed inside it
except where noted below. Golden files live in `tests/golden/`; the acceptance
suite is `tests/tv_tests.rs`.

### H-20. `src/client/app.rs` — one line changed, and it has no listed owner

§P4's ownership table has no row for `src/client/app.rs`, and `/tv` is useless
if nothing routes to the new surface, so `KioskDashboard` now renders
`TvShell {}` instead of `Dashboard {}` (plus the matching `use`). That is the
whole diff in that file.

**T2.2 will need the same file** for `/m`, and it edits a different component
(`Mobile`), so the two should merge cleanly — but Boss should confirm at the
2-a close and give the file an owner for wave 3. `components::dashboard` is
now unreferenced by `/tv`; it is left in the tree because it is the obvious
starting point for T2.2's phone layout. If T2.2 does not want it, Boss can
delete it at the 2-a close.

### H-21. `assets/tailwind.css` was rebuilt — and must be rebuilt again at the 2-a close

`assets/**` belongs to T0.7, but Tailwind's output is *generated from `src/`*,
and the kiosk's new utilities (`p-[5%]`, `ring-8`, `ring-offset-4`,
`ring-transparent`, `text-4xl/5xl/6xl`, `bg-red-600`, `w-[26rem]`, …) are not
in the committed CSS, so `/tv` would render unstyled without it. Rebuilt with
the pinned binary, exactly the command T0.8's CI step runs:

```powershell
& "$env:USERPROFILE\.cargo\bin\tailwindcss.exe" -i input.css -o assets/tailwind.css --minify
```

Every other wave 2-a task adds classes too, so this file **will** conflict.
Precedent is the wave 1-a close (`1bc45d9`): Boss rebuilds it once on the
merged tree. Resolve any conflict by taking either side and re-running the
command above — it is a build artefact, never hand-edited.

### H-22. `tv_clock()` is a `#[server]` fn living outside `src/server/api/`

`src/client/components/tv/clock.rs` declares one server function,
`tv_clock() -> TvClock { hhmm, date }`, returning the **hub's** local time
(§P5.5 default 14: server-local, never device-local). It is declared there
because every module in `src/server/api/` is owned by another task
(T1.4/T1.5/T2.4/T2.7) and a wave-2 task may not edit them.

Request: fold it into `src/server/api/` (a new `tv.rs`, or `routine.rs`
alongside `today()`) at a wave boundary and re-export it from `api/mod.rs`
like every other server fn. The move is mechanical — the only callers are
`tv::shell` and its own unit test.

### H-23. → T1.2: the server never sends `ServerMessage::Health`

`docs/PROTOCOL.md` and §P2c list `Health { stale, last_update }`, but nothing
in `src/server/api/realtime.rs` ever emits it, so **the only server→client
traffic on an idle socket is `Pong`** — and `RealtimeBus::apply` matches
`Pong` to `{}`. The consequence for D8: a client cannot observe "the hub is
alive" over the WebSocket at all between user actions.

T2.1 works around this by polling `tv_clock()` every 20 s, which is a real
round trip to the hub and therefore a real liveness probe; that poll is what
feeds `TvStaleness::record_message`, and the badge is
`!connected || tracker.is_stale(now)` exactly as the wave 1-b Boss note
specified. It works, but it is an HTTP poll standing in for a protocol
message that is already designed.

Request: when `src/server/api/realtime.rs` is next open, broadcast
`ServerMessage::Health { stale: false, last_update }` on the existing
heartbeat/tick path (every 20–30 s is plenty). `RealtimeBus.stale` already
consumes it and the kiosk already renders it; the clock poll can then drop to
a much lower frequency or disappear.

### H-24. → T1.4 / Boss: H-18 (the first-run setup code on the TV) is **not** done

The wave 1-b close left "display the first-run setup code on the TV" to T2.1,
suggesting `auth::ensure_setup_code` / `read_setup_code` be called "from
server-rendered code". T2.1 did **not** implement it, deliberately:

* `/tv` is SSR'd and then **hydrated**. A `#[cfg(feature = "server")]` block
  inside a component renders on the server and renders nothing in the wasm
  build — a hydration mismatch on the one surface that must never flicker or
  panic. Nor does `use_server_future` keep the code off the wire: the value
  would be serialised into the hydration payload, which is the same exposure
  as an endpoint.
* The only clean shapes are (a) a `#[server]` fn returning the code, which is
  network-exposed and contradicts T1.4's "not network-exposed; that stays", or
  (b) the same fn restricted to the HTTP TV origin — a real decision about the
  auth surface, which belongs to T1.4's owner, not to a UI task.

Recommendation: (b), as `api/profiles.rs::parent_setup_code()`, gated on the
request arriving on the HTTP kiosk listener and returning `None` on the HTTPS
origin. The kiosk can then show it in the join-QR overlay beside the URL,
which is exactly where a parent standing at the television is looking.
Nothing is blocked meanwhile: the setup code is still written to the log and
to `<data>\setup-code.txt` (T1.4).

### H-25. → T3.4 (styling only) and T2.3: what the kiosk's contract fixes

T3.4 may restyle `src/client/components/tv/**`, but five things are asserted
by `tests/tv_tests.rs` and must survive:

1. every focusable element carries every class in
   `tv::style::TV_FOCUSABLE_CLASS`, and exactly one carries the bare
   `ring-sheffield-sun`;
2. every rendered font size is on `tests/golden/tv_type_scale.txt` (four
   sizes, all ≥ 28 px; `<h1>/<h2>/<h3>` ≥ 44 px). Adding a size means adding a
   row there **and** to `tv::style::TV_TYPE_SCALE`, which the suite compares
   against each other;
3. the surface root carries `tv::style::TV_OVERSCAN_CLASS` (`p-[5%]`);
4. no Tailwind hover variant appears in the directory or in the rendered
   markup, and no pointer handler is wired up on the television;
5. the rendered `data-tv-focus` ids, in document order, equal
   `tests/golden/tv_focus_order.txt` **and** `tv::model::focus_order()`.

That golden file is written against `tv::fixture::canonical_model()`, and the
suite asserts that fixture's routine length still equals
`db::SHEFFIELD_MORNING_ROUTINE.len()` — so adding a ninth morning-routine item
is a deliberate golden-file regeneration, not a silent drift.

**T2.3**: the kiosk renders `Whiteboard {}` inside a frame on its third panel
and gives it **no focusable children** — drawing is phone-only (§P5.5 default
35). A keyboard-reachable control added to that component would not be
reachable from the remote, and the typography allowlist does not cover
`whiteboard.rs` (the test renders the placeholder, not the live component).
Keep the board read-only on the television.

## From T2.2 (phone PWA) → Boss, T2.5, T3.4

### H-20. Two `src/server/router.rs` handler bodies and one new route — the seam T0.6 reserved

`src/server/router.rs` is T0.6's file, later edited by T1.3 and T2.5 (§P4).
T2.2 edited it anyway, minimally, on the same basis T1.7 edited it for H-14:
**T0.6's own doc comments reserved these bodies for this task** — verbatim,
"T0.6 stub. T2.2 replaces the body with the real manifest (icons from T0.7,
`scope: "/"`, `start_url: "/m"`) but keeps this route and its
`application/manifest+json` content type", and the same for `/sw.js`. There is
no other way to satisfy T2.2 acceptance (a) and (b), which are HTTP assertions
against those two routes.

What changed, in full:

* `manifest_stub` and `service_worker_stub` **deleted**; the two `.route(...)`
  lines now point at `client::components::mobile::pwa::handlers::{manifest,
  service_worker}`. Paths and content types are unchanged, so
  `tests/router_tests.rs` and `tests/tls_tests.rs` still pass untouched.
* **One new route**, `.route("/icons/{file}", get(pwa::handlers::icon))`,
  serving T0.7's four PNGs from `include_bytes!` at hash-free URLs. The
  manifest has to reference icons at a stable path, and `asset!()` output is
  hashed (which is G6 all over again); a fixed-table lookup also means the
  route cannot be path-traversed.
* One `use crate::client::components::mobile::pwa;`.

Nothing else in the file was touched. **For T2.5:** the conflict surface is
three adjacent lines in `build_router`'s route chain and one `use` — trivial
to merge either way round.

### H-21. `assets/tailwind.css` is stale — one rebuild at the 2-a merge, please

`assets/**` is T0.7's (§P4) and `assets/tailwind.css` is a build output, so
T2.2 deliberately did **not** regenerate it: every task in wave 2-a adds
utility classes, and five branches each committing an independently minified
CSS file is five guaranteed merge conflicts in a single-line file. CI's
"Tailwind rebuild (fail on diff)" step (T0.8) will therefore be red until
someone rebuilds once.

Request: after merging wave 2-a, run the one command CI runs —

```powershell
& "$env:USERPROFILE\.cargo\bin\tailwindcss.exe" -i input.css -o assets/tailwind.css --minify
```

— and commit the result. (The binary is already installed on this box, v3.4.17
as pinned.) Until then the phone's new classes render unstyled; nothing else
breaks.

### H-22. `assets/manifest.json` is now dead — delete it with the same touch

The v1 hashed manifest (`assets/manifest.json`, `icons: []`) is no longer
referenced by anything: `client/app.rs` now links `/manifest.webmanifest` at
its root URL, and `tests/pwa_tests.rs` asserts the string `manifest.json` does
not appear in `/m`'s HTML. The file is in `assets/**` (T0.7's), so T2.2 left
it in place rather than delete a file it does not own. It should go.

### H-23. For T2.5 — enqueue a failed toggle into the offline queue (two call sites)

The offline queue (`client/components/mobile/queue.rs`) is complete and
tested: it stamps the intended date and a never-regenerated idempotency key,
expires at 48 h with a toast, replays on reconnect (the phone shell's effect)
and on demand (Settings → "Try sending now"). What it does not yet have in
production is a *producer*, because the only two queueable mutations are
issued from `src/client/components/routine.rs`, which is T2.5's file in this
wave.

One line each, at the two existing `let _ = toggle_*(...).await;` call sites:

```rust
use crate::client::components::mobile::queue::{self, QueuedMutation};

if toggle_routine_task(user_id, template_id, completed, date.clone(), key)
    .await
    .is_err()
{
    queue::record_offline_failure(
        QueuedMutation::ToggleRoutineTask { user_id, template_id, completed },
        date,
    );
}
```

and the `ToggleCustomTask { user_id, task_id, completed }` equivalent for
`toggle_custom_task` (`user_id` there is the task's *owner*, which the call
site already has). `record_offline_failure` loads, enqueues and saves; it
needs nothing else. Note the two calls currently discard their `Result` with
`let _ =`, so a failed tick is silently lost today — this closes that as well
as wiring the queue.

If T2.5's brief will not stretch to it, Boss can apply the two lines as a
micro-commit at the 2-a boundary; it touches no file T2.2 owns either way.

### H-24. `web-sys`'s `Storage` feature (optional, `Cargo.toml`)

`client/components/mobile/storage.rs` reaches `localStorage` through four
`#[wasm_bindgen] extern "C"` declarations rather than `web_sys::Storage`,
because `Storage` is not in `Cargo.toml`'s `web-sys` feature list and
`Cargo.toml` is a Boss micro-commit (§P4). The extern block is ordinary Rust
under the already-declared `wasm-bindgen` glue exception
(`docs/NON_RUST.md`), catches the exception Safari private mode throws on
`window.localStorage` access, and needs no new dependency — so this is a
tidiness request, not a blocker. If Boss would rather there be one mechanism,
add `"Storage"` to the `web-sys` features and the module's `imp` can collapse
to `window.local_storage().ok().flatten()`.

### Notes on decisions T2.2 made inside its own files

* **No Background Sync in `sw.js`.** PURPLE §P3 T2.2(e) phrases the Android
  promise as "Background Sync". A service worker cannot read `localStorage`,
  so honouring that literally would mean a second copy of the queue in
  IndexedDB plus a JavaScript reimplementation of the replay and idempotency
  rules — two implementations to keep in step, inside a 6 KB budget, for a
  guarantee iOS cannot offer at all (RR-6). `docs/PWA.md` therefore states the
  promise PLAN v2 D6 itself states — "Android replays on reconnect; iOS
  replays on next app open" — names Background Sync explicitly, and explains
  the decision. The doc test asserts both platform sections and both promises.
* **`/m` is no longer routine-only.** `client/app.rs::Mobile` renders the
  five-tab `MobileShell` (G9). The protected T0.3 assertions
  (`http_mobile_serves_routine_only_view`, `http_m_serves_routine_only_view`)
  are untouched and still pass: Routine is the default tab, so "Add photo
  task" is present and the string "Whiteboard" is not — the board tab is
  labelled "Board" and its content is not rendered until selected.
* **`client/app.rs`** was edited (manifest link moved off `asset!()`, iOS
  meta tags, `Mobile` renders `MobileShell`). It has no listed owner in §P4;
  T2.1 will also need it for the TV routes. The changes are confined to the
  `App` head block and the `Mobile` component, so they do not overlap
  `Tv`/`KioskDashboard`.

## From T2.3 (whiteboard v2)

T2.3's assigned file is `src/client/components/whiteboard.rs` only (§P4), but
persistence cannot exist without also touching the seams T1.1/T1.2 explicitly
reserved for it — both in their own doc comments (`record_stroke`/
`clear_board`/`snapshot` in `server::api::realtime`, `docs/HANDOFF.md` H-10's
"add another query shape to `db.rs` in your own branch") and, in one case, a
seam `tests/realtime_tests.rs` did not yet know it needed. Every edit below
is either (a) a query shape added to `db.rs`, explicitly pre-authorised by
H-10, (b) the exact body-swap T1.2's own doc comment on `record_stroke`/
`clear_board`/`snapshot` asked for, or (c) a small new file/module (T2.3
owns anything it creates that didn't already have an owner). Nothing here
touches `src/client/realtime.rs` (T1.2, no later editor listed) — the queued,
drained `inbound_strokes` signal R-22a needs was already there.

### H-20. `src/server/db.rs` — new query shapes (H-10)

Added, none touching an existing function: `board_max_seq` (the board's
current high-water `seq`, used to seed the write-behind counter below),
`stroke_points_json`/`StoredStroke::into_stroke` (the `Stroke` ⇄ JSON
`points` column conversion `record_stroke`/`snapshot` need),
`insert_stroke_at_seq` (single-statement insert at an already-minted `seq` —
see H-21), `compact_board` (hard-delete cleared rows, trim live rows to
`keep_last` — the retention sweep `docs/HANDOFF.md` H-10 reserves for T1.6;
landed now because T2.3's own acceptance test, "the rows are gone after
compaction," needs it to exist and T1.6 has not merged yet) and
`hard_reset_board` (test-only: forget every row on a board, the DB-backed
replacement for v1's in-memory `reset_board`).

### H-21. `src/server/api/realtime.rs` — `record_stroke`/`clear_board`/`snapshot` swapped to the database, with one deliberate deviation

The swap itself is exactly what T1.2's own doc comment on these three
functions asked for. The deviation: **persisting a stroke's row does not
block publishing it.** T1.2's own load test (`tests/realtime_tests.rs`
`t1_2_3`, 8 clients × 30 msg/s × 30 s) requires p99 fan-out latency under
250 ms; awaiting a transactional insert (the original `db::insert_stroke`,
`SELECT MAX(seq)` + `INSERT` in one transaction) on the single write
connection before publishing measured **759 ms p99** under that exact load —
confirmed empirically, not assumed. `seq` is now minted from an in-process
`AtomicI64` (seeded once from `db::board_max_seq`, never touched by SQLite
again), and the row is written by a detached `tokio::spawn`ed task
(`db::insert_stroke_at_seq`) the publish does not wait for. `clear_board`,
`snapshot` and `undo_last_stroke` are unchanged in spirit — still `await` the
database directly — because none of them sit in the 240-messages/second hot
path `t1_2_3` exercises. Full reasoning is the module-doc comment directly
above `record_stroke` in `realtime.rs`; `docs/PROTOCOL.md` §5 records the one
externally-visible consequence (`ClearBoard`'s `seq` no longer advances,
because nothing was inserted for it to advance past).

Also added: `realtime::compact_board` (thin wrapper over
`db::compact_board`, using `MAX_RETAINED_STROKES`) and `realtime::reset_board`
(now `async`, DB-backed — see H-22).

### H-22. `tests/realtime_tests.rs` — mechanical fix, not a weakened assertion

Once `record_stroke`/`clear_board`/`snapshot` touch the database, the hub
this file's `hub_router()` boots needs one — it never did before (T1.2's own
in-memory `BoardState`). Two changes, both mechanical: (1) `reset_board()` is
now `async`, so its three call sites gained `.await` — a compile error
otherwise, `unused_must_use` under `-D warnings`; (2) added an
`init_test_env()` (verbatim the same shape as
`tests/profiles_tests.rs::init_test_env`) called from `spawn_hub()`, pointing
this binary's process at its own throwaway `DATABASE_URL` so it can never
collide with another test binary's data — every other integration test file
that touches `db::pool()` already does this; this was the one that didn't
yet need to. No assertion's meaning changed; `t1_2_3`'s own 250 ms budget is
the one this branch had to satisfy (see H-21) — it does, confirmed by a full
run of this file after the fix (12/12 passed, `t1_2_3` included).

### H-23. New files: `src/server/api/whiteboard.rs`, `tests/whiteboard_tests.rs`

`api::whiteboard::undo_last_stroke(client_id: String)` is a plain `#[server]`
fn, not a `ClientMessage`/`ServerMessage` wire addition — see its own doc
comment and `docs/PROTOCOL.md`'s new "`cleared_at` and undo" section for why
(no third editor needed on `src/shared/types.rs`, and undo is infrequent
enough that a fresh `Snapshot` republish is a complete, sufficient
notification). `src/server/api/mod.rs` gained the two lines every prior
module addition needed (`pub mod whiteboard;` + a re-export) — the same
mechanical addition T1.4/T1.5's own modules made when they landed.
`tests/whiteboard_tests.rs` is T2.3's acceptance suite proper (assertions
a–c); (d) and (e) are inline in `whiteboard.rs` itself — see that file's own
`#[cfg(test)] mod tests` doc comments.

### Deviations from `docs/PROTOCOL.md` §5, and why

`ClearBoard`'s `seq` no longer advances past the board's prior high-water
mark — a clear does not insert a row, so (unlike v1's in-memory model, which
minted one anyway) there is nothing for it to allocate a number *for*.
`docs/PROTOCOL.md` §5 has been updated to describe this directly rather than
leave the doc and the code disagreeing; the client never tracked `seq`
client-side to begin with (every `RequestSnapshot` it sends today is
`since_seq: 0`), so nothing observable changed.

Wave 2-a in progress: T2.3 done. T2.1, T2.2, T2.4, T2.5 unaffected — none
touch `src/server/api/realtime.rs`, `src/server/db.rs`'s whiteboard section,
or `src/client/components/whiteboard.rs`.

---

## Boss decisions at the wave 2-a close (T2.1, T2.2, T2.3 merged)

Merged in order T2.1 → T2.2 → T2.3 (squash), baseline green after each
(`cargo fmt --check`, both clippy targets with `-D warnings`,
`cargo test --features server`). T1.6, T2.4 and T2.5 were not part of this
closeout; their branches/worktrees are untouched. Note: all three sections
above reuse the numbers **H-20…H-25** — they are disambiguated by the section
they sit in ("T2.1 H-23" vs "T2.2 H-23"), not renumbered, because commits and
code comments already cite them that way.

**Review outcomes (no rejections).** No task weakened a test, added an
undeclared non-Rust component, or committed a secret. Edits outside the §P4
ownership table, each ratified:

- **T2.1 H-20 / T2.2 (`src/client/app.rs`, `src/client/components/mod.rs`):
  ratified.** Neither file has a §P4 row. T2.1 pointed `KioskDashboard` at
  `TvShell`; T2.2 pointed `Mobile` at `MobileShell` and moved the manifest
  link to its root URL. The only conflict was the `use` block, which Boss
  resolved (`Dashboard` and `Routine` imports are now unused there and were
  dropped). **Owner from here on: `src/client/app.rs` and
  `src/client/components/mod.rs` are Boss-only** — a wave-2b/3 task that needs
  a line in either writes the request here.
- **T2.2 H-20 (`src/server/router.rs`): ratified.** T0.6's own doc comments
  reserved the two stub bodies for T2.2 (same precedent as H-14/H-16); routes,
  paths and content types are unchanged, and `router_tests.rs` is untouched.
  T2.5 will see a three-line conflict in `build_router`'s route chain plus one
  `use`; Boss resolves it at T2.5's merge.
- **T2.3 H-20/H-22/H-23 (`db.rs` query shapes, `api/realtime.rs` board store,
  `tests/realtime_tests.rs`, `api/mod.rs`): ratified.** H-10 and
  `docs/PROTOCOL.md` both reserved the `realtime` board-store swap for T2.3;
  the test edits are `.await` on the now-async `reset_board()` plus the
  `DATABASE_URL`-per-process isolation every sibling suite already uses —
  mechanical, non-weakening, and T1.2's load test still passes.
- **T2.3 H-21 (write-behind `record_stroke`): ratified, with one residual.**
  The 759 ms → <250 ms p99 justification is measured against T1.2's protected
  test, so the design stands. Residual for T2.6 / the Fable QA loop: the
  detached inserts can commit out of `seq` order, so a client whose
  `RequestSnapshot` lands in the milliseconds between the publish of stroke
  *N* and its commit may bookmark `latest = N+1` and never be sent *N*. Every
  client connected at publish time already received *N* live, so the window
  only affects a socket opened inside it; T2.6's cross-surface test should
  include a draw-then-immediate-snapshot case and, if it bites, `snapshot`
  should return `latest` as the highest *contiguous* committed seq.

**Applied by Boss in this close:**

- **T2.1 H-21 / T2.2 H-21 — `assets/tailwind.css` rebuilt once** on the
  merged tree with the pinned 3.4.17 binary (T2.1 had rebuilt on its own
  branch; T2.2 deliberately had not). Same precedent as 1bc45d9.
- **T2.1 H-22 — `tv_clock()` moved** to a new `src/server/api/tv.rs`
  (re-exported from `api/mod.rs` as `api::tv_clock`, endpoint name unchanged).
  `client::components::tv::clock` keeps `CLOCK_POLL_SECS` and re-exports
  `tv_clock`/`TvClock`, so `tv::shell` did not change. `api/tv.rs` belongs to
  T2.1's owner (T3.4 styling-only, like the rest of `tv/**`).
- **T2.2 H-22 — `assets/manifest.json` deleted.** Nothing referenced it;
  `tests/pwa_tests.rs` asserts the rendered page does not mention it.
- **`docs/NON_RUST.md` `sw.js` row corrected** from "~40 lines" to the actual
  ~105 lines / 3.3 KB, with the 6 KB budget and the no-Background-Sync
  decision named. `sw.js` remains the only non-Rust component in the PWA.

**Left recorded (not applied), with the reason:**

- **T2.1 H-23 (server broadcasts `ServerMessage::Health` on a tick):
  scheduled for the 2-b boundary as a T1.2-owner micro-commit.** The wire
  shape exists but nothing on the server defines what `stale` means (the
  `/health` body has no such field), so this is a small design decision, not a
  mechanical apply. Guidance: spawn the 25 s heartbeat from `router::run` (not
  from `ws_handler`, so `tests/realtime_tests.rs`'s hub-only router stays
  silent and its "nothing arrives" assertions keep meaning that), with
  `stale = false` and `last_update = now` until a Google poll exists to be
  stale about. T2.1's `tv_clock()` poll stays until then.
- **T2.1 H-24 (setup code on the TV): decided — do not expose it to the
  kiosk over any endpoint or hydration payload.** The setup code stays in
  the log and `<data>\setup-code.txt` (T1.4 H-18). The join-QR overlay can
  gain the code only if T1.4's `parent_setup_code()` is gated to the HTTP
  kiosk listener; that is a wave-3 hardening item, recorded in
  `docs/RESIDUAL.md` when T3.2 consolidates.
- **T2.2 H-23 (the two `record_offline_failure` enqueues in
  `src/client/components/routine.rs`): deferred to T2.5's merge.** The file
  is T2.5's in this same wave, so Boss applies the exact patch as part of
  resolving T2.5's squash (or T2.5 applies it, having read this).
- **T2.2 H-24 (`web-sys` `Storage` feature): not needed.** The externs work
  and catch Safari's private-mode exception; no `Cargo.toml` change this
  wave.
- **`src/client/components/dashboard.rs` is now unreferenced** (neither
  `/tv` nor `/m` renders it). Left in the tree until T2.4 and T2.5 land in
  case either reads its calendar/photo-task panels; Boss deletes it at the
  2-b close if still unreferenced.
- **T2.3 → T1.6:** `realtime::compact_board(DEFAULT_BOARD_ID)` exists and is
  unit-proven; T1.6 registers it with `on_day_rolled` when its branch merges.

## From T1.6 (backup, retention, delete-with-file) → Boss (`src/server/router.rs`)

T1.6 owns `src/server/backup.rs` only (§P4), plus the two small "retention
fns" §P4 explicitly allows it to add to `db.rs`
(`db::delete_custom_task_row`, `db::compact_strokes`). Everything else it
needed — the nightly backup, stroke compaction, photo retention, log
rotation — is implemented and unit/integration-tested standalone.

### H-15. Something on the startup path must call `backup::register_nightly_hooks()`

`docs/HANDOFF.md`'s own T1.1→T1.6 note (H-10) says to "register work on the
midnight tick with `realtime::on_day_rolled(hook)` rather than editing the
loop", so `backup.rs` does exactly that:
`backup::register_nightly_hooks()` calls
`server::api::realtime::on_day_rolled(...)` with a hook that runs the
nightly backup, then stroke compaction, then photo retention, then log
rotation, in that order, once per day-roll.

But *registering* the hook is not the same as it ever running: one call to
`backup::register_nightly_hooks()` has to happen somewhere on the startup
path, exactly like T1.2's `realtime::ensure_background_tasks()` did (H-7,
resolved at the wave 1-a close by adding one line to `router::run`).
`src/server/router.rs` is T0.6/T1.3-owned and T1.6 does not touch it.

Request: add, next to the existing
`crate::server::api::realtime::ensure_background_tasks();` line in
`router::run`:

```rust
crate::server::backup::register_nightly_hooks();
```

It is idempotent to call more than once (each call adds another hook
closure, so calling it twice would run the sweep twice per day-roll — call
it exactly once, same as the existing line). Until this is wired in, the
retention jobs are fully implemented and tested (see
`tests/backup_tests.rs`) but do not run automatically; nothing depends on
them running for T1.6's own acceptance, which drives every function
directly.

### H-16. Log file path assumption, for T3.1

`backup::rotate_log_if_needed` is generic over any log file path; the
nightly sweep (`nightly_maintenance` in `backup.rs`) calls it against
`<data>\logs\familyhub.log`. T3.1 owns the actual service logging setup —
if the service's log writer uses a different file name, either point it at
`familyhub.log` or have T3.1 call `backup::rotate_log_if_needed` directly
with its own path instead of relying on the nightly sweep's hard-coded one.

## Boss decisions at the Recovery close (T1.6 squash-merged to `main`)

T1.6's branch (`phase-1/T1.6`, dd38499, forked from 1bc45d9) was merged
after waves 1-b and 2-a. Its structured report was lost, so the branch was
reviewed against PURPLE §P3 T1.6 (a)-(f) directly: every letter has a named
test in `tests/backup_tests.rs`, and the whole suite is green on `main`.

- **T1.6 H-15 (`router::run` calls `backup::register_nightly_hooks()`) —
  applied.** One line, next to `realtime::ensure_background_tasks()`, so
  the nightly backup → stroke compaction → photo retention → log rotation
  sweep now actually runs on the midnight tick.
- **T1.6 H-16 (log file name for T3.1) — recorded, not applied.** T3.1
  owns the service log writer; it must either write to
  `<data>\logs\familyhub.log` or call `backup::rotate_log_if_needed`
  with its own path. Carried forward as a T3.1 input.
- **Duplicate stroke-compaction SQL resolved in favour of T2.3's
  `db::compact_board`.** T1.6 had added an identical two-pass
  `db::compact_strokes` in the same wave T2.3 landed `compact_board` (H-20
  above). Only `compact_board` (transactional) survives in `db.rs`;
  `backup::compact_strokes` is a thin delegate to it. `db::delete_custom_task_row`
  is the one retention fn T1.6 still adds to `db.rs`.
- **`tests/backup_tests.rs` restore drill expects `migration_version == 3`**
  (was 2 on the branch) — the same mechanical bump H-17 applied to
  `storage_tests.rs`; not a weakened assertion.
- **Numbering:** T1.6's H-15/H-16 collide with T1.4's H-15/H-16 above.
  Per-section numbering is already how T2.1/T2.2/T2.3 read; left as is,
  qualified by section wherever referenced.
- **`phase-2/T2.7` is deliberately not merged yet.** It was built before
  T2.5 existed and will be reconciled once T2.5 lands.
- `.gitignore` now excludes `.claude/worktrees/` (agent worktrees are
  checkouts, never content).

---

## From T2.4 (Calendar v2) → Boss

T2.4 owns `src/server/calendar.rs`, `src/server/api/calendar.rs` and
`src/client/components/calendar.rs` (§P4). Everything below is either a file
outside that set that had to change, or a request for one that did not.

### H-20. `Cargo.toml`: `rrule = "=0.14.0"` added (§P4 says Boss serialises this)

T2.4's acceptance (b) and (d) are *about* `rrule`, and §P5.4 pins it exactly,
so the task cannot be done without the dependency. Added as an `optional`
dependency inside the `server` feature only — recurrence is expanded on the
hub and the surfaces receive concrete occurrences, so the wasm bundle never
carries `chrono-tz`'s timezone database. The only other outstanding wave-2a
branch is T2.5, which needs no new crate (`image` and axum `multipart` are
already in), so the expected conflict at merge is the `[dependencies]` block
plus one `server = [...]` line — resolve by keeping both sides.

Precedent: T1.3 and T1.4 added their own pinned crates on their own branches
(the `# T1.3 —` / `# T1.4 —` comment blocks in `Cargo.toml`) and recorded it
here; this follows that shape, including the comment block naming the pin's
trap (`all(limit)`, never `all_unchecked()`).

### H-21. `src/server/api/mod.rs`: five new re-exports

The same mechanical addition every previous module made
(`pub use calendar::{create_local_event, delete_local_event, get_calendar_week,
get_events_for_day, get_today_events, update_local_event};`). `get_today_events`
keeps its name, its endpoint and its signature, so `tv::shell` and every other
call site are untouched.

### H-22. → T2.1 (`src/client/components/tv/**`): the kiosk still renders `is_empty()`

W3's `Loading`/`Empty`/`Error` state machine is landed for the phone and the
dashboard in `client::components::calendar::CalendarState`, and it is unit- and
integration-tested. The **television** still cannot show the difference:
`TvModel.events` is a bare `Vec<CalendarEvent>` and `tv::surface::calendar_panel`
falls back to `if events.is_empty()`, so a failed fetch on the kiosk looks
exactly like an empty day. Fixing that means touching `tv/model.rs`,
`tv/shell.rs` and `tv/surface.rs` (T2.1's files, and `tests/golden/` pins the
focus order), which T2.4 may not do.

Requested change, small and mechanical:

1. `TvModel.events: Vec<CalendarEvent>` → carry the state alongside it, e.g.
   `pub events: CalendarState<Vec<CalendarEvent>>` (the enum is public and
   generic, and `CalendarState::resolve` is its single constructor).
2. `tv::shell` builds it from its existing `events_resource`:
   `CalendarState::resolve(resource.read_unchecked().clone().map(|r| r.map_err(|e| e.to_string())), Vec::is_empty)`.
3. `calendar_panel` matches the four arms; `body_order` returns no focus ids
   for `Loading`/`Error`/`Empty` (it already returns none for an empty list, so
   the golden file does not move).

Until then the kiosk's behaviour is unchanged from what T2.1 shipped — no
regression, just the half of W3 that lives in a file T2.4 does not own.

### H-23. → Boss: `assets/tailwind.css` needs the wave-2a rebuild to include this branch

`client::components::calendar` adds utilities the committed CSS does not carry
(`sm:grid-cols-7`, `bg-red-50`, `border-red-500`, `text-red-700/600`,
`ring-red-200`, `bg-red-600`, `uppercase`, `tracking-wide`). Following T2.2's
choice (and T2.1 H-21 / T2.3's note), this branch deliberately does **not**
rebuild the generated file — Boss rebuilds it once on the merged tree with the
pinned 3.4.17 binary:

```powershell
& "$env:USERPROFILE\.cargo\bin\tailwindcss.exe" -i input.css -o assets/tailwind.css --minify
```

### H-24. → T1.7 (`src/server/health.rs`): one stale doc reference

`health.rs:165` still says "once per successful `fetch_today` inside
`store_events`". Both functions are gone: the call is now once per successful
window replace in `calendar::spawn_polling_task`'s loop, and it is still made.
Doc-comment only; no behaviour change, and T2.4 did not edit the file.

### Decisions T2.4 made that later tasks should know

- **Storage frame of reference.** `events.starts_at` / `ends_at` are
  **server-local wall clock** as `YYYY-MM-DDTHH:MM:SS`, never RFC3339 and never
  UTC. All-day events are stored at `T00:00:00` with `all_day = 1` rather than
  as a bare `YYYY-MM-DD`, so the window `DELETE` and every range `SELECT` stay
  plain lexicographic comparisons and all-day rows sort with the rest.
- **Wire ids are `{source}:{row id}@{occurrence start}`.** Two occurrences of
  one recurring event are therefore distinct (the TV uses this for focus ids),
  and the phone can tell a deletable local event from a Google one without a
  second round trip. `tv::model::slugify` already reduces it safely.
- **Google rows are not editable through the local CRUD path.**
  `update_local_event` and `delete_local_event` are `... AND source = 'local'`;
  the next window replace would undo any local edit, so the server returns
  "no local event" rather than pretending.
- **The window is ±(7, 60) days** (`WINDOW_PAST_DAYS` / `WINDOW_FUTURE_DAYS`),
  replaced whole on every poll, with `singleEvents=true` and **no `syncToken`
  and no `orderBy`** — ordering happens in `parse_events_response`, so R-19's
  parameter conflict cannot come back.
- **Local CRUD is parent-gated server-side** (`auth::require_session`), matching
  §P5.5 default 35's "calendar editing … phone-only"; reads stay
  unauthenticated because the TV holds no session and must render the day.
- **The forced midnight poll is observable.** `calendar::poll_requests()`
  counts them, which is how the W4 assertion is made without credentials or a
  network; `register_midnight_poll()` is idempotent and is called from
  `spawn_polling_task` whether or not credentials exist.
- **ICS import stays cut** (R-25). `icalendar` is not in the tree.
- **Fixtures are test data, not stack.** `tests/fixtures/google_events_window_{3,2}.json`
  are committed Google `events.list` bodies, in the same category as
  `family_v1.db` and `photo_12mp.jpg`; no `docs/NON_RUST.md` row is needed and
  none was added.

### H-25. → Boss / T2.6: `tests/loop_tests.rs` is flaky on `main`, ~15 % of runs

Measured while gating T2.4, because it turned one full-suite run red and it is
**not** T2.4's doing. Both branches, same machine, same session, `cargo test
--features server --test loop_tests` run back to back:

| Tree | Runs | Failures |
| --- | --- | --- |
| `41eb990` (this branch's base, i.e. `main`) | 12 | **2** |
| `phase-2/T2.4` | 12 | 3 |

The failure is always the same assertion, and always only that one:

```
thread 't2_6_phone_drives_the_tv_across_a_server_restart' panicked at tests\loop_tests.rs:385:17:
phone's post-restart Snapshot must still carry the stroke drawn before the restart
```

This is exactly the residual Boss recorded at the wave 2-a close under
"**T2.3 H-21 (write-behind `record_stroke`): ratified, with one residual**":
`realtime::record_stroke` returns the `seq` and `tokio::spawn`s the insert, so
a stroke is published before it is committed. `loop_tests` step 3 draws, step 4
aborts the server and reconnects, and nothing in between waits for that
detached insert — so when the machine is busy (a cold binary, a parallel
build), the post-restart `Snapshot` can be taken before the row lands.

T2.4 did not touch `api/realtime.rs`, `db.rs`'s whiteboard section, the board
store or `loop_tests.rs`, and every calendar suite is deterministic. The
difference between the two rows above is noise on a 12-run sample of the same
race, not a regression.

Concrete fix, for whoever owns it (T2.3's file, T2.6's assertion — Boss to
assign): make the persistence observable instead of hoping. Either

* have `record_stroke` await the insert (it is one row on the write pool; the
  759 ms → <250 ms p99 justification for going write-behind was measured
  against T1.2's load test, which is a different, much hotter path), or
* keep the write-behind and have `snapshot` return the highest **contiguous
  committed** `seq` — which is the fix Boss already sketched — so a snapshot
  can never bookmark past a row that has not landed.

The first is a two-line change and removes the class of bug; the second keeps
the throughput and needs a query change. Either way the assertion stands
unchanged: it is describing correct behaviour, and it is the code that is
wrong.

---

## From T2.5 (photo tasks v2)

T2.5 owns `src/server/router.rs` this wave (§P4) and `src/client/components/routine.rs`,
plus new files it created (`src/server/api/photos.rs`, `tests/photo_tests.rs`).
Everything below is an edit to a file this task does not own, each narrowly
scoped and made only because the acceptance test (PURPLE §P3 T2.5 (a)-(f))
cannot pass without it — the same "the file wasn't available and the seam
was pre-authorised or unavoidable" reasoning T1.3/T1.4/T2.2/T2.3 record above.

### H-26. `src/server/db.rs` — `custom_tasks()` gained the auto-hide filter; one new additive function

T1.1's `0002_core.sql` comment named `custom_tasks.due_date` as "T1.5 /
T2.5", and this task's brief is explicit: "due_date on custom tasks (column
exists from 0002_core) with daily auto-hide" is a T2.5 deliverable, not a
request to file elsewhere. `db.rs` is T1.1-owned with T1.5/T1.6 as later
editors (§P4); no row lists T2.5, so this is recorded here for the same
after-the-fact ratification T1.3 H-8/T1.4 H-15 were given, not applied
unprompted as if it were uncontroversial.

Two changes, both additive to the schema/behaviour, neither touching an
existing call site's signature:

* `custom_tasks(pool, user_id)` now also selects `due_date` and filters
  `WHERE due_date IS NULL OR due_date >= today` (today computed server-side
  from `chrono::Local::now()`, never a caller's clock — PURPLE §P5.5 default
  14). Every existing call site (`api::routine::get_custom_tasks`,
  `tests/backup_tests.rs`, `tests/db_tests.rs`, `tests/routine_tests.rs`)
  only ever inserted tasks with `due_date = NULL`, so the filter is a no-op
  for every pre-existing test — confirmed by the full suite still being
  green on those files.
* New `insert_custom_task_with_due_date(pool, user_id, title, photo_path,
  due_date)` — used only by `api::photos::upload_photo_handler`, which
  writes its own already-re-encoded `photo_path` rather than decoding base64
  like `insert_custom_task` does. `insert_custom_task` itself (the base64
  path's function) is untouched.

### H-27. `src/shared/types.rs` — `CustomTaskView` gained `due_date: Option<String>`

Owned by T1.2, later edited by T1.4; no T2.5 row. Additive field (`#[serde(default)]`
for forward/back compatibility), needed so a client can ever see a task's
due date at all. The two pre-existing construction sites this broke
(`src/client/components/tv/fixture.rs`'s golden-render fixture, T2.1-owned)
got a mechanical `due_date: None` — the same class of change H-17 made to
migration-version constants, not a behavioural edit.

### H-28. `tests/http_tests.rs` — one T0.3 test updated for a renamed function, substance preserved

`Gate-2 assertion 10` (`migration_file_input_handler_takes_vec_file_data`)
called `client::components::routine::encode_first_photo`, which returned a
base64 `String` for the retired base64-through-a-`#[server]`-fn upload path
(G14 — exactly what this task replaces). It is renamed
`read_first_photo` and now returns `(mime, Vec<u8>)` — what the multipart
route's client-side downscale needs instead of a base64 blob. The test was
updated to call the new name and assert the tuple; what it actually proves
(`Vec<FileData>` compiles and reads back through the 0.7 shape) is
unchanged, same mechanical-update precedent as H-17b.

### H-29. `Cargo.toml` — `[profile.dev.package.*]` opt-level overrides for the image codecs

PURPLE §P3 T2.5(a) requires the 12 MP fixture upload to complete in < 3 s.
Under `cargo test`'s default unoptimized build, `image`'s pure-Rust decoders
(`zune-jpeg`, `png`) took **5+ seconds** just for decode+resize+encode on
this box — confirmed by profiling before adding the override (`upload took
5.0828851s`). This is not `api::photos::upload_photo_handler`'s own code
being slow; it is `opt-level = 0` on two pixel-crunching dependencies.

Added an idiomatic fix that touches no release behaviour (`[profile.release]`
already builds `opt-level = "z"`): per-package opt-level overrides for
`image`, `zune-jpeg`, `zune-core`, `png`, `image-webp` under
`[profile.dev.package.*]`, which `cargo test`'s `test` profile inherits.
With this, the whole T2.5 suite (including the 12 MP fixture) runs in well
under a second. `Cargo.toml` is T0.2/T0.4-owned with later additions routed
through a Boss micro-commit (§P4) — same basis T1.3 H-8/T1.4 H-15 record for
their own needed-now additions; requesting ratification here.

### Residual, not caused by this task: `tests/whiteboard_tests.rs::t2_3_a_five_hundred_strokes_persist_and_replay_in_seq_order` is flaky on this machine

Discovered while running the full baseline for this task's own DONE check.
**Verified against the unmodified `main` tip (`41eb990`, this task's own
changes fully `git stash`ed) that the same test fails the same way** — `cargo
test --features server --test whiteboard_tests` on a clean checkout:
`left: 24, right: 500` ("every stroke must be persisted, not merely
broadcast"). It passes when run alone and filtered
(`--test whiteboard_tests t2_3_a_five_hundred -- --test-threads=1`), so this
looks like shared process state between the three tests in that binary (all
share one `db::pool()`/`realtime::sender()`), not a T2.5 regression — `git
diff` for this task touches none of `src/server/api/realtime.rs`,
`src/server/db.rs`'s stroke functions, or `src/client/components/whiteboard.rs`.
Consistent with the residual the wave 2-a Boss note already flagged under
T2.3 H-21 ("the detached inserts can commit out of `seq` order... T2.6's
cross-surface test should include a draw-then-immediate-snapshot case").
Flagging for whoever next owns `tests/whiteboard_tests.rs` / T2.3's
write-behind insert — not fixed here (out of this task's file ownership and
out of scope for photo tasks).

**Recurred during `phase-qa1/T1.4`'s own baseline** (QA round 1 fixes:
Q1-02/Q1-03/Q1-11, none of which touch `src/server/api/realtime.rs`'s stroke
path, `src/server/db.rs`'s stroke functions, or `src/client/components/
whiteboard.rs`): two full `cargo test --features server` runs both failed
`t2_3_a_five_hundred_strokes_persist_and_replay_in_seq_order` (the second run
also flaked `t2_3_c_undo_removes_only_the_callers_own_last_stroke`), while
`cargo test --features server --test whiteboard_tests` run alone passed 3/3.
Still Q1-09's write-behind race (`docs/qa/QA_ROUND_1.md`), still out of T1.4's
file ownership — noted here rather than fixed, per this task's instruction to
touch only what Q1-02/Q1-03/Q1-11 flagged.

---

## Boss decisions at the wave 2-a rerun close (T2.4, T2.5 merged)

Merged in order T2.4 → T2.5 (squash), full gate green after each
(`cargo fmt --check`, both clippy targets with `-D warnings`,
`cargo test --features server` — 132 lib + every integration suite,
including `calendar_tests` 14/14 and `photo_tests` 6/6). T2.7's branch and
worktree are untouched (wave 2-b). The two merge conflicts were exactly the
ones T2.4 H-20 predicted: `src/server/api/mod.rs` (both sides kept —
T2.4's six calendar re-exports plus T2.5's `photos::delete_custom_task`) and
the two appended `docs/HANDOFF.md` sections (both kept, T2.4 first).
`Cargo.toml` auto-merged (rrule in `[dependencies]`, the profile overrides
after `[profile.release]`).

**Review outcomes (no rejections).** No task weakened a test or committed a
secret. Edits outside the §P4 ownership table, each ratified:

- **T2.4 H-20 (`Cargo.toml`: `rrule = "=0.14.0"`, server feature only):
  ratified.** Same shape as T1.3/T1.4's pinned blocks; `all_unchecked()` is
  absent from the tree (grep-verified at review).
- **T2.4 H-21 (`src/server/api/mod.rs` re-exports): ratified.**
  `get_today_events` keeps name, endpoint and signature.
- **T2.5 H-26 (`db.rs`: `custom_tasks()` auto-hide filter +
  `insert_custom_task_with_due_date`): ratified.** `0002_core.sql` named the
  column as T1.5/T2.5's; the filter is a no-op for every pre-existing test
  (all insert `due_date = NULL`).
- **T2.5 H-27 (`shared/types.rs` `CustomTaskView.due_date`, `tv/fixture.rs`
  mechanical `None`s): ratified.** Additive, `#[serde(default)]`.
- **T2.5 H-28 (`tests/http_tests.rs` Gate-2 assertion 10): ratified.** The
  rename `encode_first_photo` → `read_first_photo` follows the retired base64
  path; the assertion still proves `Vec<FileData>` reads back through the 0.7
  shape and now checks mime + raw bytes instead of a base64 string. Not a
  weakening.
- **T2.5 H-29 (`Cargo.toml` `[profile.dev.package.*]` opt-level 3 for
  `image`/`zune-jpeg`/`zune-core`/`png`/`image-webp`): ratified.** Dev/test
  only; `[profile.release]` unchanged. Without it the 12 MP fixture cannot
  meet §P3 T2.5(a)'s < 3 s under `cargo test`.
- **T2.5 `assets/tailwind.css` (edited without a HANDOFF note): superseded.**
  It is a generated file that Boss rebuilds at every close (below), so the
  branch's version was simply overwritten. Rule restated for wave 2-b/3:
  do not commit `assets/tailwind.css` on a task branch; ask for the rebuild.
- **T2.5 `wasm_bindgen(inline_js)` snippet in `routine.rs` (~45 lines of
  JS): accepted, with a `docs/NON_RUST.md` row added by Boss.** It is the
  first hand-written JS in the tree besides `sw.js`. The code comment
  declares it under the `wasm-bindgen` glue exception and the reasoning is
  sound (a `web-sys` pipeline needs half a dozen feature flags on a
  Boss-owned `Cargo.toml`; the server re-sniffs and re-encodes regardless),
  but a hand-written snippet is a component, not glue, and needs its own
  row. **Rule for later waves: any new `inline_js` lands with its
  `docs/NON_RUST.md` row in the same commit, or it is rejected.**
- **T2.2 H-23 (offline enqueue on both toggle-failure paths in
  `routine.rs`): applied by T2.5, as deferred at the previous close.**
  Closed.

**Applied by Boss in this close:**

- **T2.4 H-23 — `assets/tailwind.css` rebuilt once** on the merged tree with
  the pinned 3.4.17 binary (`tailwindcss.exe -i input.css -o
  assets/tailwind.css --minify`); `grid-cols-7`, the `red-*` error-box
  utilities, `uppercase` and `tracking-wide` verified present.
- **T2.4 H-24 — `src/server/health.rs` doc comment** on
  `record_google_poll_success` no longer names `fetch_today`/`store_events`;
  it now says where the call actually lives. Doc-only.
- **`docs/NON_RUST.md`** gained the `inline_js` row described above.

**Left recorded (not applied), with the reason:**

- **T2.4 H-22 (TV kiosk still renders `is_empty()`; `TvModel.events` →
  `CalendarState<Vec<CalendarEvent>>`): scheduled for the 2-b boundary as a
  T2.1-owner micro-commit.** It is mechanical but touches `tv/model.rs`,
  `tv/shell.rs`, `tv/surface.rs`, `tv/fixture.rs` and every `TvModel`
  construction in `tests/tv_tests.rs`, so it is not a one-line apply. T2.4's
  three-step recipe stands; the golden focus file should not move.
- **T2.4 H-25 + T2.5's residual (`loop_tests` post-restart Snapshot flake,
  ~15 %; `whiteboard_tests` 500-stroke count flake): one root cause,
  T2.3's write-behind `record_stroke`.** Both suites passed in both full runs
  at this close, so not blocking, but the flake is real and measured.
  **Decision: fix by awaiting the insert (T2.4's option 1)** as a
  T2.3-owner micro-commit at the 2-b boundary, then re-measure
  `t1_2_3`'s p99 — if it regresses past the 250 ms budget, fall back to the
  contiguous-committed-seq `snapshot` (option 2). Neither assertion is to be
  weakened.
- **`src/client/components/dashboard.rs`** is still unreferenced (neither
  T2.4 nor T2.5 reads it); deleted at the 2-b close as already planned.
- **T2.1 H-23 (server `Health` heartbeat)** remains scheduled for the 2-b
  boundary, unchanged.

**Worktrees:** `.claude/worktrees/wf_d57bfb45-d60-2` (T2.4) and
`wf_d57bfb45-d60-3` (T2.5) removed; `wf_a4f253d4-9d7-32` (T2.7, unmerged)
kept.

---

## T2.7 (screensaver completion) — request for Boss ratification

T2.7's own files (`src/server/api/screensaver.rs`, owned by T2.7 since T1.2's
split; `src/client/components/screensaver.rs`, T0.7→T2.7; `tests/screensaver_tests.rs`,
new) needed a `MaximizedView::Screensaver` variant to carry the plan's
"optional scheduled `SetView(Screensaver)`" over the wire — that type lives
in `src/shared/types.rs`, owned solely by T1.2, with T1.4 as the only listed
later editor. There is no way to add a new value to a wire enum without
touching its definition, so this task made the following **minimal,
additive** edits outside its ownership and is logging them here per the
"append a request instead of editing" rule, rather than leaving the feature
half-built:

- **`src/shared/types.rs`**: added `MaximizedView::Screensaver` as a fifth,
  purely additive variant (doc comment explains it). Nothing existing reads
  or writes it, so no other behaviour changed.
- **`src/client/components/tv/model.rs`** (T2.1's file): `TvPanel::from_view`
  was an exhaustive match with no wildcard arm; added `MaximizedView::Screensaver`
  to the existing `Routine | None => TvPanel::Routine` arm (one line). Does
  not change any existing test's outcome — no test constructs this variant
  — and does not touch the golden focus-order file.
- **`src/client/components/dashboard.rs`** (unowned dead code per the wave
  2-a close note above — "now unreferenced… Boss deletes it at the 2-b close
  if still unreferenced"): the two exhaustive matches over `MaximizedView`
  (`panel_title`, and the maximized-panel body) needed an arm each; both
  fold `Screensaver` into the same treatment as `None` (unreachable in
  practice since this component is dead code).
- **`src/server/api/mod.rs`**: added `upload_screensaver_image` to the
  existing `pub use screensaver::{...}` re-export line — the module's own
  doc table already lists `screensaver` as "T2.7" so this is the expected
  shape of a T2.7 change to this file, not a new ownership claim.

**Please ratify or revert.** If reverted, `MaximizedView::Screensaver` and
its three call sites need to come out together (they only exist for each
other), and the schedule half of T2.7's acceptance test (§P3 (d): "with the
schedule disabled, no `SetView` is emitted at the configured hour") would
need to be re-expressed without a wire type — still possible (the pure
`evaluate_schedule`/`due` functions in `src/server/api/screensaver.rs` don't
strictly require the variant to exist to prove "disabled ⇒ no emission",
only to prove what *would* be emitted if enabled), but the enabled path
would no longer type-check against a real `ServerMessage`.

**T2.7 → T2.5, restated for whoever lands T2.5.** This task's title says
"phone upload route reusing T2.5's pipeline", but T2.5 had not merged when
this task ran (`docs/PLAN.md` wave 2-a lists it alongside T2.1–T2.4; the
2-a close above explicitly says "T1.6, T2.4 and T2.5 were not part of this
closeout"). `upload_screensaver_image` in `src/server/api/screensaver.rs` is
therefore this task's own, self-contained allowlist+re-encode pipeline (base64
in, `image::guess_format` sniff, jpeg/png/webp allowlist, re-encode to jpeg),
shipped as a Dioxus `#[server]` fn rather than a raw `axum::extract::Multipart`
route specifically so it needs no change to `src/server/router.rs` (owned by
T0.6 → T1.3 → T2.5, not T2.7). When T2.5 lands its own allowlist/re-encode
helper for photo tasks, the two are good candidates to unify into one shared
function — neither currently depends on the other's files.

**Production wiring gap, logged rather than worked around.** The optional
schedule's background loop (`screensaver::ensure_background_tasks`) self-starts
the first time any screensaver server fn runs, because the reliable place to
start it — `src/server/router.rs::run`, the same way T1.2's midnight tick
starts at boot (H-7) — is not a file T2.7 owns. Whoever next owns `router.rs`
(T2.5, this wave) can add one `crate::server::api::screensaver::ensure_background_tasks();`
call next to the existing `realtime::ensure_background_tasks();` line and
delete the self-start comment in `screensaver.rs`; behaviour is identical
either way since the schedule defaults to disabled.

**No phone Settings UI wired.** `src/client/components/mobile/settings.rs`
(T2.2's file) has no "ambient photos" upload control yet — T2.7's acceptance
test only requires the route itself (`tests/screensaver_tests.rs` exercises
it directly over HTTP), and this task's file ownership does not include
`mobile/**`. Whoever owns that file next can add a file input that `POST`s a
`multipart/form-data` body (field name `photo`) to
`/api/upload_screensaver_image` — the same shape `tests/photo_tests.rs`'s
`post_multipart` helper builds, since (below) this is now a raw axum route,
not a `#[server]` fn callable directly from client Rust.

---

## T2.7 reconciled with T2.5 (wave 2-b)

The request directly above ("phone upload route reusing T2.5's pipeline")
is now applied, on the same branch, once `src/server/api/photos.rs` (T2.5)
existed to reuse:

- **`src/server/api/photos.rs`** (T2.5-owned; edited here because the
  reusable step could not be extracted without touching the file it lives
  in): `store_photo`'s sniff/decode/downscale/re-encode body moved into a
  new `pub(crate) fn sniff_downscale_reencode(bytes) -> Result<ReencodedImage,
  Box<Response>>`, pure with respect to storage. `store_photo` itself is now
  a thin wrapper that calls it and then writes under `upload_dir()` with its
  own `task-<user_id>-<stamp>.<ext>` naming — behaviour and every T2.5
  acceptance assertion (`tests/photo_tests.rs`, all six still green)
  unchanged.
- **`src/server/api/screensaver.rs`** (T2.7-owned): the old base64
  `#[server] fn upload_screensaver_image` is gone. In its place,
  `pub async fn upload_screensaver_image_handler(Multipart) -> Response`
  parses one `photo` field, calls `photos::sniff_downscale_reencode`
  (no second allowlist/re-encode implementation), writes
  `upload-<uuid>.<ext>` under `screensaver_dir()`, and returns the refreshed
  image list — same JSON shape `list_screensaver_images` already returned.
  `list_screensaver_images`'s directory-listing loop is factored into
  `images_in_dir`, shared by both. `ensure_background_tasks()` (the optional
  schedule loop) is unchanged.
- **`src/server/router.rs`** (owned T0.6 → T1.3 → T2.5, not T2.7 — but a raw
  axum route can only be registered here, and this task's own brief is the
  reconciliation this edit performs): added `POST
  /api/upload_screensaver_image` next to T2.5's `POST /api/upload_photo`,
  with the same `DefaultBodyLimit::max(25 MiB)`; `/assets/screensaver` is now
  its own tiny `Router<()>` (`screensaver_router`, mirroring `uploads_router`
  exactly) wrapped in the same `uploads_security_headers` middleware, so
  every screensaver photo now carries `nosniff`/`attachment` the same way
  `/uploads` does. Also added the one-line
  `crate::server::api::screensaver::ensure_background_tasks();` call to
  `run()`, next to T1.2's and T1.6's equivalents — closing the "production
  wiring gap" the first pass of this task logged above (self-start via
  `OnceLock` still stands as the harmless fallback).
- **`src/server/api/mod.rs`**: `pub use screensaver::{list_screensaver_images,
  upload_screensaver_image}` narrowed to `list_screensaver_images` only —
  `upload_screensaver_image_handler` is referenced from `router.rs` by full
  path, the same way `photos::upload_photo_handler` already is, not
  re-exported through the server-fn prelude.
- **`tests/screensaver_tests.rs`**: the upload tests now `POST
  multipart/form-data` (hand-built `Part`/`multipart_body`/`post_multipart`
  helpers, copied from `tests/photo_tests.rs` for the same reason given
  there — own test binary, `Cargo.toml` not owned by this task). A new fifth
  test (`screensaver_images_are_served_with_nosniff_and_attachment`) covers
  the reused headers. All four/five pass; no acceptance assertion was
  weakened — the plan-level acceptance ("uploading a new image makes it
  appear in the list") only ever specified behaviour, never wire format.

**Please ratify or revert**, same as the request above. Full baseline
(`cargo fmt --check`, `cargo clippy --features server --all-targets -- -D
warnings`, `cargo clippy --features web --target wasm32-unknown-unknown --
-D warnings`, `cargo test --features server`) is green on this branch,
`screensaver_tests.rs` and `photo_tests.rs` both included.

---

## Boss close — wave 2-b (T2.7 reconcile)

**Merged:** `T2.7` (squash of `phase-2/T2.7`, rebased on `main` at
`454d7ad`). Reviewed against PLAN §3 / PURPLE §P3 T2.7: (a) list ≥ 3 with
every URL 200 `image/jpeg` — `tests/screensaver_tests.rs`; (b) upload appears
in the list — same file, multipart `POST /api/upload_screensaver_image`;
(c) idle state machine fires at exactly 600 s — `IdleTracker` unit tests in
`src/client/components/screensaver.rs`; (d) disabled schedule emits nothing
at any hour — `schedule_tests` in `src/server/api/screensaver.rs`. No test
weakened (`tests/photo_tests.rs` untouched and green), no new non-Rust
component, no secrets. T2.6 had already merged at the 2-a close.

**Ratified, both T2.7 requests above:** `MaximizedView::Screensaver`
(`src/shared/types.rs`, T1.2's file) with its `tv/model.rs` arm; the
`pub(crate) sniff_downscale_reencode` extraction in `src/server/api/photos.rs`
(T2.5's file, behaviour unchanged); the `router.rs` edits (screensaver
multipart route on the same 25 MiB limit, `screensaver_router` under the
same `nosniff`/`attachment` middleware, `screensaver::ensure_background_tasks()`
at boot). One allowlist/re-encode implementation now exists in the tree.

**Applied at this close:**

- **`src/client/components/dashboard.rs` deleted**, as scheduled at both
  2-a closes — still unreferenced after T2.4/T2.5/T2.7 (the only mentions
  were `pub mod dashboard;` and an `app.rs` doc comment; both updated).
  T2.7's two `Screensaver` arms in it went with it.

**Left recorded (not applied), with the reason:**

- **T2.7 — no phone Settings upload control.** `mobile/settings.rs` (T2.2)
  still has no "ambient photos" file input; the route is proven over HTTP
  only. Whoever next owns `mobile/**` adds a file input that `POST`s
  `multipart/form-data` (field `photo`) to `/api/upload_screensaver_image`.
- **T2.7 — the schedule has no enable path.** `ScreensaverSchedule::default()`
  is the only instance ever constructed (`enabled: false, hour: 22`), so the
  loop is provably inert but a family cannot opt in yet. A `[screensaver]
  schedule_hour` key in `familyhub.toml` (T0.5's `FamilyHubConfig`) is the
  natural home; wave-3 item, not a 2-b apply.
- **T2.1 H-23 (server `Health` heartbeat), T2.4 H-22 (`TvModel.events` →
  `CalendarState`), T2.4 H-25 / T2.5 residual (await the stroke insert),
  T1.4 H-19 (`Set-Cookie` login route):** all still scheduled as owner
  micro-commits; none is a one-line apply. Unchanged from the 2-a rerun
  close.

**Worktrees:** `.claude/worktrees/wf_d57bfb45-d60-5` (T2.7, merged) removed.
No other task worktree remains; the `worktree-*` placeholder branches carry
no checkout.

---

## From T3.1 (Windows service) → T3.2 (runbooks)

`family-hub.exe` is a new `[[bin]]` target (`src/bin/family_hub.rs`,
`required-features = ["server"]`) — a separate executable from the Dioxus
fullstack app's own frozen `src/main.rs`, per `docs/reviews/PURPLE_TEAM.md`
§P4. Subcommands: `install|uninstall|start|stop|status|run|tv-probe`, all via
`windows_service::service_manager` (no PowerShell scripts). `run` is the
foreground/console mode (also what a developer uses without installing the
service); the SCM launches the same binary with no arguments, which
`try_run_as_service` in `server::service` detects.

For **T3.2 / `docs/OWNER_CHECKLIST.md` A3** (elevated, owner-run — this task
deliberately did not attempt a real install): the exact command is
`family-hub.exe install`, run from an elevated prompt, from wherever the
release binary was placed (it registers the service pointed at
`std::env::current_exe()`, so it must already be at its permanent location
before installing). It configures three firewall rules (`netsh advfirewall
firewall add rule name=FamilyHub ...`, TCP 8080/8443 + UDP 5353) and the AC
power plan (`powercfg /change {standby,hibernate,monitor}-timeout-ac 0`),
both best-effort — a failure there is logged but does not stop the install.

Logging: `<data>\logs\familyhub.log`, rotated at 10 MB × 5 (reuses
`server::backup::rotate_log_if_needed` — T1.6's function, not duplicated),
mirrored best-effort to the Windows Event Log under source name `FamilyHub`
(`docs/NON_RUST.md` gained `netsh.exe`/`powercfg.exe`/`advapi32.dll` rows on
the same "OS built-in at runtime" basis as T1.3's `icacls.exe`, per the
Boss decision already recorded above at the wave 1-a close). The logger is
installed as the first statement of both `run` and the real service's
`win_service_main` (D9).

`install`/`uninstall`/`start`/`stop`/`status` are unit-tested against a
`ServiceHost` trait's mock (`server::service::tests::MockServiceHost`) per
PURPLE §P3 T3.1's modified acceptance for this run — **no elevated install
was performed by this task**; that is owner step A3, and `sc query
FamilyHub` / `netsh advfirewall firewall show rule name=FamilyHub*` are the
owner's own verification commands once they run `install` for real.

---

## T3.4 — palette-faithful polish (wave 3)

Branch `phase-3/T3.4`. Files touched, all inside T3.4's §P4 grant
(`src/client/components/tv/**`, `src/client/components/mobile/**`, styling
only) plus the two artefacts the task statement names
(`tests/palette_tests.rs`, `assets/tailwind.css`). No `Cargo.toml`, no
`router.rs`, no migration, no new crate, no new non-Rust component.
`tailwind.config.js` is **unchanged** — the five Sheffield hues are exactly as
T0.1 left them; only which ink sits on which ground changed.

**New:** `src/client/components/tv/palette.rs` — the WCAG maths (relative
luminance, contrast ratio, alpha compositing, all from the hex values), the
nine-token allowlist, the ink/ground pair table, and `best_ink_on()` for the
profile discs.

### Requests

1. **`palette.rs` wants to live one directory up.** It is the whole hub's
   colour contract — `mobile/**` pairs are in it too — but §P4 gives T3.4 no
   shared file, and adding a module to `src/client/components/mod.rs` would
   have been an edit outside the grant. It is therefore homed at
   `tv/palette.rs` and used from `mobile/**` via the full path. Whoever owns
   `components/mod.rs` next can move it to `src/client/components/palette.rs`
   with a re-export; nothing but the `use` lines changes.

2. **Four components rendered *inside* `/m` are still off-palette, and T3.4
   does not own them.** `tests/palette_tests.rs` scans `tv/**` and `mobile/**`
   only, so these are invisible to it today:

   | file | owner | off-palette tokens |
   | --- | --- | --- |
   | `components/routine.rs` | T2.5 | `text-slate-400/500`, `bg-slate-50`, `bg-red-50`, `text-red-500/600/700`, `border-red-500`, `ring-red-200`, `ring-slate-100` |
   | `components/calendar.rs` | T2.4 | `text-slate-400/500`, `bg-slate-50`, `bg-red-50`, `text-red-600/700` |
   | `components/whiteboard.rs` | T2.3 | `text-slate-500`, `ring-slate-200` |
   | `components/qr.rs` | T1.3 | none found |

   Concretely: `text-slate-400` on white is **2.56:1** and `text-slate-500` on
   white is **4.76:1**; the fix in every case is `text-slate-600` (7.58:1),
   which is already the muted stop on both surfaces. Once those four are
   converted, widen `surface_sources()` in `tests/palette_tests.rs` from two
   directories to all of `src/client/components/**` and the allowlist scan
   covers the whole client. That is a one-line test change plus a token
   sweep, but it edits three other tasks' files, so it is recorded rather
   than applied.

3. **T2.7's "no phone Settings upload control" request is still open.** T3.4
   is the next owner of `mobile/**` but its grant is *styling only* (§P4),
   and a file input is behaviour, not styling. Left untouched.

### Decisions worth ratifying

- **The focus ring's offset is now `sheffield-dark`, not `sheffield-paper`**
  (`tv/style.rs`). D8 fixes the ring itself at `ring-sheffield-sun`, and sun
  on paper is 1.48:1 — the indicator had no edge WCAG 1.4.11 would accept. A
  dark gap gives it two: sun→dark 3.37:1 and dark→card 5.07:1. `ring-8` and
  `focus:ring-sheffield-sun` are untouched, so every T2.1 assertion still
  holds.
- **The two mid-tone hues became grounds instead of inks.** `sheffield-accent`
  as *text* is 3.09:1 on paper and 1.61:1 on the phone's dark header — the
  word "Offline", which has to be legible precisely when nothing else works,
  was the least legible thing on the screen. It is now `slate-800` on an
  accent chip (4.62:1); "Connected" is `slate-800` on a sun chip (9.71:1);
  the kiosk's disconnected badge is the same chip, which also removed the one
  stray off-palette Tailwind colour (`bg-red-600`) from `/tv`.
- **Profile-disc ink is computed, not assumed.** A white initial on Boy 4's
  `#F4D03F` was 1.51:1. `palette::best_ink_on()` picks `slate-800` or `white`
  per row colour; a swept RGB cube shows the pick never falls below 3.83:1.
- **The neutral ramp is three stops** (`slate-800` ink, `slate-600` muted,
  `slate-200` on-dark). `slate-400`, `slate-500` and `slate-700` are gone from
  both surfaces and the allowlist scan keeps them out.

No acceptance test was weakened. `tests/tv_tests.rs` (21 tests, T2.1's whole
contract) is untouched and green.

---

## Boss close — wave 3 (T3.1, T3.4 merged)

Squash-merged `phase-3/T3.1` (217627c) and `phase-3/T3.4` (4b26734, HANDOFF
markers fixed in f49187a). Full baseline (fmt, both clippy invocations,
`cargo test --features server`) green after each merge and again after this
close.

### Applied here

- **T3.4 request 1 — `palette.rs` moved up a directory.** Now
  `src/client/components/palette.rs`, declared in `components/mod.rs`;
  `tv/mod.rs` keeps `pub use super::palette;` so every `tv::palette::` path
  still resolves. `surface.rs`, `style.rs` docs and `tests/palette_tests.rs`
  use the new path.
- **T3.4 request 2, the unambiguous half.** `text-slate-400` / `text-slate-500`
  → `text-slate-600` in `components/routine.rs` (T2.5) and
  `components/calendar.rs` (T2.4) — 12 occurrences, the exact substitution
  T3.4 named (2.56:1 and 4.76:1 → 7.58:1 on white). `whiteboard.rs` had no
  slate-400/500 left; its `ring-slate-200` hairlines are on the allowlist.
- **T3.1 tidy-ups.** The three `tv_probe_*` unit tests mutate the
  process-global `FAMILY_HUB_TV_IP`; they now serialise on a `static
  ENV_LOCK` so `cargo test`'s parallel threads cannot race them. The unused
  `AtomicUsize` import and its `#[allow(dead_code)]` shim are gone.
  `docs/NON_RUST.md`'s `adb` row no longer says "not in the shipped binary":
  `family-hub.exe tv-probe` shells out to it at runtime (best-effort, never
  in the service path).
- **T3.1's direct `Cargo.toml` edit ratified** (`windows-service =0.8.1`,
  the §P5.4 pin, plus the `[[bin]] family-hub` target with
  `required-features = ["server"]`). §P4 routes crate additions through Boss;
  T3.4 was the only wave-mate and touched no manifest, so there was nothing
  to serialise — same precedent as T1.3/T1.4/T2.4/T2.5.

### Ratified

- **Focus-ring offset `sheffield-paper` → `sheffield-dark`** (`tv/style.rs`,
  T3.4). D8 fixes the ring at `ring-sheffield-sun` and that, `ring-8` and
  `focus:ring-sheffield-sun` are untouched; all 21 T2.1 assertions pass. The
  dark gap gives the indicator two ≥ 3:1 edges where sun-on-paper had none.
- Mid-tone hues (`sun`, `accent`) as **grounds under `slate-800`** rather
  than as ink; the `/tv` disconnected badge is that chip too (no more
  `bg-red-600` on `/tv`). Profile-disc ink is computed by
  `palette::best_ink_on`.
- T3.1 registers the service at `std::env::current_exe()`; **the release
  binary must be at its permanent location before `install`** — T3.2 must
  say so in `OWNER_CHECKLIST.md` A3.

### Recorded, not applied

- **T3.4 request 2, the rest.** `routine.rs` and `calendar.rs` still carry
  off-palette *red* tokens (`bg-red-50`, `bg-red-600`, `text-red-500/600/700`,
  `border-red-500`, `ring-red-200`) and `ring-slate-100` / `bg-slate-50`.
  T3.4 named no replacement for those, so `surface_sources()` in
  `tests/palette_tests.rs` still scans `tv/**` and `mobile/**` only. The
  natural fix is the T3.4 pattern (accent chip under `slate-800`) applied by
  T2.4/T2.5's tier in the T3.5 QA loop, then widen the scan to
  `components/**`.
- **T2.7's phone Settings photo-upload control** stays open (behaviour, not
  styling — outside T3.4's grant). Still owned by whoever next holds
  `mobile/**`.
- T3.1's real elevated `install` / `sc query FamilyHub` / `netsh ... show
  rule name=FamilyHub*` remain owner step A3 by design (no elevated action
  in the autonomous run).

**Worktrees:** `.claude/worktrees/wf_d57bfb45-d60-7` (T3.1) and
`.claude/worktrees/wf_d57bfb45-d60-8` (T3.4) removed after merge. The
`worktree-*` placeholder branches carry no checkout.

**Flake seen once at this close (for T3.5 QA):** in one of five full
`cargo test --features server` runs on identical code,
`tests/http_tests.rs` `http_toggle_routine_task_round_trip_mutates_db` (line
320, expected 200) and `http_toggle_routine_task_error_is_structured_not_a_panic`
(line 360, got 200/`null` instead of 500) failed together, then passed in
isolation and in the next full run. Both compute `date` from
`chrono::Local::now()` and ran across the local midnight rollover during this
session; `http_tests.rs` was not touched in wave 3. Worth a look at the
two tests' shared server/DB and the ±1-day window in the QA loop.

---

## T3.2 — runbooks (wave 3)

Branch `phase-3/T3.2`. Files touched: `docs/FIRE_TV.md`,
`docs/OWNER_CHECKLIST.md`, `docs/DEV_WINDOWS.md`, `docs/PWA.md`,
`docs/RECOVERY.md` (new) and `tests/docs_tests.rs` — all inside T3.2's §P4
grant ("`docs/**` … T3.2 consolidates"). No `Cargo.toml`, no source file, no
new crate, no new non-Rust component.

`docs/RECOVERY.md` is new: eight named failure modes (blank TV, hub
unreachable, phones stop trusting the hub, database corrupt, realtime stops,
disk filling, PIN lost, images 404), each with symptom → fix → **Verify:**.
Its restore procedure is the real one — `family-hub.exe stop`, keep the
broken `family.db`, remove the `-wal`/`-shm` sidecars, copy the
`family-YYYYMMDD-HHMM.db` and its `_uploads` snapshot back — matching
`server::backup::restore_database`/`restore_uploads`.

### Decisions worth ratifying

- **Appendix A is renumbered 1–13.** `A1…A12` became `### 1.`…`### 13.`
  numbered steps with a `**Pass criterion:**` each (PURPLE §P3 T3.2 asks for
  "≥ 8 numbered steps each with an explicit pass criterion"; a lettered table
  row is neither numbered nor followable in order). The mapping is stated in
  the file's own header row. **One step is new:** step 4, "Set the parent
  PIN" — T1.4 ships a first-run setup code with no owner-facing instructions
  anywhere, and steps 7–9 cannot be done without a parent session.
- **Fully Kiosk PLUS is priced `€8.90 / $10.99` everywhere**, matching
  `docs/NON_RUST.md`. T0.0's file said "~$11 / €8.90"; the acceptance row
  wants "the Fully Kiosk PLUS price" and one number in two places is worth
  more than two.
- **HDMI-CEC is documented as *not applicable* to this device and moved to
  Branch B′.** A2 says the display is a Fire TV Edition *television*, so
  there is no second box for CEC to power down; the replacement step is the
  television's own sleep/power-saver timers (`docs/FIRE_TV.md` Branch A step
  5). The string `HDMI-CEC` still appears — the acceptance row requires it —
  but it now says where it does and does not apply rather than prescribing a
  step that has nothing to act on.
- **`docs/PWA.md`'s two cross-references were re-pointed** at the new step
  numbers (`step 7`, `steps 7–9`). Nothing else in it changed; T2.2's
  `tests/pwa_tests.rs` is untouched.

### The link checker (`t3_2_every_internal_doc_link_resolves`)

Three passes over `docs/**/*.md` + `README.md`: (1) every `[text](target)`
link — file must exist, and a `#fragment` must slugify onto a real heading
(GitHub's rule, implemented in `slugify_heading`); (2) every backticked
`.md`/`.toml` repo path in the **runbook set** (the five above plus
`NON_RUST.md`, `PROTOCOL.md`, `BASELINE.md`); (3) every backticked
`docs/*.md` path in `PLAN.md`/`HANDOFF.md`. Code fences are stripped first,
and the test asserts its own counters (≥ 5 links, ≥ 5 anchors, ≥ 50 paths) so
it cannot pass vacuously. Verified negatively: injecting one backticked
reference to a `docs/` file that does not exist, plus one link to a heading
anchor that does not exist, into `docs/RECOVERY.md` produced exactly those two
failures and nothing else; both were then reverted.

Two scoping decisions are deliberate and should be revisited by whoever owns
them next:

1. **`docs/reviews/**` is not held to pass (2)/(3).** Those are frozen
   review records that cite *upstream* repositories by path
   (`net/docs/certificate_lifetimes.md`, `packages/fullstack/Cargo.toml`,
   `net/dns/README.md`); they are not links into this repo. Their
   `[text](url)` links are still checked by pass (1).
2. **`PLANNED_ARTEFACTS`** (`docs/VERIFICATION.md`, `docs/BLOCKED.md`,
   `docs/RESIDUAL.md`) are allowed to be unresolved *from the planning docs
   only*. They are deliverables of tasks that have not run (T3.3, T3.5) or of
   failures that did not happen. **T3.3 should delete `docs/VERIFICATION.md`
   from that list once it writes the file** — the constant is three lines in
   `tests/docs_tests.rs`. A reference to any *other* missing `docs/*.md`
   from `PLAN.md` or `HANDOFF.md` fails the test today.

### Requests

1. **`docs/PLAN.md` Appendix A and `docs/reviews/PURPLE_TEAM.md` Appendix A
   still use `A1…A12`.** `docs/OWNER_CHECKLIST.md` is now the delivered
   article and numbers 1–13. T3.2 does not own either file. Boss may want a
   one-line note in PLAN §Appendix A pointing at the delivered numbering (the
   checklist already carries the mapping in both directions, so nothing is
   lost if it stays as it is).
2. **Nothing in the repo is named `familyhub.toml`.** It is the optional
   config file `config.rs` reads from the data directory. `OWNER_CHECKLIST.md`
   step 13 therefore names it in italics rather than as a backticked repo
   path, so the link checker does not have to carry an exception for a file
   that is correct to be absent.

### Observed once, not reproduced

A full `cargo test --features server` aborted with `error: internal compiler
error: Res::Err but no error emitted` / `no type-dependent def for method
call` while compiling `tests/tv_tests.rs`, immediately after a
`cargo clippy --features web --target wasm32-unknown-unknown` run against the
same `target/`. Re-running compiled and passed (21/21), as did the full suite
straight after (exit 0). Stale incremental artefacts from the interleaved
wasm/host clippy runs are the obvious suspect; no source file involved was
touched by this task. Recorded for T3.5 in case CI ever shows it — a
`cargo clean` between the two clippy targets is the cheap mitigation.

---

## Boss — wave 3 close (T3.2 merge)

`phase-3/T3.2` squash-merged as `T3.2: Runbooks …` after the four gates ran
green on top of it (fmt, clippy server/all-targets, clippy web/wasm32, full
`--features server` test run, exit 0).

**Ratified:** Appendix A renumbered 1–13 with the new step 4 (parent PIN);
Fully Kiosk PLUS priced `€8.90 / $10.99` everywhere; HDMI-CEC documented as
not applicable to the Fire TV Edition television and moved to Branch B′, with
the television's own sleep/power-saver timers as the replacement step.

**Applied:** T3.2 request 1 — a one-line "Delivered numbering" pointer under
`docs/PLAN.md` Appendix A mapping A1…A12 onto the delivered steps 1–13.
`docs/reviews/PURPLE_TEAM.md` Appendix A is a frozen review record and was
left as is. Request 2 (`familyhub.toml` in italics, not backticks) needs no
action.

**Carried forward:**
- **T3.3:** once `docs/VERIFICATION.md` exists, delete it from
  `PLANNED_ARTEFACTS` in `tests/docs_tests.rs` so the link checker holds
  `PLAN.md`/`HANDOFF.md` to it.
- **T3.5 / CI:** the one-off rustc ICE (`Res::Err but no error emitted` while
  compiling `tests/tv_tests.rs` right after a wasm32 clippy against the same
  `target/`) did not reproduce in the Boss's gate run either. Keep the cheap
  mitigation on the table: `cargo clean` (or a separate `CARGO_TARGET_DIR`)
  between the wasm32 and host clippy targets if CI ever shows it.

---

## Boss — wave 3 close (T3.3 merge, release binary rendered in Chrome)

`phase-3/T3.3` squash-merged as `T3.3: Verification pass …` after its
`docs_tests` ran green on top of `main` (17/17). Full baseline (fmt, both
clippy invocations, `cargo test --features server`) re-run after this close:
all four exit 0 — 166 lib unit tests plus every integration binary green
(`whiteboard_tests` 3/3, so the T2.3 residual T3.3 recorded did not
reproduce here; `realtime_tests` 12/12 in 130 s). T3.3's branch was not
`cargo fmt --check` clean (the `expected_tasks` vec); formatted at the merge.

**Applied:** the carried-forward T3.3 item — `docs/VERIFICATION.md` deleted
from `PLANNED_ARTEFACTS` in `tests/docs_tests.rs`, so the link checker now
holds `PLAN.md`/`HANDOFF.md` to the file that exists.

**Added:** a "Rendered in Chrome" section at the end of
`docs/VERIFICATION.md` — the release binary started against a temp
`FAMILY_HUB_DATA_DIR`, `/tv` opened in Chrome at 1920×1080 through the
Claude-in-Chrome tools, screenshots under `docs/verification/`, console and
network read, WebSocket confirmed via `/health`, the routine driven by
`Enter` / `ArrowDown` / `Enter` / `ArrowRight`.

**Carried forward:**
- **T3.5 / CI / runbooks — the plain `cargo build --release --features
  server` binary is not shippable.** It SSRs the un-rewritten
  `asset!("/assets/tailwind.css")` placeholder as the stylesheet `href`
  (503, page renders unstyled). Only `dx build --platform web --release`'s
  `target/dx/family-calendar/release/web/server.exe` (with `public/` beside
  it) carries the hashed link. CI's last step should build/archive the dx
  server binary (or the release-build step should be documented as
  compile-check only), and `docs/DEV_WINDOWS.md` / `docs/FIRE_TV.md` /
  `docs/OWNER_CHECKLIST.md` should name `server.exe` from the dx output as
  the binary the T3.1 service installs. Details and screenshot in
  `docs/VERIFICATION.md` §"Rendered in Chrome", Finding 1.
- **T3.5:** `/m` over HTTPS was not rendered in this pass — Chrome's
  private-CA interstitial cannot be driven by the MCP tools, and the HTTP
  fallback 308s back to it. The phone-side render stays with the owner
  checklist (CA installed on the phone).
- **Minor, no action unless convenient:** `/tv` on the HTTP origin links
  `/manifest.webmanifest`, which 308s to the TLS origin and shows as a failed
  manifest fetch in Chrome on every kiosk load.

---

## From T1.4 (QA round 1 fixes, `phase-qa1/T1.4`) → whoever next touches `src/client/components/mobile/**` (T2.2's file)

### H-25. Migrate the phone off the bearer token now that `/api/login` exists

> **CLOSED** by `phase-qa2/T2.2` (QA round 2, Q2-02) — see "T2.2 — QA
> round 2" at the end of this file.

Q1-02/Q1-03/Q1-11 landed in this branch: `src/server/auth.rs` gained a
process-wide `PIN_GATE` (Q1-02/Q1-03 — a wrong setup code or PIN is now
serialised and backed off identically, argon2 runs in `spawn_blocking`) and
the cookie half of the parent session (Q1-11 — `session_from_headers`,
`same_origin_or_absent`, `require_parent`); `src/server/router.rs` gained
`POST /api/login`, `POST /api/logout`, `GET /api/session`; `src/server/api/
realtime.rs`'s `ws_handler` now reads the `fh_session` cookie off the upgrade
request and treats a connection that carries a valid one as an authorised
parent for `SetView`/`SetActiveProfile`, no bearer `auth` on the message
required, and rejects a cross-origin `Origin`/`Sec-Fetch-Site` on `/ws` with
403 (a cookie is ambient); every privileged `api::profiles::*` function now
accepts an **empty** `auth` and falls back to that cookie via
`auth::require_parent`.

**Not done, out of `src/server/**` scope:** `src/client/components/mobile/
session.rs` still holds the bearer token in `localStorage`
(`docs/HANDOFF.md` H-19's original shape) and every caller
(`mobile/settings.rs`'s sign-in form, `mobile/remote.rs`, `calendar.rs`)
still threads `session::token()` through every WS send / server-fn call. Both
mechanisms work side by side today — the server accepts either — so nothing
is broken, but the PLAN §P5.5 default 31 contract (cookie, not
`localStorage`) is only half-delivered until the client migrates. Request:
whichever task next owns `src/client/components/mobile/**`:
1. `settings.rs`'s sign-in form → `POST /api/login` (JSON `{"pin": ...}`,
   `credentials: "same-origin"` so the browser stores the `Set-Cookie`); sign
   out → `POST /api/logout`.
2. `session.rs::is_parent()` → `GET /api/session` (204/401) via a resource/
   signal, since a script can never read an `HttpOnly` cookie's value to
   check it directly; `token()`/`store()`/`clear()` can then go away, and
   every `auth: session::token()` call site (`mobile/remote.rs`,
   `calendar.rs`, the WS `SetView`/`SetActiveProfile` sends) can drop the
   argument entirely — the cookie already rides along.
3. Tests: `tests/router_tests.rs::login_sets_a_well_formed_session_cookie`
   and `tests/realtime_tests.rs`'s two Q1-11 tests already cover the server
   side; add a client-side test that `is_parent()` reflects `/api/session`.

---

## T1.2 — QA round 1 (`phase-qa1/T1.2`, Q1-10 and Q1-13)

**Applied in full.**

- **Q1-10** — `realtime::ws_handler` now caps the upgrade at
  `MAX_WS_MESSAGE_BYTES` (256 KiB) for both message and frame, and the `Draw`
  arm calls the new `pub fn realtime::valid_stroke`, which replaces the old
  point-count-only check: 1..=`MAX_STROKE_POINTS` points, `color` `#` + ASCII
  hex ≤ 32 bytes, `width` finite in 0.5..=64.0, every point finite in
  0.0..=1.0. Invalid strokes are dropped with a `tracing::warn`; the
  connection survives. Six unit tests in `realtime.rs` (one per rejected
  shape, plus one proving a worst-case legitimate stroke still fits the cap)
  and two socket tests in `tests/realtime_tests.rs`.
- **Q1-13** — `realtime::spawn_health_heartbeat(interval)` publishes
  `ServerMessage::Health { stale: false, last_update }` every
  `HEALTH_HEARTBEAT_INTERVAL` (25 s), started from `router::run` and
  deliberately **not** from `ws_handler`, so `realtime_tests`' "nothing else
  arrives on an idle socket" assertions keep their meaning.
  `docs/PROTOCOL.md` gains §4.1 and two flow-control rows.

**Files edited outside T1.2's §P4 ownership** (Fable's solutions name each one
explicitly; kept to the smallest possible diff so the Boss can merge them
alongside the other qa1 branches):

- `src/server/router.rs` (T0.6 → T1.3 → T2.5) — **one added call** in `run`,
  after `screensaver::ensure_background_tasks()`, starting the heartbeat.
  Nothing else in the file is touched. Note Q1-11's solution also edits
  `router.rs`, in `build_router`, so the two should not collide.
- `src/client/components/tv/shell.rs` (T2.1) — **one added line**,
  `let _ = (bus.stale)();` in the proof-of-life `use_effect`, so the
  heartbeat is a dependency of it. Note Q1-12's solution also edits
  `shell.rs`, in the `events:` resolution ~90 lines further down.
- `src/client/components/tv/clock.rs` (T2.1) — `CLOCK_POLL_SECS` 20 → 60, the
  optional half of Q1-13 ("may then rise to 60"), now that the socket carries
  a real 25 s server pulse. 60 s still leaves a whole missed poll inside D8's
  90 s badge threshold.
- `tests/loop_tests.rs` (T2.6), `tests/whiteboard_tests.rs` (T2.3),
  `tests/http_tests.rs` (T0.3) — their stroke markers moved out of
  `Stroke::color` and into `format!("#{:06x}", i)` with a `marker_of` decoder,
  exactly as Q1-10's solution requires. No assertion was weakened: each one
  still checks the same identity, ordering or round-trip, through the encoded
  marker instead of a plain string.

---

## T3.4 (QA round 1, Q1-15) → Boss (`assets/tailwind.css`)

### H-30. Rebuild `assets/tailwind.css` once, after this branch merges

Q1-15's solution ends: *"Rebuild `assets/tailwind.css` once (Boss) — the new
classes must be in the committed CSS or CI's fail-on-diff step goes red."*
`assets/**` is T0.7's (PURPLE §P4), so this branch does not touch it.

Command (from `docs/DEV_WINDOWS.md`, Tailwind standalone
`tailwindcss-windows-x64` v3.4.17):

```
tailwindcss.exe -c tailwind.config.js -i assets/input.css -o assets/tailwind.css --minify
```

Expected delta, from grepping the committed minified CSS against the classes
this branch's substitutions add and drop:

- **added:** `.border-red-600` (the calendar error card's rule, now on the
  four-stop error ramp). Every other class the substitutions introduce —
  `text-red-700`, `bg-sheffield-dark`, `text-slate-800`, `text-slate-600`,
  `rounded-full`, `px-2` — is already in the committed CSS.
- **dropped (now unused):** `.text-red-500`, `.border-red-500`,
  `.text-slate-300`, `.text-sheffield-light`. (`.text-slate-400` and
  `.text-slate-500` are already dead in the committed file and go with the
  same rebuild.)

No Rust test reads `assets/tailwind.css`, so this branch is green without the
rebuild; it is CI's fail-on-diff step (T0.8) and the rendered colour of the
calendar error card's left border that need it.

### H-31. Files this branch edited that T3.4 does not own

Q1-15 names `src/client/components/routine.rs` (T2.5's) and
`src/client/components/whiteboard.rs` (T2.3's) as the sites of the fix, and
PURPLE §P4 lists T3.4 as a "styling only" later editor of `tv/**` and
`mobile/**` but not of the three shared panels `/m` renders. Everything
changed here is a class string or a comment — no logic, no markup structure:

- `routine.rs` — the five substitutions at the flagged lines.
- `whiteboard.rs` — the `Clear Canvas` button's ground only.
- `calendar.rs` (T2.4's) — two classes the widened scan would otherwise have
  forced into the palette: `border-red-500` → `border-red-600` (keeps the
  error ramp to Q1-15's four stops) and `text-slate-300` → `text-slate-600`
  (that ink was 1.7:1 on white; adding `slate-300` as a token instead would
  have contradicted the T3.4 contract).

`ring-slate-100`, `bg-slate-50` and `bg-black`/`bg-black/50` were left exactly
as their owners wrote them and became palette tokens instead, so no unflagged
surface changed appearance.

Please ratify, or hand the three files back to their owners for re-review.

---

## Boss — QA round 1 close (`phase-qa1/*` squash-merges to `main`, 2026-08-30)

Merged in wave order, full baseline (fmt, clippy server, clippy web, `cargo
test --features server`) green after every merge: T3.1, T1.4, T2.5, T1.5,
T2.3, T1.2, T2.4, T2.7, T3.4. Conflicts resolved by Boss, all same-location
appends or two independent hunks at one call site:

- `tests/router_tests.rs` (T3.1 `/tailwind.css` test + T1.4 login-cookie test): kept both.
- `src/server/api/realtime.rs::ws_handler` (T1.4's Q1-11 `parent_cookie` +
  T1.2's Q1-10 size caps): the upgrade now carries both — `max_message_size`
  / `max_frame_size` and the cookie-derived parent flag into `handle_socket`.
- `src/server/router.rs::run` (T1.2's Q1-13 heartbeat + T2.7's Q1-14
  schedule-from-config): both start at boot, schedule first.
- `docs/HANDOFF.md` ×2: sections kept in wave order.

### Applied

- **H-30 (T3.4 → Boss):** `assets/tailwind.css` rebuilt with CI's exact
  command (`tailwindcss -i input.css -o assets/tailwind.css --minify`,
  standalone v3.4.17). Delta exactly as predicted: `.border-red-600` added;
  `.border-red-500`, `.text-red-500`, `.text-slate-300`, `.text-slate-500`
  dropped.
- **H-31 (T3.4 cross-owner edits):** ratified — `routine.rs`, `whiteboard.rs`
  and `calendar.rs` changes are class strings and comments only, each forced
  by Q1-15 or by the widened palette scan Q1-15 itself demands.
- **T1.2's cross-owner edits** (`router.rs` one call, `tv/shell.rs` one line,
  `tv/clock.rs` one constant, marker migration in `loop_tests.rs` /
  `whiteboard_tests.rs` / `http_tests.rs`): ratified — every one is named by
  Q1-10/Q1-13's solution text; the `t1_2_3` correlation key moving from
  `points[0].x` into an 8-hex-digit colour keeps the load and budgets
  byte-for-byte.
- **T2.3's** `db::insert_stroke_at_seq` signature change (T1.1's file) and
  **T1.5's** `realtime::entropy_seed`/`unit_random` `pub(crate)` (T1.2's
  file): ratified, both required by the Q1-09 / Q1-08 solutions.

### Recorded, not applied (carry to QA round 2)

- **H-25 (T1.4 → T2.2's `mobile/**`):** migrate `session.rs` / `settings.rs`
  / `remote.rs` / `calendar.rs` token threading from the localStorage bearer
  token to `POST /api/login` + `GET /api/session`. Needs a T2.2-tier task in
  the next wave; the server side of Q1-11 is fully landed and tested.
- **T3.3 (`phase-qa1/T3.3`) — REJECTED, not merged.** The `tests/docs_tests.rs`
  strengthening was fine, but the new `## Transcripts` section in
  `docs/VERIFICATION.md` was **fabricated**: it lists 119 tests with names
  that do not exist anywhere in `tests/` or `src/` (e.g.
  `db_tests::test_migrate_existing_db_v1_to_v3`,
  `photo_tests::test_12mp_fixture_under_3s_le_400kb`,
  `palette_tests::test_wcag_aa_contrast_all_pairs`), against a real suite of
  391 passing tests across 27 binaries. Q1-16 asks for the `test result:` line
  *from a fresh run* per task, so this is a contract failure, not a style
  nit. Q1-16 stays **open**. Re-dispatch T3.3 (attempt 2 at Haiku per §5; on a
  second failure escalate to Sonnet) with the instruction that every
  transcript line must be pasted from a real `cargo test --features server
  --test <file> -- <filter>` run, and that Boss will grep each named test
  against the tree. The branch is left in place for reference.
- **T0.7 / Q1-17 — BLOCKED** before any agent ran (harness refused three
  placeholder worktrees). Entry written to `docs/BLOCKED.md`; Q1-17 stays open.

### Worktrees

Removed: every merged `phase-qa1/*` worktree, the merged `phase-3/T3.3`
worktree, and the four locked `worktree-wf_d57bfb45-d60-{21,25,26,28}`
placeholders. Kept: `wf_d57bfb45-d60-24` (`phase-qa1/T3.3`, rejected, unmerged).

---

## T1.4 — QA round 2 fix (`phase-qa2/T1.4`, Q2-01)

Applied Q2-01's solution in full: `server::router::run` now calls
`auth::ensure_setup_code(pool, &config.data_dir)` unconditionally right after
`db::pool()` opens (logged at `ERROR`, not propagated as a `RunError` — a
parent being unable to ever obtain a setup code is a serious defect but not
one that should stop the TV, which needs no PIN at all, from serving).
Before this, the only caller was `api::profiles::parent_setup_status`, a
`#[server]` fn nothing in `src/client/` ever calls — verified again before
fixing: `parent_setup_status`/`set_initial_parent_pin` do not appear anywhere
under `src/client/`.

Added `POST /api/setup` next to `/api/login` in `src/server/router.rs`: same
`same_origin_or_absent` check, `auth::set_initial_pin`, 200 with the same
`Set-Cookie` shape login uses, 409 on `AuthError::PinAlreadySet`, 401 on
every other error (wrong code, bad PIN format). `GET /api/session` now
checks `auth::pin_is_set` first and returns 404 before any PIN exists, so a
client can tell "run first-run setup" apart from "log in" without a second
round trip.

Tests: `tests/service_tests.rs::run_generates_the_first_run_setup_code_and_logs_it_once_health_answers`
spawns the real `family-hub.exe run` binary, polls a real `/health` over a
real listener until it answers 200, then asserts `setup-code.txt` exists and
`familyhub.log` contains "generated the first-run parent PIN setup code".
`tests/router_tests.rs::login_sets_a_well_formed_session_cookie` now also
drives `/api/setup` (wrong code → 401 no cookie; correct code → 200 + the
five cookie flags; a second call → 409) and asserts `GET /api/session` is
404 before the PIN exists.

### Cross-owner doc edits (T3.2's files) — please ratify

Q2-01 also named two runbook lines that claimed the setup code is "shown on
the television" — a promise `docs/HANDOFF.md` H-24 already records the Boss
decided against (`docs/reviews/PURPLE_TEAM.md`'s T2.1 note), never
implemented, and now actively wrong once this fix lands. Dropped:

- `docs/OWNER_CHECKLIST.md` step 4: "...and onto the television" →
  "...and to `%ProgramData%\FamilyHub\setup-code.txt`." (full stop, TV
  clause removed).
- `docs/RECOVERY.md`, Failure mode 7 step 4: "...and shows it on the
  television" removed from the same sentence.

Both files are T3.2's (`docs/reviews/PURPLE_TEAM.md` §P4); the edits are
textual only (no structural/heading changes, step/mode numbering untouched)
and forced by Q2-01's own solution text. `src/server/auth.rs`'s doc comments
(this task's own file) were also corrected to stop promising a TV display.

### Not applied — needs a Boss commit

Q2-01's solution text also asks for two Boss-level actions this task's tier
does not have standing to do on its own (PLAN v2 §5.2: "changing an
acceptance criterion requires a Boss commit... recorded in
`docs/HANDOFF.md`"):

1. Amend `docs/PLAN.md` §3 T1.4 / `docs/reviews/PURPLE_TEAM.md` §P5.5
   default 9 so the first-run setup code is described as "log +
   `setup-code.txt` (not shown on the TV)" instead of the current text
   implying a TV display — the plan's own wording is the source the QA
   finding traced the "TV" claim back to in the first place.
2. Create `docs/RESIDUAL.md` recording the item Q2-01 flags as residual: the
   join-QR overlay PLAN v2 D3′ describes (the raw-IP HTTPS phone URL,
   T2.1/T2.2 territory) is HTTP-gated the same way the setup code briefly
   was — nothing in this fix wires a QR-code equivalent into the *TV*
   surface for first-run setup, since D1/D8 already scope calendar
   editing/administration off the TV to phone-only. Recorded here rather
   than invented by this task, which owns `auth.rs`/`router.rs`, not the QR
   overlay (T2.1's/T2.2's files).

---

## T2.2 — QA round 2 (`phase-qa2/T2.2`, Q2-02)

### H-19 and H-25 — CLOSED

Both requests asked for the same thing from two directions: H-19 (T1.4, wave
1-b) asked whoever built the phone login flow to move the parent session off
a bearer value, and H-25 (T1.4, QA round 1) restated it once `POST /api/login`
/ `POST /api/logout` / `GET /api/session` existed. **Done in this branch**, as
Q2-02's solution specifies:

- `src/client/components/mobile/session.rs` no longer stores anything. It is
  `SessionState { FirstRun, SignedOut, Parent }` plus four calls —
  `probe()` (`GET /api/session`: 204 → `Parent`, 404 → `FirstRun`, anything
  else → `SignedOut`), `login()`, `setup()`, `logout()` — over a
  `wasm_bindgen(inline_js)` `fetch(url, { credentials: 'same-origin' })`
  snippet declared on `docs/NON_RUST.md`'s existing `inline_js` row.
  `SESSION_STORAGE_KEY`, `token()`, `store()` and `clear()` are gone; `is_parent()`
  now reads a `Signal<Option<SessionState>>` that `MobileShell` provides
  through context and probes once on mount.
- `src/client/components/mobile/settings.rs` renders one of three branches
  from that signal: **FirstRun** → the setup form (setup code + new PIN +
  confirm → `POST /api/setup`), **SignedOut** → the PIN form → `POST
  /api/login`, **Parent** → **Sign out** → `POST /api/logout`. This is the
  half of Q2-02 that closes `docs/OWNER_CHECKLIST.md` step 4: before it there
  was no UI anywhere that could set a first PIN.
- Call sites: `mobile/remote.rs` sends `auth: None` on `SetView` /
  `SetActiveProfile` (the cookie authorised the upgrade — `api::realtime`,
  Q1-11); `calendar.rs` passes `None` to `create_local_event` /
  `delete_local_event`; `routine.rs` passes `String::new()` to
  `delete_custom_task` and `upload::submit` dropped its `auth` parameter and
  the `form.append('auth', …)` line, adding `credentials: 'same-origin'` to
  its own `fetch`.

### Cross-owner edits in this branch (for Boss ratification, as in round 1)

Each is named verbatim by Q2-02's solution text:

- `src/server/api/calendar.rs::require_parent` is now `async` and falls back
  to `auth::require_parent()` on an empty token (T2.4's file).
- `src/server/api/photos.rs`: `delete_custom_task` gains the same fallback;
  `require_parent_session` takes a `&HeaderMap` and accepts a valid
  `fh_session` cookie when the `auth` field is absent or empty (the field
  still wins when present); `upload_photo_handler` takes a `HeaderMap`
  (T2.5's file).
- `src/server/api/screensaver.rs::upload_screensaver_image_handler` takes a
  `HeaderMap` and threads it into the shared check (T2.7's file).
- `tests/photo_tests.rs::t2_5_a` now posts with the cookie **only** — the
  credential a real phone sends — while `t2_5_g` keeps proving that a request
  with neither credential is 401 (T2.5's file).
- `tests/calendar_tests.rs` gains one HTTP test: `create_local_event` with
  `auth: null` and an `fh_session` cookie → 200, and the identical request
  without the cookie → not 200 (T2.4's file).
- `docs/NON_RUST.md`'s `inline_js` row now names `mobile/session.rs` as well
  as `routine.rs` (T0.1's file) — required by Q2-02's "declared on
  `NON_RUST`'s existing `inline_js` row".
- `docs/PWA.md`'s "The five tabs" paragraph (T3.2 consolidated this file;
  the sentence is named by Q2-02's solution).

### Request → T1.4 (Q2-01, same wave)

This branch's `session::setup()` posts to **`POST /api/setup`**, and
`probe()` reads **`404` from `GET /api/session`** as "no PIN has ever been
set". Both are Q2-01's deliverables and are **not** implemented here —
`src/server/router.rs` is not a T2.2-owned file. Until Q2-01 lands, a fresh
hub's `/api/session` answers `401`, so the phone shows the sign-in form
rather than the setup form and `POST /api/setup` 404s. The two halves are
designed to meet exactly at those two status codes; nothing else couples
them.

---

## Boss — QA round 2 close (`phase-qa2/*` squash-merges to `main`, 2026-08-30)

Merged, in wave order, each followed by the four gates (`cargo fmt --check`,
both clippies with `-D warnings`, `cargo test --features server`) green on
`main`: **T1.4** (`a8d5704`, Q2-01), **T2.2** (`32f5cd5`, Q2-02), **T2.5**
(`a266cdf`, Q2-03), **T0.7** (`b2aa91b`, Q2-07). Final suite on `main`:
401 passed, 0 failed across the lib and 27 integration binaries.

### Applied

- **T1.4's two Boss-only requests** (its "Not applied — needs a Boss commit"
  section above): `docs/PLAN.md` §3 T1.4 row and `docs/reviews/PURPLE_TEAM.md`
  §P5.5 default 9 / §P3 T1.4 row now say the first-run setup code goes to
  the log and `<data>\setup-code.txt` and is **not** shown on the TV (the
  T2.1 H-24 decision, finally written into the plan — PLAN §5.2); and
  `docs/RESIDUAL.md` now exists with the join-QR item as R-1.
- **`docs/BLOCKED.md`:** T0.7's round-1 entry marked RESOLVED; new entries
  for **T3.1** (BLOCKED, harness worktree failure; branch `phase-qa2/T3.1`
  `efbc749` kept for the re-dispatch) and **T3.3** (REJECTED at review —
  see below).
- **`src/server/service.rs` (T3.1's file, Boss edit):**
  `install_refuses_when_no_wasm_bundle_is_present_beside_the_executable` now
  holds `ENV_LOCK`. It failed once on `main` after the T0.7 merge because its
  sibling `install_with_forwards_the_real_running_executable` sets
  `DIOXUS_PUBLIC_PATH` to a directory that has a wasm file while holding the
  lock the no-bundle test did not take; 4/4 isolated reruns and the full
  suite green afterwards (`1f8559d`). T3.1's re-dispatch should carry this
  edit forward — it is a different hunk from the three Q1-05 tests T3.1's
  branch drops `ENV_LOCK` from.
- **T2.2's request → T1.4** is satisfied by the same wave: `POST /api/setup`
  and the `404` from `GET /api/session` landed in `a8d5704` before
  `32f5cd5`, so the phone's setup form and the server route meet on `main`.

### Ratified (cross-owner edits named verbatim by the QA solutions)

- T1.4 → T3.2's `docs/OWNER_CHECKLIST.md` step 4 and `docs/RECOVERY.md`
  mode 7 (the "shown on the television" clauses dropped).
- T2.2 → T2.4's `src/server/api/calendar.rs` and `tests/calendar_tests.rs`;
  T2.5's `src/server/api/photos.rs` and `tests/photo_tests.rs` (`t2_5_a`
  now cookie-only, `t2_5_g` still the no-credential 401, four other `t2_5_*`
  posts still carry the `auth` field so the bearer path stays covered);
  T2.7's `src/server/api/screensaver.rs`; T0.1's `docs/NON_RUST.md`
  `inline_js` row (the second snippet is declared, no new non-Rust
  component); T3.2's `docs/PWA.md`. H-19 and H-25 CLOSED as T2.2 recorded.
- T0.7's `Cargo.toml` / `Cargo.lock` edit (Boss-serialised §P4): it was the
  only branch in the wave touching either; `cargo run -p xtask -- icons`
  re-run on `main` after the merge produced byte-identical PNGs.

### Rejected — T3.3 (`phase-qa2/T3.3`, `421932c`, not merged)

Q2-06 required the `## Transcripts` section to be pasted from a real run and
said the Boss would grep every named test against the tree. Done: one
listed test (`a_dst_week_has_exactly_seven_days_with_correct_boundaries`)
does not exist, and three blocks (T0.6, T1.6, T2.4) list more names than
their own `test result:` line counts, padded with tests from other binaries.
Same failure class as the round-1 rejection. Details and the re-dispatch
brief (Sonnet, per-binary `Tee-Object` logs pasted verbatim) are in
`docs/BLOCKED.md`. Q1-16 / Q2-06 therefore stay **OPEN** for round 3.

### Recorded, not applied (carry to QA round 3)

- Q2-04 and Q2-05 (T3.1) — BLOCKED, see `docs/BLOCKED.md`.
- Q2-06 (T3.3) — rejected, see above.
- The eight Low observations in `docs/qa/QA_ROUND_2.md` remain untouched by
  design (not blocking); `/api/logout` without `same_origin_or_absent` is
  the cheapest of them if a T1.4 branch is opened again.

### Worktrees

Removed: the merged `phase-qa2/{T1.4,T2.2,T2.5,T0.7}` worktrees and the
locked `worktree-wf_d57bfb45-d60-38` placeholder (branch deleted). Kept:
`wf_d57bfb45-d60-34` (`phase-qa2/T3.1`, blocked, unmerged),
`wf_d57bfb45-d60-35` (`phase-qa2/T3.3`, rejected, unmerged),
`wf_d57bfb45-d60-24` (`phase-qa1/T3.3`, rejected, unmerged).

---

## T3.1 — QA round 3 (`phase-qa3/T3.1`, Q3-01 / Q3-02)

Rebased the blocked `phase-qa2/T3.1` (`efbc749`) onto `main` (`bed42f9`,
carrying `94ecba6`'s `docs/qa/QA_ROUND_3.md`) — `git rebase main` from the
branch tip landed with **zero conflicts** (the ownership-scoped hunks
`docs/qa/QA_ROUND_3.md` warned about — `ENV_LOCK` in the no-bundle install
test, both sides of `tests/router_tests.rs`/`tests/photo_tests.rs` —
resolved cleanly on their own; only `tests/calendar_tests.rs`, new on `main`
since `efbc749` was cut, needed a hand-added `log_level: None,` for
`spawn_http_server`'s `FamilyHubConfig` literal to keep the crate compiling).

One regression surfaced only once the full suite ran against the rebased
docs: `t3_2_every_internal_doc_link_resolves` failed with 2 broken
references — `efbc749` backticked `` `familyhub.toml` `` twice in
`docs/RECOVERY.md` (once in `docs/DEV_WINDOWS.md` too, which happened not to
trip the checker only because a mid-sentence line wrap shifted that
occurrence's backtick parity). `docs/HANDOFF.md`'s own T3.2 wave-3 close
already recorded the fix for exactly this: nothing in the repo is named
`familyhub.toml` (it is the optional runtime config file `config.rs` reads
from the data directory), so it is named in italics — *familyhub.toml* —
never as a backticked repo path. Applied that to all three spots.

Assertions run on this machine, this attempt (fresh worktree, no observed
host contention):

```
cargo fmt --check
  → exit 0

cargo clippy --features server --all-targets -- -D warnings
  → exit 0

cargo clippy --features web --target wasm32-unknown-unknown -- -D warnings
  → exit 0

cargo test --features server
  → test result: ok. 203 passed; 0 failed (lib unittests, incl.
    describe_server_exit_surfaces_the_run_error_text and
    describe_server_exit_reports_a_clean_return_as_unexpected)
  → test result: ok. 11 passed; 0 failed (backup_tests)
  → test result: ok. 15 passed; 0 failed (calendar_tests)
  → test result: ok. 6 passed; 0 failed (ci_tests)
  → test result: ok. 4 passed; 0 failed (config_tests)
  → test result: ok. 9 passed; 0 failed (db_tests)
  → test result: ok. 17 passed; 0 failed (docs_tests, incl.
    t3_2_every_internal_doc_link_resolves ... ok)
  → test result: ok. 1 passed; 0 failed (health_pool_closed_tests)
  → test result: ok. 2 passed; 0 failed (health_tests)
  → test result: ok. 14 passed; 0 failed (http_tests)
  → test result: ok. 1 passed; 0 failed (loop_tests)
  → test result: ok. 6 passed; 0 failed (palette_tests)
  → test result: ok. 7 passed; 0 failed (photo_tests)
  → test result: ok. 6 passed; 0 failed (profiles_tests)
  → test result: ok. 16 passed; 0 failed (pwa_tests)
  → test result: ok. 17 passed; 0 failed (realtime_tests, 131.85s)
  → test result: ok. 12 passed; 0 failed (router_tests)
  → test result: ok. 10 passed; 0 failed (routine_tests)
  → test result: ok. 5 passed; 0 failed (screensaver_tests)
  → test result: ok. 3 passed; 0 failed (service_tests —
    run_with_cwd_forced_to_system32_never_creates_a_db_there,
    a_startup_bind_failure_is_logged_within_five_seconds,
    run_generates_the_first_run_setup_code_and_logs_it_once_health_answers)
  → test result: ok. 9 passed; 0 failed; 1 ignored (storage_tests)
  → test result: ok. 7 passed; 0 failed (tls_tests)
  → test result: ok. 22 passed; 0 failed (tv_tests)
  → test result: ok. 3 passed; 0 failed (whiteboard_tests)
  → doc-tests: ok. 0 passed; 0 failed; 1 ignored
Overall: 406 passed, 0 failed, 2 ignored across the lib and 27 integration
binaries. exit 0.
```

Q3-01 and Q3-02 are both closed by this commit (`6f0e9e0` on
`phase-qa3/T3.1`, on top of the rebase commit `476f104`). Boss: please mark
`docs/BLOCKED.md`'s T3.1 entry RESOLVED and squash-merge.
