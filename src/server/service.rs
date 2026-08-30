//! Windows service host + CLI (PLAN v2 D9, task T3.1).
//!
//! `family-hub.exe install|uninstall|start|stop|status|run|tv-probe`, all via
//! [`windows_service::service_manager`] — **no PowerShell scripts**
//! (`docs/reviews/PURPLE_TEAM.md` I-10). This is the one file that owns the
//! Service Control Manager (SCM) integration, the service-hosted logger, and
//! the `install` subcommand's firewall/power-plan configuration.
//!
//! Two seams keep the SCM- and OS-facing pieces unit-testable without an
//! elevated prompt (my task explicitly forbids attempting a real elevated
//! install — that is owner step **A3**, `docs/OWNER_CHECKLIST.md`):
//!
//! * [`ServiceHost`] wraps the four SCM operations the CLI needs
//!   (create/delete/start/stop/query). [`WindowsServiceHost`] is the real
//!   implementation ([`windows_service::service_manager`]); tests inject a
//!   [`MockServiceHost`] instead.
//! * [`CommandRunner`] wraps "run an external program and capture whether it
//!   succeeded" for the two OS built-ins `install` shells out to (`netsh.exe`
//!   for the firewall, `powercfg.exe` for the power plan) — the same "OS
//!   built-in invoked at runtime, not a new project dependency" basis
//!   `icacls.exe` was accepted under (`docs/HANDOFF.md` H-12, ratified at the
//!   wave 1-a close, which named `netsh.exe` as T3.1's future addition on
//!   the same reasoning). Tests inject a [`RecordingCommandRunner`].
//!
//! Logging is the other half of D9 ("file + Event Log logging as the first
//! statement"). [`ServiceLogger`] is a `tracing::Subscriber` — like
//! `server::config`'s `CountingSubscriber` test double and
//! `server::api::realtime`'s `TokenBucket`/`RateLimiter`, it takes no
//! dependency on Dioxus, a socket or a running service, so
//! [`ServiceLogger::open`] plus a scoped `tracing::subscriber::with_default`
//! is exactly what the unit tests below exercise directly, and
//! [`run_console`]/[`win_service_main`] install it globally as the *first*
//! statement of the real startup path. It writes every event to the rotating
//! log file (`server::backup::rotate_log_if_needed`, reused rather than
//! duplicated — T1.6 already owns that logic) and best-effort mirrors it to
//! the Windows Event Log via a direct `advapi32.dll` FFI call
//! (`RegisterEventSourceW`/`ReportEventW`/`DeregisterEventSource`) — the same
//! "OS API reached by a direct `extern \"system\"` call, no crate, no
//! spawned process" basis `server::health::disk_free_bytes` already uses for
//! `kernel32.dll::GetDiskFreeSpaceExW` (`docs/NON_RUST.md`).

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use crate::server::backup;
use crate::server::config::FamilyHubConfig;

/// The service name the SCM knows this hub by (`sc query FamilyHub`,
/// PURPLE §P3 T3.1(a)).
pub const SERVICE_NAME: &str = "FamilyHub";
const SERVICE_DISPLAY_NAME: &str = "Sheffield Family Hub";
const SERVICE_DESCRIPTION: &str =
    "Sheffield Family Calendar & Routine Hub — local-first family calendar, \
     routines, whiteboard and photo tasks server.";
/// The Windows Event Log source name [`ServiceLogger`] reports under.
pub const EVENT_SOURCE: &str = "FamilyHub";
/// `<data>\logs\familyhub.log` — matches `docs/HANDOFF.md` H-16's assumption
/// (recorded, not applied, at the T1.6 close: "T3.1 owns the actual service
/// logging setup").
pub const LOG_FILE_NAME: &str = "familyhub.log";

const USAGE: &str = "usage: family-hub.exe <install|uninstall|start|stop|status|run|tv-probe>";

// ---------------------------------------------------------------------------
// CLI entry point (called by src/bin/family_hub.rs)
// ---------------------------------------------------------------------------

/// Dispatch an explicit subcommand (argv already stripped of argv[0]) and
/// return the process exit code. `src/bin/family_hub.rs` calls this for
/// every recognised subcommand; an empty argument list is handled one layer
/// up (`try_run_as_service`), since only that path involves the SCM
/// dispatcher.
pub fn dispatch(args: &[String]) -> i32 {
    match args.first().map(String::as_str) {
        Some("install") => run_and_report(cmd_install()),
        Some("uninstall") => run_and_report(cmd_uninstall()),
        Some("start") => run_and_report(cmd_start()),
        Some("stop") => run_and_report(cmd_stop()),
        Some("status") => cmd_status(),
        Some("run") => {
            run_console(FamilyHubConfig::load());
            0 // unreachable in practice: run_console serves forever.
        }
        Some("tv-probe") => run_and_report(cmd_tv_probe()),
        Some(other) => {
            eprintln!("unknown subcommand {other:?}\n{USAGE}");
            2
        }
        None => {
            eprintln!("{USAGE}");
            2
        }
    }
}

fn run_and_report(result: Result<String, ServiceError>) -> i32 {
    match result {
        Ok(message) => {
            println!("{message}");
            0
        }
        Err(err) => {
            eprintln!("{err}");
            1
        }
    }
}

// ---------------------------------------------------------------------------
// ServiceHost: the SCM operations the CLI needs, behind a trait so the four
// mutating subcommands are unit-testable without an elevated real install
// (PURPLE §P3 T3.1: "install/uninstall unit-tested against a mocked
// service_manager").
// ---------------------------------------------------------------------------

/// Everything this module can fail with, concrete rather than boxed so a
/// caller (and a test) can match on the kind.
#[derive(Debug)]
pub enum ServiceError {
    Scm(String),
    Io(io::Error),
    NotInstalled,
}

impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Scm(msg) => write!(f, "service control manager error: {msg}"),
            Self::Io(err) => write!(f, "I/O error: {err}"),
            Self::NotInstalled => write!(f, "the {SERVICE_NAME} service is not installed"),
        }
    }
}

impl std::error::Error for ServiceError {}

impl From<io::Error> for ServiceError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

/// The coarse run state [`ServiceHost::query_state`] reports — just what the
/// `status` subcommand and `install`'s "already exists" check need, not the
/// SCM's full `SERVICE_STATUS` structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceRunState {
    Running,
    Stopped,
    StartPending,
    StopPending,
    Other,
}

impl std::fmt::Display for ServiceRunState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Running => "RUNNING",
            Self::Stopped => "STOPPED",
            Self::StartPending => "START_PENDING",
            Self::StopPending => "STOP_PENDING",
            Self::Other => "UNKNOWN",
        };
        f.write_str(s)
    }
}

/// The four SCM operations `install`/`uninstall`/`start`/`stop`/`status`
/// need. A trait, not a direct call to [`windows_service::service_manager`],
/// so the mutating subcommands below can be exercised against
/// [`MockServiceHost`] without touching the real SCM.
pub trait ServiceHost {
    /// Register the service, pointing at `exe_path` with no launch
    /// arguments (the SCM starts it with none; `family-hub.exe` detects the
    /// SCM launch via [`try_run_as_service`]).
    fn create(&self, exe_path: &Path) -> Result<(), ServiceError>;
    fn delete(&self) -> Result<(), ServiceError>;
    fn start(&self) -> Result<(), ServiceError>;
    fn stop(&self) -> Result<(), ServiceError>;
    fn query_state(&self) -> Result<ServiceRunState, ServiceError>;
}

// ---------------------------------------------------------------------------
// The real implementation, windows-service =0.8.1 (PURPLE §P5.4).
// ---------------------------------------------------------------------------

#[cfg(windows)]
pub struct WindowsServiceHost;

