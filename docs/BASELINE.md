# Baseline — untouched code at commit 5769946, Dioxus 0.6.3

Captured 2026-08-29 on Windows 11 (rustc 1.98.0, cargo 1.98.0, MSVC 14.29, Windows SDK 10.0.19041).

| Check | Result | Wall time (cold cache) |
| --- | --- | --- |
| `cargo test --features server` | **8 passed, 0 failed** (`tests/db_tests.rs`) | 6m 57s |
| `cargo clippy --features server --all-targets` | clean, 0 warnings | 3m 21s |
| `cargo clippy --features web --target wasm32-unknown-unknown` | clean, 0 warnings | 4m 08s |

Toolchain installed for this project: rustup stable + `wasm32-unknown-unknown`; `dx` 0.7.10 (`~/.cargo/bin`) and `dx` 0.6.3 (`~/.cargo/dx06`); Tailwind CSS 3.4.17 (npm global); `adb` 1.0.41 (scoop); `cargo-binstall`.

Not yet exercised: `dx serve --platform web` (deliberately — the owner asked for the plan and reviews before the app is run).
