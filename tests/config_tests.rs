//! T0.5 acceptance tests (PLAN v2 §3 / `docs/reviews/PURPLE_TEAM.md` §P3):
//! `FamilyHubConfig` resolves every data path **absolutely** from
//! `FAMILY_HUB_DATA_DIR`, never relative to the process's current working
//! directory, and `fullstack_address_or_localhost()` is gone from the
//! release path.

#![cfg(feature = "server")]

use std::path::Path;

use family_calendar::server::config::FamilyHubConfig;
use family_calendar::server::db;

/// HS9 (`docs/BACKLOG.md` B-3): pin this binary's data directory to a
/// pid-keyed scratch directory before any test here calls
/// `FamilyHubConfig::load()`. Without it, a `load()` in a shell that never
/// exported `FAMILY_HUB_DATA_DIR` resolves to the family's live
/// `%ProgramData%\FamilyHub` — the accident B-3 records.
fn init_test_env() -> std::path::PathBuf {
    static ONCE: std::sync::Once = std::sync::Once::new();
    let base = std::env::temp_dir().join(format!("familyhub-config-tests-{}", std::process::id()));
    ONCE.call_once(|| {
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("test scratch directory is creatable");
        std::env::set_var("FAMILY_HUB_DATA_DIR", &base);
    });
    base
}

/// Sets `FAMILY_HUB_DATA_DIR` to a fresh temp directory, boots the real
/// database bootstrap path (`db::pool()`, exactly what `main.rs` calls), and
/// asserts `family.db` was created **inside that directory and nowhere
/// else** — specifically not at `family.db` relative to the test's current
/// working directory (the crate root under `cargo test`), which is exactly
/// where G23/R-14 said it used to land.
#[tokio::test]
async fn boots_with_data_dir_and_writes_family_db_there_and_nowhere_else() {
    // This test relies on `DATABASE_URL` being unset so `db::pool()` falls
    // back to the `FamilyHubConfig`-derived path. `http_tests.rs` sets
    // `DATABASE_URL` explicitly in its own (separate) test binary process,
    // so there is no cross-binary interference here.
    assert!(
        std::env::var("DATABASE_URL").is_err(),
        "this test asserts the FamilyHubConfig fallback path, so DATABASE_URL \
         must not be set in this test binary's process"
    );

    let data_dir = std::env::temp_dir().join(format!(
        "familyhub-config-acceptance-test-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&data_dir);
    std::env::set_var("FAMILY_HUB_DATA_DIR", &data_dir);

    db::pool().await.expect("the database bootstraps");

    let expected_db_path = data_dir.join("family.db");
    assert!(
        expected_db_path.is_file(),
        "expected family.db to exist inside FAMILY_HUB_DATA_DIR at {}",
        expected_db_path.display()
    );

    // The historical bug (G23): a bare `sqlite://family.db` URL resolves
    // relative to the process's CWD, which under `cargo test` is the crate
    // root — assert it did NOT land there.
    assert!(
        !Path::new("family.db").exists(),
        "family.db must not be created relative to the current working directory"
    );

    let _ = std::fs::remove_dir_all(&data_dir);
}

/// `config.http_addr == 0.0.0.0:8080` by default (no env, no
/// `familyhub.toml` present for this process).
#[test]
fn default_bind_address_is_zero_zero_zero_zero_colon_eight_zero_eight_zero() {
    // HS9: `load()` below resolves the data dir too, so pin it at a scratch
    // directory first (B-3) — this test is about the bind address, and must
    // not be the thing that opens the family's live data directory.
    init_test_env();

    // `FamilyHubConfig::load()` reads the real environment; guard against a
    // stray FAMILY_HUB_ADDR leaking in from the caller's shell so this
    // assertion is meaningful rather than accidentally true.
    let had_addr = std::env::var("FAMILY_HUB_ADDR").ok();
    std::env::remove_var("FAMILY_HUB_ADDR");

    let config = FamilyHubConfig::load();
    assert_eq!(config.http_addr, "0.0.0.0:8080".parse().unwrap());

    if let Some(value) = had_addr {
        std::env::set_var("FAMILY_HUB_ADDR", value);
    }
}

/// Gate: no relative-to-CWD path literal remains in `src/` (T0.5's own
/// removal of G23/R-14), run in-process instead of shelling out to `grep` so
/// it also runs on a machine without a `grep` on PATH.
#[test]
fn no_cwd_relative_data_path_literals_remain_in_src() {
    const BANNED: [&str; 3] = [
        "sqlite://family.db",
        "\"assets/uploads\"",
        "\"assets/screensaver\"",
    ];

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for path in rust_sources(&root) {
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("{} is readable: {err}", path.display()));
        for needle in BANNED {
            assert!(
                !source.contains(needle),
                "{} still contains the CWD-relative literal {needle:?}",
                path.display()
            );
        }
    }
}

/// `fullstack_address_or_localhost()` (which reads the bare `IP`/`PORT` env
/// vars) must be gone from the release path.
///
/// T0.6 moved the actual bind call out of `src/main.rs` and into
/// `src/server/router.rs::run` (`main.rs` is now < 25 lines and defines no
/// routes/binds itself — PLAN v2 T0.6 / `docs/reviews/PURPLE_TEAM.md` §P4),
/// so this test now reads `router.rs` for the `config.http_addr` bind and
/// asserts the removed helper is gone from **both** files.
#[test]
fn fullstack_address_or_localhost_is_removed_from_the_release_path() {
    let main_rs =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs"))
            .expect("src/main.rs is readable");
    let router_rs =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/server/router.rs"))
            .expect("src/server/router.rs is readable");

    assert!(
        !main_rs.contains("fullstack_address_or_localhost"),
        "src/main.rs must not call fullstack_address_or_localhost() any more"
    );
    assert!(
        !router_rs.contains("fullstack_address_or_localhost"),
        "src/server/router.rs must not call fullstack_address_or_localhost() any more"
    );
    assert!(
        router_rs.contains("config.http_addr"),
        "src/server/router.rs should bind FamilyHubConfig::http_addr instead"
    );
}

fn rust_sources(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        let entries = std::fs::read_dir(&next)
            .unwrap_or_else(|err| panic!("{} is readable: {err}", next.display()));
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}
