//! `FamilyHubConfig`: the single source of truth for every absolute path and
//! bind address the server uses (PLAN v2 D9, task T0.5).
//!
//! Resolution order, env always wins over the file:
//!   1. `FAMILY_HUB_DATA_DIR` / `FAMILY_HUB_ADDR` / `FAMILY_HUB_TLS_ADDR`
//!      environment variables.
//!   2. The matching key in `familyhub.toml`, looked up next to the running
//!      executable and then in the current directory.
//!   3. Hard-coded defaults: `%ProgramData%\FamilyHub`, `0.0.0.0:8080`,
//!      `0.0.0.0:8443`.
//!
//! Every other module asks this type for a path (`db_path`, `upload_dir`,
//! `screensaver_dir`, `pki_dir`, `log_dir`) instead of hard-coding one, so
//! nothing is ever resolved relative to the process's current working
//! directory (G23 / R-14) — important once T3.1 runs this as a Windows
//! service, whose CWD is `C:\Windows\System32`.
//!
//! `familyhub.toml` only needs a handful of flat keys today. Rather than pull
//! in a full TOML dependency ahead of when one is actually needed (Cargo.toml
//! is owned by T0.2/T0.4; see `docs/reviews/PURPLE_TEAM.md` §P4), this module
//! ships a minimal parser for `key = "value"` / `key = value` lines with `#`
//! comments and `[section]` headers (sections are namespaced as
//! `section.key` for T1.3's future `[certs]` / T1.8's `[acme]` blocks, but
//! T0.5 itself only reads top-level keys). If a later task needs real nested
//! tables, request the `toml` crate via `docs/HANDOFF.md`.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::PathBuf;

/// Default HTTP bind address for the TV-facing kiosk origin (PLAN v2 D3′).
pub const DEFAULT_HTTP_ADDR: &str = "0.0.0.0:8080";
/// Default HTTPS bind address for the phone-facing PWA origin (PLAN v2 D3′).
pub const DEFAULT_TLS_ADDR: &str = "0.0.0.0:8443";

const ENV_DATA_DIR: &str = "FAMILY_HUB_DATA_DIR";
const ENV_HTTP_ADDR: &str = "FAMILY_HUB_ADDR";
const ENV_TLS_ADDR: &str = "FAMILY_HUB_TLS_ADDR";
/// QA round 1, Q1-14: the optional scheduled-screensaver hour. Unset (the
/// default) leaves `ScreensaverSchedule::default()` — disabled — the only
/// schedule the process can ever build, which was the bug: there was no
/// enable path at all. Setting this is that path.
const ENV_SCREENSAVER_HOUR: &str = "FAMILY_HUB_SCREENSAVER_HOUR";
/// QA round 2, Q2-05: the log level. A service started by the SCM inherits
/// the machine's environment, not the owner's shell, so `FAMILY_HUB_LOG` set
/// in a PowerShell prompt before `install`/`start` never reaches it — the
/// runbooks described a control that did nothing on the deployed path.
/// `[log] level` in `familyhub.toml` is the seam that actually reaches an
/// installed service; the env var still wins when both are present, so a
/// developer's `run` shell keeps working exactly as before.
const ENV_LOG_LEVEL: &str = "FAMILY_HUB_LOG";
/// HS1 (§2 H5, review finding R-4/P-1): where the boot-time curriculum loader
/// looks for `*.toml` files. Deliberately **not** a field on
/// [`FamilyHubConfig`] — it is derived from `data_dir` like every other path
/// this type hands out, and only an owner who has moved the files somewhere
/// else ever sets it. A relative value is resolved under `data_dir` rather
/// than the process's CWD (G23/R-14: a Windows service's CWD is
/// `C:\Windows\System32`), so [`FamilyHubConfig::curricula_dir`] is absolute
/// whatever the environment says.
const ENV_CURRICULA_DIR: &str = "FAMILY_HUB_CURRICULA_DIR";

const CONFIG_FILE_NAME: &str = "familyhub.toml";

