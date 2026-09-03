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

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

fn family_hub_exe() -> &'static str {
    env!("CARGO_BIN_EXE_family-hub")
}

/// HS9 (`docs/BACKLOG.md` B-3): every child process below is given an
/// explicit `FAMILY_HUB_DATA_DIR`, but a child also inherits this test
/// process's environment — so pin it here too, once, rather than trusting the
/// shell that launched `cargo test`. A future test that forgets the `.env()`
/// call then still lands in `%TEMP%`, never in the family's live
/// `%ProgramData%\FamilyHub`.
fn init_test_env() -> PathBuf {
    static ONCE: std::sync::Once = std::sync::Once::new();
    let base = std::env::temp_dir().join(format!("familyhub-service-tests-{}", std::process::id()));
    ONCE.call_once(|| {
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("test scratch directory is creatable");
        std::env::set_var("FAMILY_HUB_DATA_DIR", &base);
    });
    base
}

fn scratch_data_dir(name: &str) -> PathBuf {
    init_test_env();
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

/// PURPLE §P3 T3.1(c): "a deliberate startup failure appears in the log file
/// within 5 s (proves logging precedes everything)". Q1-04: exercises the
/// **real** startup path — `router::run` returning `RunError::Bind` from a
/// genuine second bind on an already-occupied port, propagated through
/// `service::run_console` to a non-zero process exit — rather than a
/// hand-written `tracing::error!` call standing in for a startup failure.
#[test]
fn a_startup_bind_failure_is_logged_within_five_seconds() {
    let data_dir = scratch_data_dir("bind-failure");

    // Occupy a real ephemeral port first, so the hub's own HTTP bind fails.
    let occupied = std::net::TcpListener::bind("127.0.0.1:0").expect("bind an ephemeral port");
    let addr = occupied.local_addr().expect("local_addr");

    let mut child = Command::new(family_hub_exe())
        .arg("run")
        .env("FAMILY_HUB_DATA_DIR", &data_dir)
        .env("FAMILY_HUB_ADDR", addr.to_string())
        .env("FAMILY_HUB_TLS_ADDR", "127.0.0.1:0")
        .spawn()
        .expect("failed to spawn family-hub.exe run");

    let started = Instant::now();
    let deadline = started + Duration::from_secs(15);
    let status = loop {
        if let Some(status) = child.try_wait().expect("try_wait should not error") {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("family-hub.exe run did not exit within 15s of a startup bind failure");
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let elapsed = started.elapsed();
    drop(occupied);

    assert!(
        !status.success(),
        "family-hub.exe run must exit non-zero when it fails to start, got {status:?}"
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "a startup bind failure took {elapsed:?} to end the process, expected under 5s"
    );

    let log_path = data_dir.join("logs").join("familyhub.log");
    let contents = std::fs::read_to_string(&log_path).unwrap_or_else(|err| {
        panic!(
            "expected {} to exist and be readable: {err}",
            log_path.display()
        )
    });
    assert!(
        contents.contains("failed to bind"),
        "familyhub.log did not record the bind failure: {contents:?}"
    );

    let _ = std::fs::remove_dir_all(&data_dir);
}

/// A free `127.0.0.1` TCP port, found by binding an ephemeral one and
/// releasing it immediately. Same pattern the bind-failure test above uses
/// to *occupy* a port; here the listener is dropped instead of held, so the
/// port is free again by the time `family-hub.exe` binds it moments later.
fn free_local_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind an ephemeral port");
    listener.local_addr().expect("local_addr").port()
}

/// A minimal blocking HTTP/1.1 GET, just enough to read the status line —
/// no need for a full HTTP client dependency to poll `/health` for "the
/// server answers yet".
fn http_get_status_code(port: u16, path: &str) -> Option<u16> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .ok()?;
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    )
    .ok()?;
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf);
    text.lines().next()?.split_whitespace().nth(1)?.parse().ok()
}