#[cfg(windows)]
impl WindowsServiceHost {
    pub fn new() -> Self {
        Self
    }

    fn manager(
        &self,
        access: windows_service::service_manager::ServiceManagerAccess,
    ) -> Result<windows_service::service_manager::ServiceManager, ServiceError> {
        windows_service::service_manager::ServiceManager::local_computer(None::<&str>, access)
            .map_err(|e| ServiceError::Scm(e.to_string()))
    }
}

#[cfg(windows)]
impl Default for WindowsServiceHost {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(windows)]
impl ServiceHost for WindowsServiceHost {
    fn create(&self, exe_path: &Path) -> Result<(), ServiceError> {
        use windows_service::service::{
            ServiceAccess, ServiceAction, ServiceActionType, ServiceErrorControl,
            ServiceFailureActions, ServiceFailureResetPeriod, ServiceInfo, ServiceStartType,
            ServiceType,
        };
        use windows_service::service_manager::ServiceManagerAccess;

        let manager =
            self.manager(ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE)?;

        let info = ServiceInfo {
            name: OsString::from(SERVICE_NAME),
            display_name: OsString::from(SERVICE_DISPLAY_NAME),
            service_type: ServiceType::OWN_PROCESS,
            start_type: ServiceStartType::AutoStart,
            error_control: ServiceErrorControl::Normal,
            executable_path: exe_path.to_path_buf(),
            launch_arguments: Vec::new(),
            dependencies: Vec::new(),
            account_name: None, // LocalSystem
            account_password: None,
        };

        let service = manager
            .create_service(&info, ServiceAccess::START | ServiceAccess::CHANGE_CONFIG)
            .map_err(|e| ServiceError::Scm(e.to_string()))?;
        service
            .set_description(SERVICE_DESCRIPTION)
            .map_err(|e| ServiceError::Scm(e.to_string()))?;

        // Q1-04: a startup failure now makes `run_service` report `Stopped`
        // with a non-zero exit code (`scm::run_service`, above) instead of
        // leaving the service RUNNING forever — these SCM recovery actions
        // are what actually turns that into a self-healing restart.
        service
            .update_failure_actions(ServiceFailureActions {
                reset_period: ServiceFailureResetPeriod::After(Duration::from_secs(86_400)),
                reboot_msg: None,
                command: None,
                actions: Some(vec![
                    ServiceAction {
                        action_type: ServiceActionType::Restart,
                        delay: Duration::from_secs(5),
                    },
                    ServiceAction {
                        action_type: ServiceActionType::Restart,
                        delay: Duration::from_secs(30),
                    },
                    ServiceAction {
                        action_type: ServiceActionType::Restart,
                        delay: Duration::from_secs(60),
                    },
                ]),
            })
            .map_err(|e| ServiceError::Scm(e.to_string()))?;
        service
            .set_failure_actions_on_non_crash_failures(true)
            .map_err(|e| ServiceError::Scm(e.to_string()))?;

        Ok(())
    }

    fn delete(&self) -> Result<(), ServiceError> {
        use windows_service::service::{ServiceAccess, ServiceState};
        use windows_service::service_manager::ServiceManagerAccess;

        let manager = self.manager(ServiceManagerAccess::CONNECT)?;
        let service = manager
            .open_service(
                SERVICE_NAME,
                ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE,
            )
            .map_err(|_| ServiceError::NotInstalled)?;

        if let Ok(status) = service.query_status() {
            if status.current_state != ServiceState::Stopped {
                let _ = service.stop();
            }
        }
        service
            .delete()
            .map_err(|e| ServiceError::Scm(e.to_string()))
    }

    fn start(&self) -> Result<(), ServiceError> {
        use windows_service::service::ServiceAccess;
        use windows_service::service_manager::ServiceManagerAccess;

        let manager = self.manager(ServiceManagerAccess::CONNECT)?;
        let service = manager
            .open_service(SERVICE_NAME, ServiceAccess::START)
            .map_err(|_| ServiceError::NotInstalled)?;
        service
            .start(&[] as &[&std::ffi::OsStr])
            .map_err(|e| ServiceError::Scm(e.to_string()))
    }

    fn stop(&self) -> Result<(), ServiceError> {
        use windows_service::service::ServiceAccess;
        use windows_service::service_manager::ServiceManagerAccess;

        let manager = self.manager(ServiceManagerAccess::CONNECT)?;
        let service = manager
            .open_service(SERVICE_NAME, ServiceAccess::STOP)
            .map_err(|_| ServiceError::NotInstalled)?;
        service
            .stop()
            .map(|_| ())
            .map_err(|e| ServiceError::Scm(e.to_string()))
    }

    fn query_state(&self) -> Result<ServiceRunState, ServiceError> {
        use windows_service::service::{ServiceAccess, ServiceState};
        use windows_service::service_manager::ServiceManagerAccess;

        let manager = self.manager(ServiceManagerAccess::CONNECT)?;
        let service = manager
            .open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS)
            .map_err(|_| ServiceError::NotInstalled)?;
        let status = service
            .query_status()
            .map_err(|e| ServiceError::Scm(e.to_string()))?;

        Ok(match status.current_state {
            ServiceState::Running => ServiceRunState::Running,
            ServiceState::Stopped => ServiceRunState::Stopped,
            ServiceState::StartPending => ServiceRunState::StartPending,
            ServiceState::StopPending => ServiceRunState::StopPending,
            _ => ServiceRunState::Other,
        })
    }
}

/// Off Windows (e.g. `cargo check --features server` in a Linux container —
/// this project targets Windows only, PLAN v2 D9, but the type still needs
/// to exist so the module type-checks), every operation fails cleanly rather
/// than not compiling at all.
#[cfg(not(windows))]
pub struct WindowsServiceHost;

#[cfg(not(windows))]
impl WindowsServiceHost {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(not(windows))]
impl Default for WindowsServiceHost {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(windows))]
impl ServiceHost for WindowsServiceHost {
    fn create(&self, _exe_path: &Path) -> Result<(), ServiceError> {
        Err(ServiceError::Scm("not running on Windows".to_string()))
    }
    fn delete(&self) -> Result<(), ServiceError> {
        Err(ServiceError::Scm("not running on Windows".to_string()))
    }
    fn start(&self) -> Result<(), ServiceError> {
        Err(ServiceError::Scm("not running on Windows".to_string()))
    }
    fn stop(&self) -> Result<(), ServiceError> {
        Err(ServiceError::Scm("not running on Windows".to_string()))
    }
    fn query_state(&self) -> Result<ServiceRunState, ServiceError> {
        Err(ServiceError::Scm("not running on Windows".to_string()))
    }
}

// ---------------------------------------------------------------------------
// The four CLI subcommands that touch the SCM.
// ---------------------------------------------------------------------------

fn cmd_install() -> Result<String, ServiceError> {
    install_with(&WindowsServiceHost::new(), &RealCommandRunner)
}

/// Testable core of `install`: register the service pointed at the running
/// executable, then configure the firewall and power plan (best-effort, per
/// [`configure_firewall`]/[`configure_power_plan`] — a hub that cannot
/// tighten either must still finish installing, mirroring T1.3's ACL
/// precedent, `docs/HANDOFF.md` H-12).
fn install_with(
    host: &dyn ServiceHost,
    runner: &dyn CommandRunner,
) -> Result<String, ServiceError> {
    let exe_path = std::env::current_exe()?;
    install_with_exe(host, runner, exe_path)
}