/// Fully resolved server configuration. Every path handed out by this type
/// is absolute, rooted at [`FamilyHubConfig::data_dir`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyHubConfig {
    pub data_dir: PathBuf,
    pub http_addr: SocketAddr,
    pub tls_addr: SocketAddr,
    /// QA round 1, Q1-14: local hour (`0..=23`) the ambient screensaver is
    /// forced on at, from `FAMILY_HUB_SCREENSAVER_HOUR` or `[screensaver]
    /// schedule_hour` in `familyhub.toml`. `None` (the default) means the
    /// schedule stays off, per PURPLE §P5.5 default 20 — a family that never
    /// sets this sees exactly today's idle-only behaviour, forever.
    pub screensaver_schedule_hour: Option<u32>,
    /// QA round 2, Q2-05: `FAMILY_HUB_LOG`, else `[log] level` in
    /// `familyhub.toml`, else `None` (§P5.5 default 33's `info`). Passed to
    /// [`crate::server::service::level_from`] by
    /// `service::install_global_logger` — this field only resolves *which*
    /// string won; `level_from` still owns the string → `tracing::Level`
    /// mapping and its default.
    pub log_level: Option<String>,
}

impl FamilyHubConfig {
    /// Resolve configuration from `familyhub.toml` (if one is found) and the
    /// process environment, environment variables taking precedence.
    pub fn load() -> Self {
        let file = TomlValues::load_nearby(CONFIG_FILE_NAME);
        Self::from_sources(&file, &ProcessEnv)
    }

    fn from_sources(file: &TomlValues, env: &impl EnvLookup) -> Self {
        let data_dir = env
            .var(ENV_DATA_DIR)
            .or_else(|| file.get("data_dir"))
            .map(PathBuf::from)
            .unwrap_or_else(default_data_dir);

        let http_addr = resolve_addr(env, file, ENV_HTTP_ADDR, "http_addr", DEFAULT_HTTP_ADDR);
        let tls_addr = resolve_addr(env, file, ENV_TLS_ADDR, "tls_addr", DEFAULT_TLS_ADDR);
        let screensaver_schedule_hour = resolve_screensaver_hour(env, file);
        let log_level = env.var(ENV_LOG_LEVEL).or_else(|| file.get("log.level"));

        Self {
            data_dir,
            http_addr,
            tls_addr,
            screensaver_schedule_hour,
            log_level,
        }
    }

    /// Absolute path to the SQLite database file.
    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("family.db")
    }

    /// `sqlx` connection URL for [`Self::db_path`].
    pub fn database_url(&self) -> String {
        // sqlx's sqlite URL parser wants forward slashes even on Windows.
        let path = self.db_path().display().to_string().replace('\\', "/");
        format!("sqlite://{path}")
    }

    /// Absolute directory photo-task uploads are written to.
    pub fn upload_dir(&self) -> PathBuf {
        self.data_dir.join("uploads")
    }

    /// Absolute directory the ambient screensaver reads images from.
    pub fn screensaver_dir(&self) -> PathBuf {
        self.data_dir.join("screensaver")
    }

    /// Absolute directory the local CA and leaf certificate live in (T1.3).
    pub fn pki_dir(&self) -> PathBuf {
        self.data_dir.join("pki")
    }

    /// Absolute directory rotated log files are written to (T3.1).
    pub fn log_dir(&self) -> PathBuf {
        self.data_dir.join("logs")
    }

    /// Absolute directory the boot-time curriculum loader scans for `*.toml`
    /// files (HS1, §2 H5). `<data>\curricula` unless
    /// `FAMILY_HUB_CURRICULA_DIR` names somewhere else.
    ///
    /// A *method*, not a field: HS1's review (R-4/P-1) asked for exactly this
    /// so the one owner-facing knob does not widen the struct every other
    /// module constructs. The loader creates the directory if it is missing
    /// and logs the resolved path at `info`.
    pub fn curricula_dir(&self) -> PathBuf {
        self.curricula_dir_from(std::env::var(ENV_CURRICULA_DIR).ok())
    }

    /// [`Self::curricula_dir`] with the environment value injected, so the
    /// precedence and the "always absolute" guarantee are unit testable
    /// without mutating process-wide state that another test in this binary
    /// is reading at the same moment.
    fn curricula_dir_from(&self, raw: Option<String>) -> PathBuf {
        match raw.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(value) => {
                let candidate = PathBuf::from(value);
                if candidate.is_absolute() {
                    candidate
                } else {
                    // Never resolve a server path against the CWD (G23/R-14).
                    self.data_dir.join(candidate)
                }
            }
            None => self.data_dir.join("curricula"),
        }
    }

    /// Create every directory this config resolves to (idempotent) and log
    /// each absolute path once, exactly as D9 requires ("all paths ...
    /// logged at startup").
    pub fn ensure_dirs_and_log(&self) -> std::io::Result<()> {
        for dir in [
            &self.data_dir,
            &self.upload_dir(),
            &self.screensaver_dir(),
            &self.pki_dir(),
            &self.log_dir(),
        ] {
            std::fs::create_dir_all(dir)?;
        }

        tracing::info!(data_dir = %self.data_dir.display(), "resolved data directory");
        tracing::info!(db_path = %self.db_path().display(), "resolved database path");
        tracing::info!(upload_dir = %self.upload_dir().display(), "resolved upload directory");
        tracing::info!(screensaver_dir = %self.screensaver_dir().display(), "resolved screensaver directory");
        tracing::info!(pki_dir = %self.pki_dir().display(), "resolved PKI directory");
        tracing::info!(log_dir = %self.log_dir().display(), "resolved log directory");
        tracing::info!(http_addr = %self.http_addr, "resolved HTTP bind address");
        tracing::info!(tls_addr = %self.tls_addr, "resolved HTTPS bind address");
        match self.screensaver_schedule_hour {
            Some(hour) => {
                tracing::info!(schedule_hour = hour, "scheduled screensaver enabled")
            }
            None => tracing::info!("scheduled screensaver disabled (no schedule_hour configured)"),
        }
        match &self.log_level {
            Some(level) => tracing::info!(log_level = %level, "resolved log level"),
            None => tracing::info!(
                "resolved log level: info (no FAMILY_HUB_LOG / [log] level configured)"
            ),
        }

        Ok(())
    }
}

