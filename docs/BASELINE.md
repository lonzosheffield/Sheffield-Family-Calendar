# Baseline — untouched code at commit 5769946, Dioxus 0.6.3

Captured 2026-08-29 on Windows 11 (rustc 1.98.0, cargo 1.98.0, MSVC 14.29, Windows SDK 10.0.19041).

| Check | Result | Wall time (cold cache) |
| --- | --- | --- |
| `cargo test --features server` | **8 passed, 0 failed** (`tests/db_tests.rs`) | 6m 57s |
| `cargo clippy --features server --all-targets` | clean, 0 warnings | 3m 21s |
| `cargo clippy --features web --target wasm32-unknown-unknown` | clean, 0 warnings | 4m 08s |

Toolchain installed for this project: rustup stable + `wasm32-unknown-unknown`; `dx` 0.7.10 (`~/.cargo/bin`) and `dx` 0.6.3 (`~/.cargo/dx06`); Tailwind CSS 3.4.17 (npm global); `adb` 1.0.41 (scoop); `cargo-binstall`.

Not yet exercised: `dx serve --platform web` (deliberately — the owner asked for the plan and reviews before the app is run).

## Post-Wave 0-c — commit eb3e298, Dioxus 0.7.10

Captured 2026-08-29 on the same machine/toolchain after squash-merging `phase-0/T0.4` (9cb2a91) to `main`. Warm-cache wall times (dependencies already built by the pre-merge runs; the untouched-code numbers above remain the cold-cache reference).

| Check | Result | Wall time (warm cache) |
| --- | --- | --- |
| `cargo fmt --check` | clean | <1s |
| `cargo clippy --features server --all-targets -- -D warnings` | clean, 0 warnings | 1m 04s |
| `cargo clippy --features web --target wasm32-unknown-unknown -- -D warnings` | clean, 0 warnings | 45s |
| `cargo test --features server` | **27 passed, 0 failed** — `db_tests.rs` 8, `docs_tests.rs` 6, `http_tests.rs` 13 | 2m 32s build + 0.3s run |

Test binaries: `db_tests` 0.04s, `docs_tests` 0.00s, `http_tests` 0.24s. Lib/bin unit-test targets contain 0 tests.