/// Q2-01 (QA round 2): `router::run` must generate the first-run parent PIN
/// setup code unconditionally at boot — before this fix it was only ever
/// generated by a `#[server]` fn nothing in `src/client/` ever called, so on
/// a real install `<data>\setup-code.txt` and the log line the owner
/// checklist tells the parent to read never appeared, and every parent-only
/// feature was permanently unreachable. This exercises the **real** boot
/// path (a real `family-hub.exe run` process) and asserts the setup-code
/// file exists once `/health` actually answers, and that the log recorded
/// generating it.
#[test]
fn run_generates_the_first_run_setup_code_and_logs_it_once_health_answers() {
    let data_dir = scratch_data_dir("setup-code");
    let http_port = free_local_port();

    let mut child = Command::new(family_hub_exe())
        .arg("run")
        .env("FAMILY_HUB_DATA_DIR", &data_dir)
        .env("FAMILY_HUB_ADDR", format!("127.0.0.1:{http_port}"))
        .env("FAMILY_HUB_TLS_ADDR", "127.0.0.1:0")
        .spawn()
        .expect("failed to spawn family-hub.exe run");

    let deadline = Instant::now() + Duration::from_secs(30);
    let mut health_answered = false;
    while Instant::now() < deadline {
        if http_get_status_code(http_port, "/health") == Some(200) {
            health_answered = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(
        health_answered,
        "family-hub.exe run's /health never answered 200 within 30s on port {http_port}"
    );

    let setup_code_path = data_dir.join("setup-code.txt");
    let log_path = data_dir.join("logs").join("familyhub.log");

    let _ = child.kill();
    let _ = child.wait();

    assert!(
        setup_code_path.exists(),
        "expected {} to exist once /health answered",
        setup_code_path.display()
    );
    let code = std::fs::read_to_string(&setup_code_path).expect("setup-code.txt is readable");
    assert_eq!(
        code.trim().len(),
        6,
        "the setup code must be exactly six digits, got {code:?}"
    );
    assert!(
        code.trim().bytes().all(|b| b.is_ascii_digit()),
        "the setup code must be all digits, got {code:?}"
    );

    let log_contents = std::fs::read_to_string(&log_path).unwrap_or_else(|err| {
        panic!(
            "expected {} to exist and be readable: {err}",
            log_path.display()
        )
    });
    assert!(
        log_contents.contains("generated the first-run parent PIN setup code"),
        "familyhub.log did not record generating the setup code: {log_contents:?}"
    );

    let _ = std::fs::remove_dir_all(&data_dir);
}

// ---------------------------------------------------------------------------
// HS1 (g): family-hub.exe import-curriculum <path> [--replace]
// ---------------------------------------------------------------------------

/// HS1 accept (g): "`import-curriculum` with a bad path or bad TOML exits
/// non-zero and **writes nothing**".
///
/// Validation runs before the copy on purpose, so a file a parent mistyped
/// never lands in the directory the hub scans at every boot. Both failure
/// shapes are exercised against the real binary, with `FAMILY_HUB_DATA_DIR`
/// pointed at a scratch directory — never the owner's.
#[test]
fn import_curriculum_with_a_bad_path_or_bad_toml_exits_non_zero_and_writes_nothing() {
    let data_dir = scratch_data_dir("import-bad");
    let curricula = data_dir.join("curricula");

    // (1) a path that does not exist at all.
    let missing = data_dir.join("nowhere.toml");
    let output = Command::new(family_hub_exe())
        .args(["import-curriculum", &missing.display().to_string()])
        .env("FAMILY_HUB_DATA_DIR", &data_dir)
        .output()
        .expect("failed to spawn family-hub.exe import-curriculum");
    assert!(
        !output.status.success(),
        "a missing path must exit non-zero, got {:?}",
        output.status
    );
    assert!(
        !curricula.join("nowhere.toml").exists(),
        "nothing may be copied for a path that does not exist"
    );

    // (2) a file that exists but does not validate.
    let bad = data_dir.join("bad.toml");
    std::fs::write(
        &bad,
        "[curriculum]\nslug = \"Not A Slug\"\nname = \"Bad\"\nweeks = 2\n",
    )
    .expect("write bad.toml");

    let output = Command::new(family_hub_exe())
        .args(["import-curriculum", &bad.display().to_string()])
        .env("FAMILY_HUB_DATA_DIR", &data_dir)
        .output()
        .expect("failed to spawn family-hub.exe import-curriculum");
    assert!(
        !output.status.success(),
        "a file that fails validation must exit non-zero, got {:?}",
        output.status
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("bad.toml") && stderr.contains("line "),
        "the failure must name the file and the line: {stderr}"
    );
    assert!(
        !curricula.join("bad.toml").exists(),
        "a rejected file must never be copied into the curricula directory: {stderr}"
    );
    assert!(
        !data_dir.join("family.db").exists(),
        "a rejected file must not even open the database"
    );

    let _ = std::fs::remove_dir_all(&data_dir);
}

/// The other half of HS1 (g): the happy path really does validate, copy and
/// insert — otherwise "exits non-zero and writes nothing" would be trivially
/// satisfied by a subcommand that never works at all.
#[test]
fn import_curriculum_copies_a_valid_file_into_the_curricula_directory() {
    let data_dir = scratch_data_dir("import-good");
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/curricula/sample-year.toml");
    assert!(fixture.is_file(), "missing {}", fixture.display());

    let output = Command::new(family_hub_exe())
        .args(["import-curriculum", &fixture.display().to_string()])
        .env("FAMILY_HUB_DATA_DIR", &data_dir)
        .output()
        .expect("failed to spawn family-hub.exe import-curriculum");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "importing the committed fixture must succeed: {stdout}{stderr}"
    );
    assert!(
        stdout.contains("sample-year"),
        "the summary must name the curriculum: {stdout}"
    );
    // QH1-08: the counts must be the ones this command inserted. Opening the
    // pool runs the boot loader over the curricula directory, so a copy made
    // before the pool opens would leave the command reporting all zeroes.
    assert!(
        stdout.contains("7 subjects, 9 assignments, 3 term notes inserted"),
        "the summary must report the rows this import inserted, not zeroes: {stdout}"
    );
    assert!(
        data_dir
            .join("curricula")
            .join("sample-year.toml")
            .is_file(),
        "the file must be copied where the boot-time loader will find it"
    );
    assert!(data_dir.join("family.db").is_file());

    let _ = std::fs::remove_dir_all(&data_dir);
}