fn resolve_addr(
    env: &impl EnvLookup,
    file: &TomlValues,
    env_key: &str,
    file_key: &str,
    default: &str,
) -> SocketAddr {
    let raw = env
        .var(env_key)
        .or_else(|| file.get(file_key))
        .unwrap_or_else(|| default.to_string());

    raw.parse().unwrap_or_else(|err| {
        panic!("{env_key} / familyhub.toml [{file_key}] is not a valid host:port address ({raw:?}): {err}")
    })
}

/// Resolve the optional scheduled-screensaver hour (QA round 1, Q1-14):
/// `None` unless an owner explicitly opts in via `FAMILY_HUB_SCREENSAVER_HOUR`
/// or `[screensaver] schedule_hour` in `familyhub.toml`, env taking
/// precedence over the file exactly like every other key in this module. A
/// value that does not parse as `0..=23` panics at startup rather than being
/// silently clamped or ignored — the same fail-loud policy [`resolve_addr`]
/// uses for a bad bind address.
fn resolve_screensaver_hour(env: &impl EnvLookup, file: &TomlValues) -> Option<u32> {
    let raw = env
        .var(ENV_SCREENSAVER_HOUR)
        .or_else(|| file.get("screensaver.schedule_hour"))?;

    let hour: u32 = raw.parse().unwrap_or_else(|err| {
        panic!(
            "{ENV_SCREENSAVER_HOUR} / familyhub.toml [screensaver] schedule_hour is not a valid hour ({raw:?}): {err}"
        )
    });
    assert!(
        hour <= 23,
        "{ENV_SCREENSAVER_HOUR} / familyhub.toml [screensaver] schedule_hour must be 0..=23, got {hour}"
    );
    Some(hour)
}

/// `%ProgramData%\FamilyHub` on Windows (falling back to `C:\ProgramData` if
/// the environment variable itself is somehow unset — Windows always sets
/// it, but a test harness or a stripped-down service account might not),
/// `/var/lib/familyhub` everywhere else so the crate still builds and has a
/// sane default off Windows (e.g. for `cargo check` in CI containers).
fn default_data_dir() -> PathBuf {
    #[cfg(windows)]
    {
        let program_data =
            std::env::var("ProgramData").unwrap_or_else(|_| r"C:\ProgramData".to_string());
        PathBuf::from(program_data).join("FamilyHub")
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/var/lib/familyhub")
    }
}

