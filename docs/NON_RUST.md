# Non-Rust Components

The Sheffield Family Calendar & Routine Hub stack is **Rust whole-stack**, with the following declared exceptions:

| Component | Purpose | Justification | Notes |
| --- | --- | --- | --- |
| `sw.js` | Service Worker (~40 lines) | Served from Rust; offline app shell and network request routing | Included via `include_str!` in Rust server |
| `tailwindcss-windows-x64` v3.4.17 | CSS build-time compiler (binary) | Standalone Tailwind CLI for Windows; v4 is incompatible with our config model and has no Rust equivalent | Downloaded as a binary; runs at build time only |
| Fully Kiosk Browser ≥ 1.61.2 | Kiosk shell on Fire TV (Branch A) | Proprietary launcher; PLUS licence ~€8.90/$10.99 one-off | Fire OS only; one of three runbook branches (A/B/B′); sideloaded via adb |
| `adb` (Android Debug Bridge) | Fire TV configuration tool | Granting SYSTEM_ALERT_WINDOW and GET_USAGE_STATS permissions; setting sleep timeout; toggling HDMI-CEC | One-time device setup; not in the shipped binary |
| GitHub Actions YAML (`.github/workflows/`) | CI/CD pipeline definition | Declarative workflow for build, test, lint, release automation | Not code executed by the app; configuration only |
| `wasm-bindgen` glue | WebAssembly FFI bindings | Browser APIs (canvas, events, storage); generated from `web-sys` types at compile time | Included in the Dioxus build process; transparent to Rust code |
| Browser / WebView | Runtime execution | Fire TV: Amazon Silk (Vega OS) or Fully Kiosk; phones: Chrome (Android) or Safari (iOS) | Third-party; outside our control; tested against via the app's HTTP interface |
| Amazon Silk (Vega OS Branch B) | Kiosk shell on Fire TV (alternative) | Native browser on newer Fire TV models (Vega OS 2024+); no sideload required | Optional branch; fallback if Fully Kiosk unavailable |
| Android SDK (Phase C; **not in this run**) | Native Android app shell (cut) | WebView host for a Rust-native Android wrapper; full D-pad integration | **Deferred indefinitely**; kept in docs for completeness |
| `icacls.exe` (Windows built-in) | Narrow the NTFS ACL on `<data>\pki\*.key` when a key is first written (§P5.5 default 7, T1.3) | OS built-in invoked at runtime; best-effort, failure is logged and never fatal. T3.1 adds `netsh.exe` / `sc.exe` rows on the same basis | No download; present on every Windows 11 install |
