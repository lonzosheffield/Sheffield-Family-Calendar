# Development Setup — Windows 11

## Prerequisites & Setup

### Step 1: Update `$env:PATH` (Required for every shell session)

In **PowerShell** (admin recommended but not required):

```powershell
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:USERPROFILE\scoop\shims;$env:APPDATA\npm;$env:PATH"
$env:RUST_BACKTRACE = "1"
$env:FAMILY_HUB_DATA_DIR = "$env:TEMP\familyhub-test"
```

This prefix ensures:
- **`$USERPROFILE\.cargo\bin`** — Rust toolchain (cargo, rustc, dx)
- **`$USERPROFILE\scoop\shims`** — Scoop package manager (if installed)
- **`$APPDATA\npm`** — npm global binaries (if Node.js installed)

Or in **Bash** (MSYS2 / Git Bash):

```bash
export PATH="$HOME/.cargo/bin:$HOME/scoop/shims:$APPDATA/npm:$PATH"
export RUST_BACKTRACE="1"
export FAMILY_HUB_DATA_DIR="$TEMP/familyhub-test"
```

### Step 2: Install Rust (if needed)

```powershell
rustup toolchain install stable
rustup target add wasm32-unknown-unknown
```

### Step 3: Install Dioxus CLI

Pin the exact version **0.7.10**:

```powershell
cargo binstall dioxus-cli@0.7.10 -y
```

Or build from source (slower):

```powershell
cargo install --locked dioxus-cli@0.7.10
```

### Step 4: Install Tailwind Standalone (v3.4.17)

Download the **Windows x64 binary** (no Node.js required):

```powershell
# Create a directory for Tailwind
mkdir -p "$env:USERPROFILE\.cargo\bin\tailwind" -ErrorAction SilentlyContinue

# Download the v3.4.17 binary
$url = "https://github.com/tailwindlabs/tailwindcss/releases/download/v3.4.17/tailwindcss-windows-x64.exe"
$dest = "$env:USERPROFILE\.cargo\bin\tailwindcss.exe"
Invoke-WebRequest -Uri $url -OutFile $dest

# Verify it works
& $dest --version
```

**Why this version?** Tailwind v4 changes the config model entirely. There is no pure-Rust equivalent; v3.4.17 is the last stable v3 release. The binary is build-time only and does not appear in the shipped application.

### Step 5: Build the Project

```powershell
# Full server build + web target
cargo build --features server --target wasm32-unknown-unknown --release

# Or use Dioxus CLI (one command)
dx build --platform web --release
```

The build will:
1. Compile Rust server code (src/main.rs, src/lib.rs, migrations/)
2. Generate WASM and glue code (web-sys, wasm-bindgen)
3. Run Tailwind CLI to compile `input.css` → `output.css`
4. Bundle assets

### Step 6: Run Locally

```powershell
# Development (dev build, faster iteration)
dx serve

# Or production (optimized)
cargo run --features server --release
```

Then open:
- **TV UI:** `http://127.0.0.1:8080/tv`
- **Phone PWA:** `https://127.0.0.1:8443/m` (requires CA installation for local cert)

---

## Troubleshooting

### `cargo` not found
Ensure Step 1's `$env:PATH` is set in **this shell session** (it does not persist between PowerShell windows). Add it to your PowerShell `$PROFILE` if you want it permanent:

```powershell
# Open/create your profile
notepad $PROFILE

# Add these lines and save
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:USERPROFILE\scoop\shims;$env:APPDATA\npm;$env:PATH"
$env:RUST_BACKTRACE = "1"
$env:FAMILY_HUB_DATA_DIR = "$env:TEMP\familyhub-test"
```

### `tailwindcss.exe` not found during build
Run Step 4 to download the binary. Ensure `$env:USERPROFILE\.cargo\bin` is in your `$env:PATH`.

### Wasm32 target not installed
Run Step 2's `rustup target add wasm32-unknown-unknown`.

### Slow builds
The first build will compile all dependencies. Subsequent builds are faster. On this machine (14 cores), expect 4–7 minutes for a clean build.

---

## Testing

Run all tests (requires `--features server`):

```powershell
cargo test --features server
```

Specific test:

```powershell
cargo test --features server test_name
```

---

## Formatting & Linting

```powershell
# Check formatting (no changes)
cargo fmt --check

# Fix formatting
cargo fmt

# Lint (server target, all tests)
cargo clippy --features server --all-targets -- -D warnings

# Lint (web target)
cargo clippy --features web --target wasm32-unknown-unknown -- -D warnings
```

All CI checks must pass before committing.
