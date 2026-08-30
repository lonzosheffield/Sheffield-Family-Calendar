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