/// [`install_with`]'s body, taking the executable path as an explicit
/// argument (Q1-01) rather than always resolving `std::env::current_exe()`
/// — the seam that lets a unit test point the wasm-bundle check below at a
/// scratch directory instead of wherever the test binary itself happens to
/// live.
fn install_with_exe(
    host: &dyn ServiceHost,
    runner: &dyn CommandRunner,
    exe_path: PathBuf,
) -> Result<String, ServiceError> {
    // Q1-01: refuse to register a service that cannot serve the kiosk at
    // all. The binary the owner is told to install is `cargo build
    // --release --bin family-hub` — with no `public\` bundle beside it
    // (from `dx build --platform web --release`), `/tv` and `/m` render but
    // never hydrate: no wasm client, no WebSocket, no D-pad handler. Skipped
    // when `DIOXUS_PUBLIC_PATH` is set — that env var is itself an explicit
    // "the bundle lives somewhere else" declaration (`server::router::
    // ensure_public_dir_exists` resolves the same variable).
    if std::env::var_os("DIOXUS_PUBLIC_PATH").is_none() {
        let public = exe_path
            .parent()
            .map(|dir| dir.join("public"))
            .unwrap_or_default();
        if !crate::server::router::public_bundle_present(&public) {
            return Err(ServiceError::Io(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "{} has no wasm client bundle; copy target/dx/family-calendar/release/web/public \
                     beside family-hub.exe first",
                    public.display()
                ),
            )));
        }
    }

    host.create(&exe_path)?;

    let firewall = configure_firewall(runner);
    let power = configure_power_plan(runner);

    Ok(format!(
        "installed {SERVICE_NAME} ({} at {}); firewall rules ok: {}/{}; power-plan commands ok: {}/{}",
        SERVICE_DISPLAY_NAME,
        exe_path.display(),
        firewall.iter().filter(|o| o.succeeded).count(),
        firewall.len(),
        power.iter().filter(|o| o.succeeded).count(),
        power.len(),
    ))
}

fn cmd_uninstall() -> Result<String, ServiceError> {
    uninstall_with(&WindowsServiceHost::new())
}

fn uninstall_with(host: &dyn ServiceHost) -> Result<String, ServiceError> {
    host.delete()?;
    Ok(format!("uninstalled {SERVICE_NAME}"))
}

fn cmd_start() -> Result<String, ServiceError> {
    start_with(&WindowsServiceHost::new())
}

fn start_with(host: &dyn ServiceHost) -> Result<String, ServiceError> {
    host.start()?;
    Ok(format!("started {SERVICE_NAME}"))
}

fn cmd_stop() -> Result<String, ServiceError> {
    stop_with(&WindowsServiceHost::new())
}

fn stop_with(host: &dyn ServiceHost) -> Result<String, ServiceError> {
    host.stop()?;
    Ok(format!("stopped {SERVICE_NAME}"))
}

fn cmd_status() -> i32 {
    status_with(&WindowsServiceHost::new())
}

fn status_with(host: &dyn ServiceHost) -> i32 {
    match host.query_state() {
        Ok(state) => {
            println!("{SERVICE_NAME}: {state}");
            0
        }
        Err(ServiceError::NotInstalled) => {
            println!("{SERVICE_NAME}: NOT_INSTALLED");
            1
        }
        Err(err) => {
            eprintln!("{err}");
            1
        }
    }
}

// ---------------------------------------------------------------------------
// CommandRunner: netsh.exe (firewall) / powercfg.exe (power plan), the OS
// built-ins `install` shells out to (`docs/NON_RUST.md`).
// ---------------------------------------------------------------------------

/// One shelled-out command's outcome — recorded rather than propagated as a
/// hard error, because a hub that cannot configure the firewall or the power
/// plan (no elevation, a locked-down machine) must still finish installing;
/// the failure is logged and surfaced in `install`'s own summary line
/// instead.
#[derive(Debug, Clone)]
pub struct CommandOutcome {
    pub program: String,
    pub args: Vec<String>,
    pub succeeded: bool,
}

/// Indirection over "run an external program", so [`configure_firewall`] and
/// [`configure_power_plan`] are unit-testable without actually shelling out
/// (and without needing elevation to observe the exact commands built).
pub trait CommandRunner {
    fn run(&self, program: &str, args: &[&str]) -> io::Result<bool>;
}

pub struct RealCommandRunner;

impl CommandRunner for RealCommandRunner {
    fn run(&self, program: &str, args: &[&str]) -> io::Result<bool> {
        let status = std::process::Command::new(program).args(args).output()?;
        Ok(status.status.success())
    }
}

/// Records every invocation without running anything, for unit tests.
#[derive(Default)]
pub struct RecordingCommandRunner {
    pub calls: Mutex<Vec<(String, Vec<String>)>>,
    /// If set, [`CommandRunner::run`] returns this instead of `Ok(true)` —
    /// lets a test prove a failed command is still recorded and does not
    /// abort the rest of `install`.
    pub fail: bool,
}

impl CommandRunner for RecordingCommandRunner {
    fn run(&self, program: &str, args: &[&str]) -> io::Result<bool> {
        if let Ok(mut calls) = self.calls.lock() {
            calls.push((
                program.to_string(),
                args.iter().map(|s| s.to_string()).collect(),
            ));
        }
        Ok(!self.fail)
    }
}

fn record_outcome(runner: &dyn CommandRunner, program: &str, args: &[&str]) -> CommandOutcome {
    let succeeded = match runner.run(program, args) {
        Ok(ok) => ok,
        Err(err) => {
            tracing::warn!(program, ?args, %err, "install: command failed to launch");
            false
        }
    };
    if !succeeded {
        tracing::warn!(program, ?args, "install: command exited non-zero");
    }
    CommandOutcome {
        program: program.to_string(),
        args: args.iter().map(|s| s.to_string()).collect(),
        succeeded,
    }
}

/// Three inbound-allow rules named `FamilyHub*` (PURPLE §P3 T3.1(d): `netsh
/// advfirewall firewall show rule name=FamilyHub*` lists 3 rules) — TCP
/// 8080/8443 (D3′'s two origins) and UDP 5353 (mDNS, `docs/reviews/
/// PURPLE_TEAM.md` R-09: "UDP/5353 firewall rule").
pub fn configure_firewall(runner: &dyn CommandRunner) -> Vec<CommandOutcome> {
    const RULES: [(&str, &str, &str); 3] = [
        ("FamilyHub HTTP", "TCP", "8080"),
        ("FamilyHub HTTPS", "TCP", "8443"),
        ("FamilyHub mDNS", "UDP", "5353"),
    ];

    RULES
        .iter()
        .map(|(name, proto, port)| {
            let name_arg = format!("name={name}");
            let proto_arg = format!("protocol={proto}");
            let port_arg = format!("localport={port}");
            record_outcome(
                runner,
                "netsh.exe",
                &[
                    "advfirewall",
                    "firewall",
                    "add",
                    "rule",
                    &name_arg,
                    "dir=in",
                    "action=allow",
                    &proto_arg,
                    &port_arg,
                ],
            )
        })
        .collect()
}

/// Never sleep or hibernate on AC power (D9: "power plan set to never
/// sleep/hibernate") — a wall display that goes dark or drops its server
/// mid-routine is a worse failure mode than a slightly higher power bill.
pub fn configure_power_plan(runner: &dyn CommandRunner) -> Vec<CommandOutcome> {
    const SETTINGS: [&str; 3] = [
        "standby-timeout-ac",
        "hibernate-timeout-ac",
        "monitor-timeout-ac",
    ];

    SETTINGS
        .iter()
        .map(|setting| record_outcome(runner, "powercfg.exe", &["/change", setting, "0"]))
        .collect()
}

// ---------------------------------------------------------------------------
// ServiceLogger: file (rotating) + best-effort Windows Event Log. Installed
// as the very first statement of the real startup path (D9).
// ---------------------------------------------------------------------------

