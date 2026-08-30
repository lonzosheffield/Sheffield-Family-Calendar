//! T3.1 acceptance (`docs/reviews/PURPLE_TEAM.md` §P3 T3.1(b)): running the
//! real `family-hub.exe` binary — the Windows service host, PLAN v2 D9 — with
//! its current working directory forced to `C:\Windows\System32` must never
//! create `family.db` there. `FAMILY_HUB_DATA_DIR` overrides the data
//! directory for this test (never the real one, per the mandatory shell
//! preamble), but nothing about the binary itself may special-case a test
//! environment: this proves the same absolute-path discipline
//! `server::config::FamilyHubConfig` already gives every other server-side
//! path (T0.5, G23/R-14) actually reaches the service entry point added by
//! this task, `family-hub.exe run`.
//!
//! `CARGO_BIN_EXE_family-hub` is set automatically by Cargo for integration
//! tests in this package once the `[[bin]] name = "family-hub"` target
//! exists in `Cargo.toml` (T3.1) — no manual `cargo build` step needed.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

fn family_hub_exe() -> &'static str {
    env!("CARGO_BIN_EXE_family-hub")
}

fn scratch_data_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "familyhub-service-cwd-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch data dir");
    dir
}

/// PURPLE §P3 T3.1(b): "run the service binary with CWD forced to
/// `C:\Windows\System32`; assert `family.db` is created under
/// `%ProgramData%\FamilyHub` [here: the overridden `FAMILY_HUB_DATA_DIR`]
/// and `C:\Windows\System32\family.db` does not exist."
#[test]
fn run_with_cwd_forced_to_system32_never_creates_a_db_there() {
    let data_dir = scratch_data_dir("db");
    let system32 = Path::new(r"C:\Windows\System32");
    assert!(
        system32.is_dir(),
        "this acceptance test requires a real Windows System32 directory"
    );

    let mut child = Command::new(family_hub_exe())
        .arg("run")
        .current_dir(system32)
        .env("FAMILY_HUB_DATA_DIR", &data_dir)
        // Ephemeral ports: this test only cares that the binary starts and
        // opens its database, never about reaching a fixed port, and a
        // fixed port could collide with another instance on this box.
        .env("FAMILY_HUB_ADDR", "127.0.0.1:0")
        .env("FAMILY_HUB_TLS_ADDR", "127.0.0.1:0")
        .spawn()
        .expect("failed to spawn family-hub.exe run");

    let db_path = data_dir.join("family.db");
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline && !db_path.exists() {
        std::thread::sleep(Duration::from_millis(200));
    }

    let _ = child.kill();
    let _ = child.wait();

    assert!(
        db_path.exists(),
        "expected {} to have been created by `family-hub.exe run`",
        db_path.display()
    );
    assert!(
        !system32.join("family.db").exists(),
        "family.db must never be created relative to the process's CWD (C:\\Windows\\System32)"
    );
    assert!(
        !system32.join("family.db-wal").exists() && !system32.join("family.db-shm").exists(),
        "no SQLite sidecar files may land in the service's CWD either"
    );

    let _ = std::fs::remove_dir_all(&data_dir);
}
