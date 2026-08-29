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