/// The number of buffered, sub-WARN lines [`ServiceLogger`] will hold in
/// memory before flushing anyway (Q1-05) — a bound on how much a crash (not
/// a clean exit; those go through a WARN/ERROR or [`Drop`]) could lose,
/// independent of how chatty a burst of INFO/DEBUG events gets.
const FLUSH_EVERY_N_LINES: u32 = 64;

/// The `FAMILY_HUB_LOG` environment variable, parsed.
/// `trace`/`debug`/`warn`/`error` (case-insensitive) raise or lower the
/// sink's level; anything else (unset included) is `§P5.5` default 33's
/// `info`. See `docs/DEV_WINDOWS.md` and `docs/RECOVERY.md`.
fn level_from_env() -> tracing::Level {
    match std::env::var("FAMILY_HUB_LOG")
        .ok()
        .as_deref()
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("trace") => tracing::Level::TRACE,
        Some("debug") => tracing::Level::DEBUG,
        Some("warn") => tracing::Level::WARN,
        Some("error") => tracing::Level::ERROR,
        _ => tracing::Level::INFO,
    }
}

/// The buffered writer plus how many lines have accumulated in it since the
/// last flush — both guarded by one lock so a rotate-and-reopen (which
/// replaces the writer) and the line counter it resets can never be seen out
/// of step with each other by a concurrent event.
struct LoggerFile {
    writer: io::BufWriter<std::fs::File>,
    lines_since_flush: u32,
}

/// A `tracing::Subscriber` that appends every event at or above
/// [`ServiceLogger::max_level`] to a rotating log file and mirrors WARN+
/// events, best-effort, to the Windows Event Log. Deliberately not
/// installed via `tracing::subscriber::set_global_default` inside the unit
/// tests below (that can only succeed once per process, and `cargo test`
/// runs every test in this file in one process) — tests instead use
/// `tracing::subscriber::with_default`, exactly like
/// `server::config::tests::CountingSubscriber` already does.
pub struct ServiceLogger {
    file: Mutex<LoggerFile>,
    path: PathBuf,
    event_source: String,
    /// Q1-05: `§P5.5` default 33 — `info` in the service, `debug` behind
    /// `FAMILY_HUB_LOG`. Previously absent entirely (`enabled()` always
    /// returned `true`), so every `dioxus_core`/`hyper`/`sqlx` TRACE event
    /// was formatted and flushed on the request hot path: 543 TRACE lines /
    /// 293 KB in about three idle minutes, enough to churn the 10 MB × 5
    /// ring in well under an hour of real use and bury every real error.
    max_level: tracing::Level,
}

impl ServiceLogger {
    /// Open (creating if needed) `<data>\logs\familyhub.log` and rotate it
    /// first if it is already at the size cap, so a service that has been
    /// running for months never starts a fresh log inside an oversized file.
    pub fn open(log_dir: &Path) -> io::Result<Self> {
        std::fs::create_dir_all(log_dir)?;
        let path = log_dir.join(LOG_FILE_NAME);
        let _ = backup::rotate_log_if_needed(
            &path,
            backup::LOG_ROTATION_MAX_BYTES,
            backup::LOG_ROTATION_MAX_FILES,
        );
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        Ok(Self {
            file: Mutex::new(LoggerFile {
                writer: io::BufWriter::new(file),
                lines_since_flush: 0,
            }),
            path,
            event_source: EVENT_SOURCE.to_string(),
            max_level: level_from_env(),
        })
    }

    pub fn log_path(&self) -> &Path {
        &self.path
    }

    /// Force any buffered lines to disk right now. Called automatically on
    /// [`Drop`] and whenever a WARN+ event is appended; exposed publicly so
    /// a caller (or a test) can force a deterministic flush point without
    /// waiting for either of those (Q1-05).
    pub fn flush(&self) {
        if let Ok(mut file) = self.file.lock() {
            use io::Write as _;
            let _ = file.writer.flush();
            file.lines_since_flush = 0;
        }
    }

    /// Append one formatted line, rotating first if this write would push
    /// the file over the cap. Flushes immediately for `level <= WARN` (a
    /// real problem must reach disk before a crash or `process::exit` can
    /// lose it — `Drop` never runs on `process::exit`) or every
    /// [`FLUSH_EVERY_N_LINES`] lines otherwise; every other write is a
    /// buffered, unflushed `BufWriter` append (Q1-05).
    fn append_line(&self, line: &str, level: tracing::Level) {
        let Ok(mut file) = self.file.lock() else {
            return;
        };
        // Rotate-then-reopen if the file has grown past the cap since we
        // opened it (a long-running service keeps this handle open for its
        // whole life, so `open`'s own one-time rotation is not enough). The
        // size check reads the file's on-disk size, so anything still
        // sitting in `file.writer`'s buffer is invisible to it until the
        // next flush — a bounded, harmless lag against a 10 MB cap.
        if let Ok(true) = backup::rotate_log_if_needed(
            &self.path,
            backup::LOG_ROTATION_MAX_BYTES,
            backup::LOG_ROTATION_MAX_FILES,
        ) {
            if let Ok(reopened) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)
            {
                file.writer = io::BufWriter::new(reopened);
                file.lines_since_flush = 0;
            }
        }
        use io::Write as _;
        let _ = writeln!(file.writer, "{line}");
        file.lines_since_flush += 1;
        if level <= tracing::Level::WARN || file.lines_since_flush >= FLUSH_EVERY_N_LINES {
            let _ = file.writer.flush();
            file.lines_since_flush = 0;
        }
    }

    fn report_to_event_log(&self, level: &str, message: &str) {
        eventlog::report(&self.event_source, level, message);
    }
}

impl Drop for ServiceLogger {
    fn drop(&mut self) {
        self.flush();
    }
}

impl tracing::Subscriber for ServiceLogger {
    fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
        *metadata.level() <= self.max_level
    }
    fn max_level_hint(&self) -> Option<tracing::level_filters::LevelFilter> {
        Some(tracing::level_filters::LevelFilter::from_level(
            self.max_level,
        ))
    }
    fn new_span(&self, _span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}

    fn event(&self, event: &tracing::Event<'_>) {
        let level = *event.metadata().level();
        let target = event.metadata().target();

        struct MessageVisitor(String);
        impl tracing::field::Visit for MessageVisitor {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    self.0 = format!("{value:?}");
                } else if self.0.is_empty() {
                    self.0 = format!("{}={:?}", field.name(), value);
                } else {
                    self.0 = format!("{} {}={:?}", self.0, field.name(), value);
                }
            }
        }
        let mut visitor = MessageVisitor(String::new());
        event.record(&mut visitor);

        let now = chrono::Local::now().to_rfc3339();
        let line = format!("{now} {level:>5} {target}: {}", visitor.0);
        self.append_line(&line, level);

        if level <= tracing::Level::WARN {
            self.report_to_event_log(level.as_str(), &visitor.0);
        }
    }

    fn enter(&self, _span: &tracing::span::Id) {}
    fn exit(&self, _span: &tracing::span::Id) {}
}

/// Direct `advapi32.dll` FFI for the Windows Event Log — the same "OS API by
/// direct `extern \"system\"` call, no crate, no spawned process" basis
/// `server::health::disk_free_bytes` uses for `kernel32.dll` (`docs/
/// NON_RUST.md`). Best-effort throughout: an event source that was never
/// registered in the registry (this hub does not ship an installer that
/// does so) still accepts `ReportEventW` calls — Event Viewer shows a
/// "description not found" notice alongside the raw message text — and any
/// failure here is silently swallowed rather than propagated, because a
/// logging *sink* must never be the reason the real log write (the file)
/// fails to happen.
#[cfg(windows)]
mod eventlog {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;