/// Indirection over `std::env::var` so the precedence logic above can be unit
/// tested without mutating real process-wide environment variables.
trait EnvLookup {
    fn var(&self, key: &str) -> Option<String>;
}

struct ProcessEnv;

impl EnvLookup for ProcessEnv {
    fn var(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
}

/// A minimal flat/sectioned `key = value` store parsed out of
/// `familyhub.toml`. See the module doc comment for the supported subset.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct TomlValues(BTreeMap<String, String>);

impl TomlValues {
    fn get(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }

    /// Look for `file_name` next to the running executable, then in the
    /// current directory. Missing or unreadable is not an error: the file is
    /// entirely optional and every key falls back to env/defaults.
    fn load_nearby(file_name: &str) -> Self {
        let mut candidates = Vec::new();
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                candidates.push(dir.join(file_name));
            }
        }
        candidates.push(PathBuf::from(file_name));

        for candidate in candidates {
            if let Ok(contents) = std::fs::read_to_string(&candidate) {
                return Self::parse(&contents);
            }
        }

        Self::default()
    }

    fn parse(source: &str) -> Self {
        let mut values = BTreeMap::new();
        let mut section = String::new();

        for raw_line in source.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some(name) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                section = name.trim().to_string();
                continue;
            }

            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim().trim_matches('"').trim_matches('\'');

            let qualified = if section.is_empty() {
                key.to_string()
            } else {
                format!("{section}.{key}")
            };
            values.insert(qualified, value.to_string());
        }

        Self(values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    struct FakeEnv(BTreeMap<&'static str, &'static str>);

    impl EnvLookup for FakeEnv {
        fn var(&self, key: &str) -> Option<String> {
            self.0.get(key).map(|value| value.to_string())
        }
    }

    fn empty_env() -> FakeEnv {
        FakeEnv(BTreeMap::new())
    }

    #[test]
    fn default_http_addr_is_zero_zero_zero_zero_eight_zero_eight_zero() {
        let config = FamilyHubConfig::from_sources(&TomlValues::default(), &empty_env());
        assert_eq!(config.http_addr, "0.0.0.0:8080".parse().unwrap());
        assert_eq!(config.tls_addr, "0.0.0.0:8443".parse().unwrap());
    }

    #[test]
    fn env_data_dir_overrides_file_and_default() {
        let file = TomlValues::parse("data_dir = \"C:/from-file\"\n");
        let env = FakeEnv(BTreeMap::from([(ENV_DATA_DIR, "C:/from-env")]));

        let config = FamilyHubConfig::from_sources(&file, &env);
        assert_eq!(config.data_dir, PathBuf::from("C:/from-env"));
    }

    #[test]
    fn file_data_dir_is_used_when_env_is_unset() {
        let file = TomlValues::parse("data_dir = \"C:/from-file\"\n");
        let config = FamilyHubConfig::from_sources(&file, &empty_env());
        assert_eq!(config.data_dir, PathBuf::from("C:/from-file"));
    }

    #[test]
    fn env_addr_overrides_default() {
        let env = FakeEnv(BTreeMap::from([
            (ENV_HTTP_ADDR, "127.0.0.1:9000"),
            (ENV_TLS_ADDR, "127.0.0.1:9443"),
        ]));
        let config = FamilyHubConfig::from_sources(&TomlValues::default(), &env);
        assert_eq!(config.http_addr, "127.0.0.1:9000".parse().unwrap());
        assert_eq!(config.tls_addr, "127.0.0.1:9443".parse().unwrap());
    }

    // QA round 1, Q1-14: the scheduled screensaver had no enable path
    // because `ScreensaverSchedule::default()` was the only instance ever
    // constructed. These four tests cover both configuration sources for
    // the hour that now builds a non-default schedule.

    #[test]
    fn screensaver_schedule_hour_defaults_to_none() {
        let config = FamilyHubConfig::from_sources(&TomlValues::default(), &empty_env());
        assert_eq!(config.screensaver_schedule_hour, None);
    }

    #[test]
    fn env_screensaver_schedule_hour_overrides_file_and_default() {
        let file = TomlValues::parse("[screensaver]\nschedule_hour = 5\n");
        let env = FakeEnv(BTreeMap::from([(ENV_SCREENSAVER_HOUR, "20")]));

        let config = FamilyHubConfig::from_sources(&file, &env);
        assert_eq!(config.screensaver_schedule_hour, Some(20));
    }

    #[test]
    fn file_screensaver_schedule_hour_is_used_when_env_is_unset() {
        let file = TomlValues::parse("[screensaver]\nschedule_hour = 21\n");
        let config = FamilyHubConfig::from_sources(&file, &empty_env());
        assert_eq!(config.screensaver_schedule_hour, Some(21));
    }

    #[test]
    #[should_panic(expected = "must be 0..=23")]
    fn out_of_range_screensaver_schedule_hour_panics_at_startup() {
        let env = FakeEnv(BTreeMap::from([(ENV_SCREENSAVER_HOUR, "24")]));
        let _ = FamilyHubConfig::from_sources(&TomlValues::default(), &env);
    }

    // QA round 2, Q2-05: `FAMILY_HUB_LOG` set in the owner's shell never
    // reaches a service started by the SCM (it inherits the machine
    // environment, not that shell's). `[log] level` in `familyhub.toml` is
    // the seam that does reach it; env still wins when both are set, exactly
    // like every other key this module resolves.

    #[test]
    fn log_level_defaults_to_none() {
        let config = FamilyHubConfig::from_sources(&TomlValues::default(), &empty_env());
        assert_eq!(config.log_level, None);
    }

    #[test]
    fn env_log_level_overrides_file_and_default() {
        let file = TomlValues::parse("[log]\nlevel = \"warn\"\n");
        let env = FakeEnv(BTreeMap::from([(ENV_LOG_LEVEL, "debug")]));

        let config = FamilyHubConfig::from_sources(&file, &env);
        assert_eq!(config.log_level.as_deref(), Some("debug"));
    }

    #[test]
    fn file_log_level_is_used_when_env_is_unset() {
        let file = TomlValues::parse("[log]\nlevel = \"debug\"\n");
        let config = FamilyHubConfig::from_sources(&file, &empty_env());
        assert_eq!(config.log_level.as_deref(), Some("debug"));
    }

    #[test]
    fn every_path_is_absolute_under_data_dir() {
        let data_dir = PathBuf::from("C:/temp/familyhub-unit-test");
        let config = FamilyHubConfig {
            data_dir: data_dir.clone(),
            http_addr: DEFAULT_HTTP_ADDR.parse().unwrap(),
            tls_addr: DEFAULT_TLS_ADDR.parse().unwrap(),
            screensaver_schedule_hour: None,
            log_level: None,
        };

        assert_eq!(config.db_path(), data_dir.join("family.db"));
        assert_eq!(config.upload_dir(), data_dir.join("uploads"));
        assert_eq!(config.screensaver_dir(), data_dir.join("screensaver"));
        assert_eq!(config.pki_dir(), data_dir.join("pki"));
        assert_eq!(config.log_dir(), data_dir.join("logs"));
        assert_eq!(config.curricula_dir_from(None), data_dir.join("curricula"));
        assert!(config.database_url().starts_with("sqlite://"));
        assert!(
            config.database_url().contains("family.db"),
            "database_url should embed the resolved absolute db path: {}",
            config.database_url()
        );
    }

    // HS1 (§2 H5): `curricula_dir()` is the one path the homeschool loader
    // resolves, and it must be absolute however the environment is set —
    // nothing on the server may ever be resolved against the CWD (G23/R-14).

    #[test]
    fn curricula_dir_defaults_to_curricula_under_the_data_dir() {
        let config = FamilyHubConfig {
            data_dir: PathBuf::from("C:/temp/familyhub-curricula-default"),
            http_addr: DEFAULT_HTTP_ADDR.parse().unwrap(),
            tls_addr: DEFAULT_TLS_ADDR.parse().unwrap(),
            screensaver_schedule_hour: None,
            log_level: None,
        };

        let dir = config.curricula_dir_from(None);
        assert_eq!(dir, config.data_dir.join("curricula"));
        assert!(dir.is_absolute(), "{} must be absolute", dir.display());
    }

    #[test]
    fn an_absolute_curricula_dir_environment_value_wins_outright() {
        let config = FamilyHubConfig {
            data_dir: PathBuf::from("C:/temp/familyhub-curricula-env"),
            http_addr: DEFAULT_HTTP_ADDR.parse().unwrap(),
            tls_addr: DEFAULT_TLS_ADDR.parse().unwrap(),
            screensaver_schedule_hour: None,
            log_level: None,
        };

        let dir = config.curricula_dir_from(Some("C:/school/files".to_string()));
        assert_eq!(dir, PathBuf::from("C:/school/files"));
        assert!(dir.is_absolute());
    }

    #[test]
    fn a_relative_curricula_dir_environment_value_lands_under_the_data_dir() {
        let config = FamilyHubConfig {
            data_dir: PathBuf::from("C:/temp/familyhub-curricula-relative"),
            http_addr: DEFAULT_HTTP_ADDR.parse().unwrap(),
            tls_addr: DEFAULT_TLS_ADDR.parse().unwrap(),
            screensaver_schedule_hour: None,
            log_level: None,
        };

        let dir = config.curricula_dir_from(Some("school".to_string()));
        assert_eq!(dir, config.data_dir.join("school"));
        assert!(
            dir.is_absolute(),
            "a relative FAMILY_HUB_CURRICULA_DIR must never leave a CWD-relative path: {}",
            dir.display()
        );

        // An empty or whitespace-only value is "unset", not "the data dir".
        assert_eq!(
            config.curricula_dir_from(Some("   ".to_string())),
            config.data_dir.join("curricula")
        );
    }

    #[test]
    fn toml_parser_reads_flat_and_sectioned_keys() {
        let file = TomlValues::parse(
            r#"
            # a comment
            data_dir = "C:/ProgramData/FamilyHub"
            http_addr = "0.0.0.0:8080"

            [certs]
            mode = "self_signed"
            "#,
        );

        assert_eq!(
            file.get("data_dir"),
            Some("C:/ProgramData/FamilyHub".to_string())
        );
        assert_eq!(file.get("http_addr"), Some("0.0.0.0:8080".to_string()));
        assert_eq!(file.get("certs.mode"), Some("self_signed".to_string()));
        assert_eq!(file.get("missing"), None);
    }

    /// A hand-rolled `tracing::Subscriber` that only counts emitted events,
    /// avoiding a dependency on `tracing-subscriber` (not in `Cargo.toml`,
    /// which T0.5 does not own) just to prove startup logging really fires.
    struct CountingSubscriber(Arc<AtomicUsize>);

    impl tracing::Subscriber for CountingSubscriber {
        fn enabled(&self, _metadata: &tracing::Metadata<'_>) -> bool {
            true
        }
        fn new_span(&self, _span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
            tracing::span::Id::from_u64(1)
        }
        fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}
        fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}
        fn event(&self, _event: &tracing::Event<'_>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
        fn enter(&self, _span: &tracing::span::Id) {}
        fn exit(&self, _span: &tracing::span::Id) {}
    }

    #[test]
    fn ensure_dirs_and_log_creates_every_directory_and_logs_each_path() {
        let data_dir =
            std::env::temp_dir().join(format!("familyhub-config-log-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data_dir);

        let config = FamilyHubConfig {
            data_dir: data_dir.clone(),
            http_addr: DEFAULT_HTTP_ADDR.parse().unwrap(),
            tls_addr: DEFAULT_TLS_ADDR.parse().unwrap(),
            screensaver_schedule_hour: None,
            log_level: None,
        };

        let counter = Arc::new(AtomicUsize::new(0));
        let subscriber = CountingSubscriber(counter.clone());
        tracing::subscriber::with_default(subscriber, || {
            config
                .ensure_dirs_and_log()
                .expect("directories under a fresh temp dir are creatable");
        });

        assert!(data_dir.is_dir());
        assert!(config.upload_dir().is_dir());
        assert!(config.screensaver_dir().is_dir());
        assert!(config.pki_dir().is_dir());
        assert!(config.log_dir().is_dir());
        assert!(
            counter.load(Ordering::SeqCst) >= 8,
            "expected one log line per resolved path/address, got {}",
            counter.load(Ordering::SeqCst)
        );

        let _ = std::fs::remove_dir_all(&data_dir);
    }
}
