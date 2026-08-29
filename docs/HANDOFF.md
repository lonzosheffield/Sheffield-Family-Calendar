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