    #[allow(non_snake_case)]
    #[link(name = "advapi32")]
    extern "system" {
        fn RegisterEventSourceW(
            lp_unc_server_name: *const u16,
            lp_source_name: *const u16,
        ) -> *mut c_void;
        fn ReportEventW(
            h_event_log: *mut c_void,
            w_type: u16,
            w_category: u16,
            dw_event_id: u32,
            lp_user_sid: *const c_void,
            w_num_strings: u16,
            dw_data_size: u32,
            lp_strings: *const *const u16,
            lp_raw_data: *const c_void,
        ) -> i32;
        fn DeregisterEventSource(h_event_log: *mut c_void) -> i32;
    }

    const EVENTLOG_ERROR_TYPE: u16 = 0x0001;
    const EVENTLOG_WARNING_TYPE: u16 = 0x0002;
    const EVENTLOG_INFORMATION_TYPE: u16 = 0x0004;
    const GENERIC_EVENT_ID: u32 = 1;

    fn to_wide(s: &str) -> Vec<u16> {
        std::ffi::OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    fn win_type_for(level: &str) -> u16 {
        match level {
            "ERROR" => EVENTLOG_ERROR_TYPE,
            "WARN" => EVENTLOG_WARNING_TYPE,
            _ => EVENTLOG_INFORMATION_TYPE,
        }
    }

    pub fn report(source: &str, level: &str, message: &str) {
        let wide_source = to_wide(source);
        // SAFETY: `wide_source` is a NUL-terminated UTF-16 buffer that
        // outlives this call. A null server name targets the local machine.
        let handle = unsafe { RegisterEventSourceW(std::ptr::null(), wide_source.as_ptr()) };
        if handle.is_null() {
            return;
        }

        let wide_message = to_wide(message);
        let strings = [wide_message.as_ptr()];
        // SAFETY: `handle` was just obtained and is valid until
        // `DeregisterEventSource`; `strings` holds one valid, NUL-terminated
        // UTF-16 pointer matching `w_num_strings == 1`.
        unsafe {
            ReportEventW(
                handle,
                win_type_for(level),
                0,
                GENERIC_EVENT_ID,
                std::ptr::null(),
                1,
                0,
                strings.as_ptr(),
                std::ptr::null(),
            );
            DeregisterEventSource(handle);
        }
    }
}

#[cfg(not(windows))]
mod eventlog {
    pub fn report(_source: &str, _level: &str, _message: &str) {}
}

// ---------------------------------------------------------------------------
// Startup: shared by the `run` subcommand and the real Windows service.
// ---------------------------------------------------------------------------

/// Open the logger and install it as the *global* `tracing` subscriber —
/// the first statement of both [`run_console`] and the real service's
/// startup (D9). Only the real binary calls this (never the unit tests
/// below, which use a scoped subscriber instead so they stay
/// process-global-state-free); a second call in the same process is a
/// programmer error the caller controls, not something tests exercise.
fn install_global_logger(config: &FamilyHubConfig) -> io::Result<std::sync::Arc<ServiceLogger>> {
    let logger = std::sync::Arc::new(ServiceLogger::open(&config.log_dir())?);
    if tracing::subscriber::set_global_default(logger.clone()).is_err() {
        eprintln!(
            "a tracing subscriber was already installed; familyhub.log will not receive events"
        );
    }
    // Q1-04: a panic anywhere in the process (including inside a detached
    // `runtime.spawn` task, where a panic would otherwise vanish to stderr —
    // nowhere the SCM can see it) is logged through the same sink as every
    // other event, at ERROR, before the default hook's stderr print still
    // runs.
    std::panic::set_hook(Box::new(|info| {
        tracing::error!(%info, "panic");
        eprintln!("{info}");
    }));
    Ok(logger)
}

/// `family-hub.exe run` — the foreground/console mode used by the CWD
/// acceptance test (PURPLE §P3 T3.1(b)) and by a developer running the hub
/// without installing it as a service. Installs the logger first, then
/// hands off to `router::run`, which never returns.
fn run_console(config: FamilyHubConfig) {
    let logger = install_global_logger(&config);
    if let Err(err) = &logger {
        eprintln!("failed to open {}: {err}", config.log_dir().display());
    }
    tracing::info!(pid = std::process::id(), "family-hub run: starting");

    let runtime = tokio::runtime::Runtime::new().expect("failed to start tokio runtime");
    // Q1-04: a startup failure must be logged and exit non-zero, not panic
    // silently inside `block_on`.
    if let Err(err) = runtime.block_on(crate::server::router::run(config)) {
        tracing::error!(%err, "family-hub run: startup failed");
        std::process::exit(1);
    }
}

/// `family-hub.exe tv-probe` — a thin, testable wrapper the plan calls for
/// (`docs/reviews/PURPLE_TEAM.md` I-10: "`scripts/firetv.ps1` also
/// dropped — it becomes `family-hub.exe tv-probe` shelling out to `adb`").
/// T0.0 already ran this probe once, by hand, to write `docs/FIRE_TV.md` and
/// `docs/device.toml` (both files this task does not own — `docs/**` is
/// per-task-disjoint, `docs/reviews/PURPLE_TEAM.md` §P4); this subcommand is
/// the reusable, repeatable form of that same probe for the owner to re-run
/// later (e.g. after replacing the television), reading the same IP source
/// order T0.0's doc comment describes (`FAMILY_HUB_TV_IP` env, then
/// `docs/device.toml`).
fn cmd_tv_probe() -> Result<String, ServiceError> {
    tv_probe_with(&RealCommandRunner)
}

fn tv_probe_with(runner: &dyn CommandRunner) -> Result<String, ServiceError> {
    let Some(ip) = tv_ip_from_env_or_device_toml() else {
        return Ok(
            "no TV IP configured — set FAMILY_HUB_TV_IP or docs/device.toml [tv] ip = \"...\""
                .to_string(),
        );
    };

    let target = format!("{ip}:5555");
    let connected = runner
        .run("adb.exe", &["connect", &target])
        .unwrap_or(false);
    if !connected {
        return Ok(format!("adb connect {target} failed — device unreachable"));
    }

    let _ = runner.run(
        "adb.exe",
        &["-s", &target, "shell", "getprop", "ro.build.version.name"],
    );
    Ok(format!(
        "probed {target} (see console output above for build props)"
    ))
}

/// `FAMILY_HUB_TV_IP` env, else the `[tv] ip = "..."` line of
/// `docs/device.toml` next to the running executable — the same two sources
/// T0.0's probe used (third source, "paired `adb devices`", is left to the
/// owner running `adb devices` themselves; this subcommand does not enumerate
/// paired devices).
fn tv_ip_from_env_or_device_toml() -> Option<String> {
    if let Ok(ip) = std::env::var("FAMILY_HUB_TV_IP") {
        if !ip.trim().is_empty() {
            return Some(ip.trim().to_string());
        }
    }

    let candidates = [
        PathBuf::from("docs/device.toml"),
        std::env::current_exe()
            .ok()?
            .parent()?
            .join("docs/device.toml"),
    ];
    for candidate in candidates {
        if let Ok(contents) = std::fs::read_to_string(&candidate) {
            for line in contents.lines() {
                let line = line.trim();
                if let Some(rest) = line.strip_prefix("ip") {
                    if let Some(value) = rest.trim_start().strip_prefix('=') {
                        let value = value.trim().trim_matches('"');
                        if !value.is_empty() {
                            return Some(value.to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// The real Windows service entry point (SCM-launched only; not exercised by
// this task's unit tests — a real elevated install is owner step A3).
// ---------------------------------------------------------------------------

#[cfg(windows)]
mod scm {
    use super::*;
    use windows_service::service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    };
    use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
    use windows_service::{define_windows_service, service_dispatcher};

    define_windows_service!(ffi_service_main, win_service_main);

    /// Attempt to hand this process to the SCM dispatcher. Returns `true` if
    /// the dispatcher ran (meaning the process really was launched by the
    /// SCM and has now finished servicing it — the caller should exit
    /// quietly); `false` if it was rejected immediately, meaning this is a
    /// console invocation with no recognised subcommand and the caller
    /// should print usage instead.
    pub fn try_run_as_service() -> bool {
        service_dispatcher::start(SERVICE_NAME, ffi_service_main).is_ok()
    }

    fn win_service_main(_arguments: Vec<OsString>) {
        if let Err(err) = run_service() {
            eprintln!("{SERVICE_NAME} service exited with an error: {err}");
        }
    }

    /// `StartPending` with incrementing checkpoints (PURPLE §P3 T3.1: "`
    /// StartPending` with incrementing checkpoints"), the logger opened
    /// before any of it (D9: "logging first"), then the same `router::run`
    /// the `run` subcommand uses, on a tokio runtime built *inside*
    /// `service_main` (PURPLE §P5.4: "not `main`" — `service_main` is the
    /// real OS-facing entry point here since `src/main.rs` is a different,
    /// frozen binary; this crate's equivalent is `win_service_main`, and the
    /// runtime is built here, inside it).
    fn run_service() -> windows_service::Result<()> {
        let config = FamilyHubConfig::load();
        let _logger = install_global_logger(&config);
        tracing::info!("FamilyHub service: starting");

        let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
        let event_handler = move |control_event| -> ServiceControlHandlerResult {
            match control_event {
                ServiceControl::Stop => {
                    let _ = stop_tx.send(());
                    ServiceControlHandlerResult::NoError
                }
                ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
                _ => ServiceControlHandlerResult::NotImplemented,
            }
        };
        let status_handle = service_control_handler::register(SERVICE_NAME, event_handler)?;

        let mut checkpoint: u32 = 1;
        let report_pending = |checkpoint: u32| {
            let _ = status_handle.set_service_status(ServiceStatus {
                service_type: ServiceType::OWN_PROCESS,
                current_state: ServiceState::StartPending,
                controls_accepted: ServiceControlAccept::empty(),
                exit_code: ServiceExitCode::Win32(0),
                checkpoint,
                wait_hint: Duration::from_secs(10),
                process_id: None,
            });
        };
        report_pending(checkpoint);

        let runtime = tokio::runtime::Runtime::new().expect("failed to start tokio runtime");
        checkpoint += 1;
        report_pending(checkpoint);

        let config_for_run = config.clone();
        // Q1-04: keep the JoinHandle so the loop below can notice the server
        // task ending on its own (a startup failure) rather than only ever
        // waking up on an explicit SCM Stop — a service that stayed RUNNING
        // while serving nothing never gave the SCM's recovery actions
        // (`WindowsServiceHost::create`, below) a reason to fire.
        let handle = runtime.spawn(async move { crate::server::router::run(config_for_run).await });
        checkpoint += 1;
        report_pending(checkpoint);

        status_handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Running,
            controls_accepted: ServiceControlAccept::STOP,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })?;
        tracing::info!("FamilyHub service: running");

        // Block the service-control-handler thread until a Stop arrives, or
        // until the server task itself ends unexpectedly (Q1-04);
        // `router::run` keeps serving on the runtime's own worker threads in
        // the meantime.
        let mut exit_code = ServiceExitCode::Win32(0);
        loop {
            match stop_rx.recv_timeout(Duration::from_secs(1)) {
                Ok(()) => break,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) if handle.is_finished() => {
                    tracing::error!("FamilyHub service: the server task ended unexpectedly");
                    exit_code = ServiceExitCode::ServiceSpecific(1);
                    break;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        status_handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Stopped,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code,
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })?;
        tracing::info!("FamilyHub service: stopped");
        Ok(())
    }
}

#[cfg(windows)]
pub use scm::try_run_as_service;

#[cfg(not(windows))]
pub fn try_run_as_service() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// The three `tv_probe_*` tests set/remove `FAMILY_HUB_TV_IP`, which is
    /// process-global; `cargo test` runs them on parallel threads, so they
    /// take this lock first (Boss, wave 3 close).
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "familyhub-service-unit-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    /// A fake `family-hub.exe` path under `dir`, with a `public\assets\*.wasm`
    /// bundle beside it — what `install_with_exe`'s Q1-01 check requires
    /// before it will register the service at all.
    fn bundled_exe_path(dir: &Path) -> PathBuf {
        std::fs::create_dir_all(dir.join("public/assets")).expect("scratch public/assets dir");
        std::fs::write(dir.join("public/assets/app-abc123.wasm"), b"\0asm")
            .expect("scratch wasm file");
        dir.join("family-hub.exe")
    }

    // -----------------------------------------------------------------
    // MockServiceHost — install/uninstall/start/stop/status unit-tested
    // against a mocked service_manager (PURPLE §P3 T3.1).
    // -----------------------------------------------------------------

    struct MockServiceHost {
        installed: Mutex<bool>,
        created_with: Mutex<Option<PathBuf>>,
        state: Mutex<ServiceRunState>,
        fail_next: Mutex<bool>,
    }

    impl MockServiceHost {
        fn not_installed() -> Self {
            Self {
                installed: Mutex::new(false),
                created_with: Mutex::new(None),
                state: Mutex::new(ServiceRunState::Stopped),
                fail_next: Mutex::new(false),
            }
        }
    }

    impl ServiceHost for MockServiceHost {
        fn create(&self, exe_path: &Path) -> Result<(), ServiceError> {
            *self.installed.lock().unwrap() = true;
            *self.created_with.lock().unwrap() = Some(exe_path.to_path_buf());
            Ok(())
        }
        fn delete(&self) -> Result<(), ServiceError> {
            if !*self.installed.lock().unwrap() {
                return Err(ServiceError::NotInstalled);
            }
            *self.installed.lock().unwrap() = false;
            Ok(())
        }
        fn start(&self) -> Result<(), ServiceError> {
            if !*self.installed.lock().unwrap() {
                return Err(ServiceError::NotInstalled);
            }
            if *self.fail_next.lock().unwrap() {
                return Err(ServiceError::Scm("mock start failure".to_string()));
            }
            *self.state.lock().unwrap() = ServiceRunState::Running;
            Ok(())
        }
        fn stop(&self) -> Result<(), ServiceError> {
            if !*self.installed.lock().unwrap() {
                return Err(ServiceError::NotInstalled);
            }
            *self.state.lock().unwrap() = ServiceRunState::Stopped;
            Ok(())
        }
        fn query_state(&self) -> Result<ServiceRunState, ServiceError> {
            if !*self.installed.lock().unwrap() {
                return Err(ServiceError::NotInstalled);
            }
            Ok(*self.state.lock().unwrap())
        }
    }

    #[test]
    fn install_registers_the_service_pointed_at_the_given_executable() {
        let dir = scratch_dir("install-register");
        let exe_path = bundled_exe_path(&dir);
        let host = MockServiceHost::not_installed();
        let runner = RecordingCommandRunner::default();

        let summary = install_with_exe(&host, &runner, exe_path.clone())
            .expect("install succeeds against the mock");

        assert!(*host.installed.lock().unwrap());
        assert!(summary.contains(SERVICE_NAME));
        let created_with = host.created_with.lock().unwrap().clone();
        assert_eq!(
            created_with,
            Some(exe_path),
            "install must point the service at the executable path it was given, not a hard-coded path"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `install_with` (no `_exe` suffix, the real CLI entry point) really
    /// does forward `std::env::current_exe()` — the property the previous
    /// version of this test proved — exercised with `DIOXUS_PUBLIC_PATH` set
    /// so the Q1-01 bundle check does not need a `public\` folder to exist
    /// next to whatever the test binary happens to be.
    #[test]
    fn install_with_forwards_the_real_running_executable() {
        let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = scratch_dir("install-current-exe");
        std::fs::create_dir_all(dir.join("assets")).expect("scratch assets dir");
        std::fs::write(dir.join("assets/app.wasm"), b"\0asm").expect("scratch wasm file");
        std::env::set_var("DIOXUS_PUBLIC_PATH", &dir);

        let host = MockServiceHost::not_installed();
        let runner = RecordingCommandRunner::default();
        install_with(&host, &runner).expect("install succeeds against the mock");

        std::env::remove_var("DIOXUS_PUBLIC_PATH");
        let created_with = host.created_with.lock().unwrap().clone();
        assert_eq!(
            created_with,
            Some(std::env::current_exe().expect("current_exe")),
            "install must point the service at the running executable, not a hard-coded path"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_refuses_when_no_wasm_bundle_is_present_beside_the_executable() {
        // Boss (QA round 2 close): hold `ENV_LOCK` — the sibling test above
        // sets `DIOXUS_PUBLIC_PATH` to a directory that *has* a wasm file, and
        // without the lock this test could observe it and see `install`
        // succeed (seen once on `main`, 2026-08-30).
        let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = scratch_dir("install-no-bundle");
        // Deliberately no `public\assets\*.wasm` under `dir`.
        let exe_path = dir.join("family-hub.exe");
        let host = MockServiceHost::not_installed();
        let runner = RecordingCommandRunner::default();

        let err = install_with_exe(&host, &runner, exe_path)
            .expect_err("install must refuse without a wasm client bundle");
        assert!(
            matches!(&err, ServiceError::Io(e) if e.kind() == io::ErrorKind::NotFound),
            "expected a NotFound I/O error, got {err:?}"
        );
        assert!(
            !*host.installed.lock().unwrap(),
            "install must not register the service when it refuses to install"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_succeeds_once_the_wasm_bundle_is_present_beside_the_executable() {
        let dir = scratch_dir("install-with-bundle");
        let exe_path = bundled_exe_path(&dir);
        let host = MockServiceHost::not_installed();
        let runner = RecordingCommandRunner::default();

        install_with_exe(&host, &runner, exe_path).expect("install succeeds with a bundle present");
        assert!(*host.installed.lock().unwrap());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_configures_three_firewall_rules_and_the_power_plan() {
        let dir = scratch_dir("install-firewall");
        let exe_path = bundled_exe_path(&dir);
        let host = MockServiceHost::not_installed();
        let runner = RecordingCommandRunner::default();

        install_with_exe(&host, &runner, exe_path).expect("install succeeds");

        let calls = runner.calls.lock().unwrap();
        let netsh_calls: Vec<_> = calls.iter().filter(|(p, _)| p == "netsh.exe").collect();
        assert_eq!(
            netsh_calls.len(),
            3,
            "expected 3 netsh.exe firewall rules, got {netsh_calls:?}"
        );
        for (_, args) in &netsh_calls {
            assert!(args.iter().any(|a| a.starts_with("name=FamilyHub")));
        }
        assert!(netsh_calls
            .iter()
            .any(|(_, args)| args.contains(&"protocol=TCP".to_string())
                && args.contains(&"localport=8080".to_string())));
        assert!(netsh_calls
            .iter()
            .any(|(_, args)| args.contains(&"protocol=TCP".to_string())
                && args.contains(&"localport=8443".to_string())));
        assert!(netsh_calls
            .iter()
            .any(|(_, args)| args.contains(&"protocol=UDP".to_string())
                && args.contains(&"localport=5353".to_string())));

        let powercfg_calls = calls.iter().filter(|(p, _)| p == "powercfg.exe").count();
        assert!(
            powercfg_calls >= 2,
            "expected at least standby + hibernate powercfg calls"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_still_reports_success_when_the_firewall_and_power_commands_fail() {
        // A hub that cannot tighten the firewall/power plan (no elevation)
        // must still finish installing (docs/HANDOFF.md H-12 precedent).
        let dir = scratch_dir("install-command-failures");
        let exe_path = bundled_exe_path(&dir);
        let host = MockServiceHost::not_installed();
        let runner = RecordingCommandRunner {
            fail: true,
            ..Default::default()
        };

        let summary = install_with_exe(&host, &runner, exe_path).expect("install still succeeds");
        assert!(*host.installed.lock().unwrap());
        assert!(summary.contains("0/3") || summary.contains("firewall rules ok: 0/3"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn uninstall_on_a_fresh_mock_reports_not_installed() {
        let host = MockServiceHost::not_installed();
        let err = uninstall_with(&host).expect_err("nothing installed yet");
        assert!(matches!(err, ServiceError::NotInstalled));
    }

    #[test]
    fn install_then_start_then_status_then_stop_then_uninstall_round_trips() {
        let dir = scratch_dir("install-round-trip");
        let exe_path = bundled_exe_path(&dir);
        let host = MockServiceHost::not_installed();
        let runner = RecordingCommandRunner::default();

        install_with_exe(&host, &runner, exe_path).expect("install");
        assert_eq!(
            host.query_state().expect("installed"),
            ServiceRunState::Stopped
        );

        start_with(&host).expect("start");
        assert_eq!(
            host.query_state().expect("installed"),
            ServiceRunState::Running
        );
        assert_eq!(status_with(&host), 0);

        stop_with(&host).expect("stop");
        assert_eq!(
            host.query_state().expect("installed"),
            ServiceRunState::Stopped
        );

        uninstall_with(&host).expect("uninstall");
        assert!(matches!(
            host.query_state().unwrap_err(),
            ServiceError::NotInstalled
        ));
        assert_eq!(status_with(&host), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn start_on_an_uninstalled_mock_is_a_clean_error_not_a_panic() {
        let host = MockServiceHost::not_installed();
        let err = start_with(&host).expect_err("not installed");
        assert!(matches!(err, ServiceError::NotInstalled));
    }

    // -----------------------------------------------------------------
    // ServiceLogger — file write, rotation reuse, and the "startup failure
    // logged within 5s" acceptance (PURPLE §P3 T3.1(c)/(e)).
    // -----------------------------------------------------------------

    #[test]
    fn service_logger_writes_events_to_the_log_file() {
        let dir = scratch_dir("logger-write");
        let logger = Arc::new(ServiceLogger::open(&dir).expect("open logger"));

        tracing::subscriber::with_default(logger.clone(), || {
            tracing::info!("hello from the unit test");
        });
        // Q1-05: an INFO line is buffered, not flushed on every write (that
        // hot-path flush is exactly what turned 543 TRACE lines into 293 KB
        // in three idle minutes) — force it to disk explicitly rather than
        // relying on the 64-line batch threshold.
        logger.flush();

        let contents = std::fs::read_to_string(logger.log_path()).expect("log file readable");
        assert!(
            contents.contains("hello from the unit test"),
            "log file did not contain the expected line: {contents:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // PURPLE §P3 T3.1(c) ("a deliberate startup failure appears in the log
    // file within 5 s") moved to `tests/service_tests.rs::
    // a_startup_bind_failure_is_logged_within_five_seconds` (Q1-04): it now
    // exercises the real `family-hub.exe run` startup path — a genuine bind
    // failure propagated through `router::RunError` — rather than a
    // hand-written `tracing::error!` call standing in for one.

    // -----------------------------------------------------------------
    // Q1-05: log level. §P5.5 default 33 — `info` in the service, `debug`
    // behind `FAMILY_HUB_LOG` — was not implemented: `enabled()` returned
    // `true` unconditionally, so every `dioxus_core`/`hyper`/`sqlx` TRACE
    // event was formatted and flushed on the request hot path.
    // -----------------------------------------------------------------

    #[test]
    fn default_log_level_drops_debug_and_trace_but_keeps_info_and_above() {
        let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("FAMILY_HUB_LOG");
        let dir = scratch_dir("log-level-default");
        let logger = Arc::new(ServiceLogger::open(&dir).expect("open logger"));
        assert_eq!(logger.max_level, tracing::Level::INFO);

        tracing::subscriber::with_default(logger.clone(), || {
            tracing::trace!("q1-05 trace line, must be dropped by default");
            tracing::debug!("q1-05 debug line, must be dropped by default");
            tracing::info!("q1-05 info line, must be kept by default");
            tracing::warn!("q1-05 warn line, must be kept by default");
        });
        logger.flush();

        let contents = std::fs::read_to_string(logger.log_path()).expect("log file readable");
        assert!(!contents.contains("q1-05 trace line"), "{contents:?}");
        assert!(!contents.contains("q1-05 debug line"), "{contents:?}");
        assert!(contents.contains("q1-05 info line"), "{contents:?}");
        assert!(contents.contains("q1-05 warn line"), "{contents:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn family_hub_log_env_var_raises_the_level_to_debug() {
        let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("FAMILY_HUB_LOG", "debug");
        let dir = scratch_dir("log-level-debug-env");
        let logger = Arc::new(ServiceLogger::open(&dir).expect("open logger"));
        std::env::remove_var("FAMILY_HUB_LOG");
        assert_eq!(logger.max_level, tracing::Level::DEBUG);

        tracing::subscriber::with_default(logger.clone(), || {
            tracing::trace!("q1-05 trace line, still dropped under FAMILY_HUB_LOG=debug");
            tracing::debug!("q1-05 debug line, kept under FAMILY_HUB_LOG=debug");
        });
        logger.flush();

        let contents = std::fs::read_to_string(logger.log_path()).expect("log file readable");
        assert!(!contents.contains("q1-05 trace line"), "{contents:?}");
        assert!(contents.contains("q1-05 debug line"), "{contents:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn warn_and_error_events_flush_immediately_without_an_explicit_flush_call() {
        let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("FAMILY_HUB_LOG");
        let dir = scratch_dir("log-level-warn-flush");
        let logger = Arc::new(ServiceLogger::open(&dir).expect("open logger"));

        tracing::subscriber::with_default(logger.clone(), || {
            tracing::error!("q1-05 error line flushes on its own");
        });
        // No `logger.flush()` here: a WARN/ERROR event must already be on
        // disk (this is what lets `run_console`/`scm::run_service` log a
        // startup failure and then `std::process::exit`/return without
        // losing it — `Drop` never runs on `std::process::exit`).
        let contents = std::fs::read_to_string(logger.log_path()).expect("log file readable");
        assert!(
            contents.contains("q1-05 error line flushes on its own"),
            "{contents:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// PURPLE §P3 T3.1(e): 20 MB of log lines produces >= 2 rotated files,
    /// the newest under the cap. Reuses `backup::rotate_log_if_needed`
    /// directly (T1.6 already owns and tests that function's own unit
    /// behaviour) — this test proves `ServiceLogger` actually calls it as
    /// it writes, not just that the function itself works.
    #[test]
    fn writing_twenty_megabytes_of_log_lines_rotates_under_the_cap() {
        let dir = scratch_dir("logger-rotate");
        let logger = ServiceLogger::open(&dir).expect("open logger");

        // A ~200-byte line, ~110k writes ≈ 20 MB — well past the 10 MB cap,
        // enough to force multiple rotations while staying fast in a test.
        let line = "x".repeat(190);
        let target_bytes: u64 = 20 * 1024 * 1024;
        let mut written: u64 = 0;
        tracing::subscriber::with_default(Arc::new(logger), || {
            while written < target_bytes {
                tracing::info!(payload = %line, "rotation stress line");
                written += 200;
            }
        });

        let log_path = dir.join(LOG_FILE_NAME);
        let active_size = std::fs::metadata(&log_path)
            .expect("active log exists")
            .len();
        assert!(
            active_size < backup::LOG_ROTATION_MAX_BYTES,
            "active log file ({active_size} bytes) should be under the {} byte cap after rotation",
            backup::LOG_ROTATION_MAX_BYTES
        );

        let rotated_1 = log_path.with_extension("log.1");
        let mut rotated_count = 0;
        for generation in 1..=backup::LOG_ROTATION_MAX_FILES {
            let mut name = log_path.as_os_str().to_os_string();
            name.push(format!(".{generation}"));
            if PathBuf::from(name).exists() {
                rotated_count += 1;
            }
        }
        assert!(
            rotated_count >= 2,
            "expected at least 2 rotated log files, found {rotated_count} (checked around {})",
            rotated_1.display()
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // -----------------------------------------------------------------
    // configure_firewall / configure_power_plan in isolation.
    // -----------------------------------------------------------------

    #[test]
    fn configure_firewall_names_every_rule_family_hub_prefixed() {
        let runner = RecordingCommandRunner::default();
        let outcomes = configure_firewall(&runner);
        assert_eq!(outcomes.len(), 3);
        for outcome in &outcomes {
            assert!(outcome.succeeded);
            assert!(outcome.args.iter().any(|a| a.starts_with("name=FamilyHub")));
        }
    }

    #[test]
    fn configure_power_plan_disables_standby_and_hibernate() {
        let runner = RecordingCommandRunner::default();
        let outcomes = configure_power_plan(&runner);
        assert!(outcomes
            .iter()
            .any(|o| o.args.contains(&"standby-timeout-ac".to_string())));
        assert!(outcomes
            .iter()
            .any(|o| o.args.contains(&"hibernate-timeout-ac".to_string())));
        for outcome in &outcomes {
            assert!(outcome.args.contains(&"0".to_string()));
        }
    }

    // -----------------------------------------------------------------
    // tv-probe
    // -----------------------------------------------------------------

    #[test]
    fn tv_probe_reports_unreachable_rather_than_erroring_when_adb_fails() {
        let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let runner = RecordingCommandRunner {
            fail: true,
            ..Default::default()
        };
        std::env::set_var("FAMILY_HUB_TV_IP", "10.0.0.178");
        let result = tv_probe_with(&runner).expect("tv-probe never hard-errors");
        std::env::remove_var("FAMILY_HUB_TV_IP");
        assert!(result.contains("unreachable") || result.contains("failed"));
    }

    #[test]
    fn tv_probe_connects_when_adb_succeeds() {
        let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let runner = RecordingCommandRunner::default();
        std::env::set_var("FAMILY_HUB_TV_IP", "10.0.0.178");
        let result = tv_probe_with(&runner).expect("tv-probe succeeds");
        std::env::remove_var("FAMILY_HUB_TV_IP");
        assert!(result.contains("10.0.0.178"));
        let calls = runner.calls.lock().unwrap();
        assert!(
            calls
                .iter()
                .any(|(p, args)| p == "adb.exe"
                    && args.first().map(String::as_str) == Some("connect"))
        );
    }

    #[test]
    fn tv_probe_without_any_configured_ip_says_so_rather_than_panicking() {
        let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("FAMILY_HUB_TV_IP");
        // No docs/device.toml relative to CARGO_MANIFEST_DIR-independent
        // cwd in a test binary is not guaranteed either way, so only assert
        // this never panics and always returns a String.
        let runner = RecordingCommandRunner::default();
        let _ = tv_probe_with(&runner);
    }

    // -----------------------------------------------------------------
    // dispatch() — unknown / empty argv never panics.
    // -----------------------------------------------------------------

    #[test]
    fn dispatch_on_an_unknown_subcommand_returns_a_nonzero_exit_code_not_a_panic() {
        assert_eq!(dispatch(&["frobnicate".to_string()]), 2);
        assert_eq!(dispatch(&[]), 2);
    }
}
