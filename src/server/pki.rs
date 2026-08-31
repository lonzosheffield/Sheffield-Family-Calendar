//! Local PKI for the phone-facing HTTPS origin (PLAN v2 D3′, task T1.3).
//!
//! The TV loads `http://<ip>:8080/tv` and never sees a certificate at all;
//! only the phones need a secure context (service worker, install prompt,
//! camera capture), and they get it from a **local** CA generated on this
//! machine by [`rcgen`]. Nothing here touches the network, needs a domain,
//! or needs any configuration.
//!
//! Shape of the seam (PURPLE_TEAM.md §P2a): [`CertSource`] selects *where*
//! certificates come from and [`CertProvider`] is what the TLS listener
//! actually talks to. Today there is exactly one implementation,
//! [`SelfSignedCa`]; T1.8 adds `CertSource::AcmeDns01` (`instant-acme`,
//! DNS-01) as a second variant behind the same trait, which is why the TLS
//! listener in [`crate::server::tls`] never names `SelfSignedCa` directly.
//!
//! Validity, per PURPLE_TEAM.md §P5.5 default 7:
//!
//! * CA leaf-signing certificate: **10 years** — installed once per phone,
//!   so a short life would be a recurring chore for the owner.
//! * Server leaf: **397 days**, with `not_before` *and* `not_after` set
//!   explicitly. rcgen's default `not_after` is the year 4096, and Apple
//!   platforms reject any TLS server certificate whose validity exceeds 398
//!   days — leaving the default in place would silently produce a
//!   certificate iOS Safari refuses.
//! * Re-issued when fewer than [`RENEW_WINDOW_DAYS`] remain **or** when the
//!   host's IPv4 addresses have changed (DHCP moved the PC), with the
//!   running listener picking the new leaf up without a restart.

use std::collections::BTreeSet;
use std::io;
use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa,
    Issuer, KeyPair, KeyUsagePurpose, SanType,
};
use time::{Duration, OffsetDateTime};

/// Leaf validity in days. Inside Apple's 398-day cap with a day of slack
/// (PURPLE_TEAM.md §P5.5 row 7).
pub const LEAF_VALIDITY_DAYS: i64 = 397;
/// CA validity in days (10 years; 2 of the next 10 are leap years).
pub const CA_VALIDITY_DAYS: i64 = 3652;
/// Re-issue the leaf once fewer than this many days remain.
pub const RENEW_WINDOW_DAYS: i64 = 30;

/// The `.local` name the mDNS responder claims and the leaf carries as a
/// DNS SAN. Stored **without** the trailing dot; [`crate::server::mdns`]
/// appends one for the FQDN mDNS requires.
pub const MDNS_HOSTNAME: &str = "familyhub.local";

const CA_CERT_FILE: &str = "ca.crt";
const CA_KEY_FILE: &str = "ca.key";
const LEAF_CERT_FILE: &str = "leaf.crt";
const LEAF_KEY_FILE: &str = "leaf.key";

/// Where certificates come from. The seam that makes T1.8's public
/// DNS-01 certificate a `familyhub.toml` change rather than a rewrite
/// (PURPLE_TEAM.md §P2a).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CertSource {
    /// Default, and the only variant that exists in this wave: an `rcgen`
    /// CA plus a short-lived leaf, both stored under `<data>\pki`.
    SelfSignedCa,
}

impl CertSource {
    /// Resolve the configured source. `certs.mode` is read from
    /// `familyhub.toml` by T1.8; until that variant exists, anything other
    /// than the default is rejected loudly at startup rather than silently
    /// falling back (PURPLE_TEAM.md §P3 T1.8 assertion (d)).
    pub fn from_mode(mode: Option<&str>) -> Result<Self, PkiError> {
        match mode {
            None | Some("self_signed") => Ok(Self::SelfSignedCa),
            Some(other) => Err(PkiError::UnknownCertSource(other.to_string())),
        }
    }
}

/// What the TLS listener needs from a certificate source. Deliberately
/// synchronous and object-safe: the listener holds a
/// `Arc<dyn CertProvider>` and the hot-reload path is a lock swap, not an
/// await point on the accept loop.
pub trait CertProvider: Send + Sync + 'static {
    /// PEM of the certificate `/ca.crt` serves — what the owner installs on
    /// each phone.
    fn ca_pem(&self) -> String;

    /// The leaf currently being served: PEM certificate, PEM private key.
    fn current(&self) -> IssuedLeaf;

    /// Re-issue the leaf if it is inside [`RENEW_WINDOW_DAYS`] of expiry or
    /// no longer covers this host's addresses. Returns whether it re-issued.
    fn renew_if_due(&self) -> Result<bool, PkiError>;
}

/// A leaf certificate and its private key, both PEM, plus the validity
/// window parsed back out of the certificate (so `/health` — T1.7 — and the
/// renewal check read the same numbers a client would).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuedLeaf {
    pub cert_pem: String,
    pub key_pem: String,
    pub not_before: OffsetDateTime,
    pub not_after: OffsetDateTime,
    /// Every SAN the leaf carries, rendered as strings (IPs as their
    /// textual form, DNS names verbatim).
    pub sans: BTreeSet<String>,
}

impl IssuedLeaf {
    /// Whole days between now and `not_after`; negative once expired.
    pub fn days_remaining(&self) -> i64 {
        (self.not_after - OffsetDateTime::now_utc()).whole_days()
    }
}

/// Errors the PKI can produce. Kept concrete rather than boxed so the
/// startup path can distinguish "misconfigured" from "disk problem".
#[derive(Debug)]
pub enum PkiError {
    Io(io::Error),
    Rcgen(rcgen::Error),
    /// A stored certificate could not be parsed back.
    Parse(String),
    /// `certs.mode` named a source this build does not have.
    UnknownCertSource(String),
}

impl std::fmt::Display for PkiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "pki i/o error: {err}"),
            Self::Rcgen(err) => write!(f, "certificate generation failed: {err}"),
            Self::Parse(msg) => write!(f, "could not parse a stored certificate: {msg}"),
            Self::UnknownCertSource(mode) => write!(
                f,
                "unknown certs.mode {mode:?}; this build only supports \"self_signed\" \
                 (CertSource::AcmeDns01 lands with T1.8)"
            ),
        }
    }
}

impl std::error::Error for PkiError {}

impl From<io::Error> for PkiError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<rcgen::Error> for PkiError {
    fn from(err: rcgen::Error) -> Self {
        Self::Rcgen(err)
    }
}

/// Every non-loopback IPv4 address of this host, sorted and de-duplicated.
///
/// These are the addresses a phone can actually reach the server on, so
/// every one of them becomes an IP SAN on the leaf — the owner's DHCP
/// reservation (Appendix A A2) picks one, but a phone that reaches the PC
/// on any other interface still gets a valid certificate. Deliberately
/// *unfiltered* beyond loopback and `0.0.0.0`: a Hyper-V or VPN adapter
/// that is down today may be up tomorrow, and a certificate is cheaper to
/// over-cover than to re-issue at the moment a phone needs it.
pub fn host_ipv4_addresses() -> Vec<Ipv4Addr> {
    let mut addrs: BTreeSet<Ipv4Addr> = BTreeSet::new();
    if let Ok(interfaces) = if_addrs::get_if_addrs() {
        for interface in interfaces {
            if interface.is_loopback() {
                continue;
            }
            if let IpAddr::V4(v4) = interface.ip() {
                if v4.is_unspecified() {
                    continue;
                }
                addrs.insert(v4);
            }
        }
    }
    addrs.into_iter().collect()
}

/// The IPv4 address the OS would use to reach the LAN — the one worth
/// printing in the kiosk QR code when several interfaces exist. No packet is
/// sent: `connect` on a UDP socket only fixes the local routing decision.
pub fn primary_ipv4_address() -> Option<Ipv4Addr> {
    let socket = std::net::UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((Ipv4Addr::new(10, 0, 0, 1), 80)).ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(v4) if !v4.is_unspecified() && !v4.is_loopback() => Some(v4),
        _ => host_ipv4_addresses().first().copied(),
    }
}

/// The SAN set every leaf carries: every non-loopback host IPv4, the mDNS
/// name, and the two loopback identities so the server can be reached (and
/// acceptance-tested) on this machine itself.
fn leaf_san_names() -> Vec<SanType> {
    let mut sans: Vec<SanType> = Vec::new();
    for addr in host_ipv4_addresses() {
        sans.push(SanType::IpAddress(IpAddr::V4(addr)));
    }
    sans.push(SanType::IpAddress(IpAddr::V4(Ipv4Addr::LOCALHOST)));
    if let Ok(name) = rcgen::string::Ia5String::try_from(MDNS_HOSTNAME) {
        sans.push(SanType::DnsName(name));
    }
    if let Ok(name) = rcgen::string::Ia5String::try_from("localhost") {
        sans.push(SanType::DnsName(name));
    }
    sans
}

fn san_labels(sans: &[SanType]) -> BTreeSet<String> {
    sans.iter()
        .map(|san| match san {
            SanType::IpAddress(ip) => ip.to_string(),
            SanType::DnsName(name) => name.as_str().to_string(),
            other => format!("{other:?}"),
        })
        .collect()
}

/// The default (and, in this wave, only) [`CertProvider`]: an `rcgen` CA and
/// leaf persisted under `<data>\pki`.
pub struct SelfSignedCa {
    dir: PathBuf,
    ca_pem: String,
    ca_key_pem: String,
    leaf: RwLock<IssuedLeaf>,
}

impl SelfSignedCa {
    /// Load the CA and leaf from `dir`, generating whatever is missing.
    ///
    /// Deliberately **does not** renew: [`CertProvider::renew_if_due`] is a
    /// separate, explicit call so the startup path, the periodic task and
    /// the acceptance test all drive renewal through the same door (and so a
    /// test can inject a nearly-expired leaf and observe it being served
    /// before renewal runs).
    pub fn open(dir: impl AsRef<Path>) -> Result<Self, PkiError> {
        let dir = dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir)?;

        let (ca_pem, ca_key_pem) = match (
            std::fs::read_to_string(dir.join(CA_CERT_FILE)),
            std::fs::read_to_string(dir.join(CA_KEY_FILE)),
        ) {
            (Ok(cert), Ok(key)) if KeyPair::from_pem(&key).is_ok() => (cert, key),
            _ => {
                let (cert, key) = generate_ca()?;
                write_public(&dir.join(CA_CERT_FILE), &cert)?;
                write_private(&dir.join(CA_KEY_FILE), &key)?;
                tracing::info!(dir = %dir.display(), "generated a new local certificate authority");
                (cert, key)
            }
        };

        let leaf = match load_leaf(&dir) {
            Ok(leaf) => leaf,
            Err(_) => {
                let leaf = issue_leaf(&ca_pem, &ca_key_pem)?;
                persist_leaf(&dir, &leaf)?;
                tracing::info!(
                    not_after = %leaf.not_after,
                    "issued the first server leaf certificate"
                );
                leaf
            }
        };

        Ok(Self {
            dir,
            ca_pem,
            ca_key_pem,
            leaf: RwLock::new(leaf),
        })
    }

    /// Directory the CA and leaf live in.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Force a re-issue regardless of the remaining validity. Used by
    /// [`CertProvider::renew_if_due`] and available to the (owner-facing,
    /// T3.1) recovery path.
    pub fn reissue(&self) -> Result<(), PkiError> {
        let leaf = issue_leaf(&self.ca_pem, &self.ca_key_pem)?;
        persist_leaf(&self.dir, &leaf)?;
        *self
            .leaf
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = leaf;
        Ok(())
    }
}

impl CertProvider for SelfSignedCa {
    fn ca_pem(&self) -> String {
        self.ca_pem.clone()
    }

    fn current(&self) -> IssuedLeaf {
        self.leaf
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn renew_if_due(&self) -> Result<bool, PkiError> {
        let current = self.current();
        let expected = san_labels(&leaf_san_names());
        let addresses_changed = !expected.is_subset(&current.sans);
        let expiring = current.days_remaining() < RENEW_WINDOW_DAYS;

        if !expiring && !addresses_changed {
            return Ok(false);
        }

        tracing::info!(
            days_remaining = current.days_remaining(),
            addresses_changed,
            "re-issuing the server leaf certificate"
        );
        self.reissue()?;
        Ok(true)
    }
}

fn distinguished_name(common_name: &str) -> DistinguishedName {
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, common_name);
    dn.push(DnType::OrganizationName, "Sheffield Family Hub");
    dn
}

/// Generate the 10-year CA. Returns `(certificate PEM, private key PEM)`.
fn generate_ca() -> Result<(String, String), PkiError> {
    let now = OffsetDateTime::now_utc();
    let mut params = CertificateParams::default();
    params.distinguished_name = distinguished_name("Sheffield Family Hub Local CA");
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    // A day of backdating absorbs clock skew between this PC and a phone
    // that has just come off a flat battery.
    params.not_before = now - Duration::days(1);
    params.not_after = now + Duration::days(CA_VALIDITY_DAYS);

    let key = KeyPair::generate()?;
    let cert = params.self_signed(&key)?;
    Ok((cert.pem(), key.serialize_pem()))
}

/// Issue a fresh leaf signed by the CA held in `ca_pem` / `ca_key_pem`.
fn issue_leaf(ca_pem: &str, ca_key_pem: &str) -> Result<IssuedLeaf, PkiError> {
    let now = OffsetDateTime::now_utc();
    let sans = leaf_san_names();

    let mut params = CertificateParams::default();
    params.distinguished_name = distinguished_name(MDNS_HOSTNAME);
    params.is_ca = IsCa::NoCa;
    params.subject_alt_names = sans.clone();
    params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyEncipherment,
    ];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    params.use_authority_key_identifier_extension = true;
    // Explicit on both ends (PURPLE_TEAM.md §P5.4 rcgen row): the default
    // `not_after` is the year 4096, which every Apple client rejects.
    params.not_before = now - Duration::hours(1);
    params.not_after = params.not_before + Duration::days(LEAF_VALIDITY_DAYS);

    let ca_key = KeyPair::from_pem(ca_key_pem)?;
    let issuer = Issuer::from_ca_cert_pem(ca_pem, ca_key)?;

    let leaf_key = KeyPair::generate()?;
    let cert = params.signed_by(&leaf_key, &issuer)?;

    Ok(IssuedLeaf {
        cert_pem: cert.pem(),
        key_pem: leaf_key.serialize_pem(),
        not_before: params.not_before,
        not_after: params.not_after,
        sans: san_labels(&sans),
    })
}

/// Read `leaf.crt` / `leaf.key` back and re-derive the validity window and
/// SAN list **from the certificate itself** rather than from whatever was in
/// memory when it was written.
fn load_leaf(dir: &Path) -> Result<IssuedLeaf, PkiError> {
    let cert_pem = std::fs::read_to_string(dir.join(LEAF_CERT_FILE))?;
    let key_pem = std::fs::read_to_string(dir.join(LEAF_KEY_FILE))?;
    // A key that no longer parses is as good as absent; the caller re-issues.
    KeyPair::from_pem(&key_pem)?;
    let (not_before, not_after, sans) = parse_certificate_validity(&cert_pem)?;
    Ok(IssuedLeaf {
        cert_pem,
        key_pem,
        not_before,
        not_after,
        sans,
    })
}

fn persist_leaf(dir: &Path, leaf: &IssuedLeaf) -> Result<(), PkiError> {
    write_public(&dir.join(LEAF_CERT_FILE), &leaf.cert_pem)?;
    write_private(&dir.join(LEAF_KEY_FILE), &leaf.key_pem)?;
    Ok(())
}

/// Parse a PEM certificate into `(not_before, not_after, SAN labels)`.
/// Shared by [`load_leaf`], `/health` (T1.7) and the acceptance tests, so
/// there is exactly one reading of what a stored certificate says.
pub fn parse_certificate_validity(
    cert_pem: &str,
) -> Result<(OffsetDateTime, OffsetDateTime, BTreeSet<String>), PkiError> {
    let (_, pem) = x509_parser::pem::parse_x509_pem(cert_pem.as_bytes())
        .map_err(|err| PkiError::Parse(err.to_string()))?;
    let cert = pem
        .parse_x509()
        .map_err(|err| PkiError::Parse(err.to_string()))?;

    let validity = cert.validity();
    let not_before = OffsetDateTime::from_unix_timestamp(validity.not_before.timestamp())
        .map_err(|err| PkiError::Parse(err.to_string()))?;
    let not_after = OffsetDateTime::from_unix_timestamp(validity.not_after.timestamp())
        .map_err(|err| PkiError::Parse(err.to_string()))?;

    let mut sans = BTreeSet::new();
    if let Ok(Some(extension)) = cert.subject_alternative_name() {
        for name in &extension.value.general_names {
            match name {
                x509_parser::extensions::GeneralName::DNSName(dns) => {
                    sans.insert((*dns).to_string());
                }
                x509_parser::extensions::GeneralName::IPAddress(bytes) => {
                    if let Ok(octets) = <[u8; 4]>::try_from(*bytes) {
                        sans.insert(Ipv4Addr::from(octets).to_string());
                    } else if let Ok(octets) = <[u8; 16]>::try_from(*bytes) {
                        sans.insert(std::net::Ipv6Addr::from(octets).to_string());
                    }
                }
                _ => {}
            }
        }
    }

    Ok((not_before, not_after, sans))
}

/// Is `cert_pem` a certificate authority (`basicConstraints: CA:TRUE`)?
/// Used by `/ca.crt`'s acceptance assertion and by the startup sanity check.
pub fn is_certificate_authority(cert_pem: &str) -> Result<bool, PkiError> {
    let (_, pem) = x509_parser::pem::parse_x509_pem(cert_pem.as_bytes())
        .map_err(|err| PkiError::Parse(err.to_string()))?;
    let cert = pem
        .parse_x509()
        .map_err(|err| PkiError::Parse(err.to_string()))?;
    Ok(cert
        .basic_constraints()
        .ok()
        .flatten()
        .map(|ext| ext.value.ca)
        .unwrap_or(false))
}

fn write_public(path: &Path, contents: &str) -> io::Result<()> {
    std::fs::write(path, contents)
}

/// Write a private key and then narrow its ACL. The narrowing is
/// best-effort and never fatal: a hub that cannot tighten an ACL is still a
/// hub that must boot. T3.1 (`install`, elevated) is where the data
/// directory's inherited permissions are set properly; this is the belt to
/// that braces, and matters most when the server is run interactively from
/// a user profile.
fn write_private(path: &Path, contents: &str) -> io::Result<()> {
    std::fs::write(path, contents)?;
    restrict_key_permissions(path);
    Ok(())
}

/// The `icacls /grant:r` operands for a private key: SYSTEM and
/// Administrators by well-known SID (those always resolve), plus the
/// interactive user running the hub from a profile — named `DOMAIN\user` so
/// an Entra/AzureAD account resolves too — but **never** a machine account.
/// Owner checklist step 3 (2026-08-31): under the installed service
/// (LocalSystem) `USERNAME` is `<HOST>$`, which `icacls` cannot map; error
/// 1332 failed the whole command and both keys kept the inherited
/// `BUILTIN\Users:(RX)` — readable by every local account — on the one
/// deployment that matters. SYSTEM already holds full control there, so the
/// user grant is simply omitted.
#[cfg_attr(not(windows), allow(dead_code))]
fn key_acl_grants(username: Option<&str>, userdomain: Option<&str>) -> Vec<String> {
    let mut grants = vec![
        "*S-1-5-18:(F)".to_string(),     // NT AUTHORITY\SYSTEM
        "*S-1-5-32-544:(F)".to_string(), // BUILTIN\Administrators
    ];
    let user = username
        .map(str::trim)
        .filter(|u| !u.is_empty() && !u.ends_with('$') && !u.eq_ignore_ascii_case("SYSTEM"));
    if let Some(user) = user {
        let principal = match userdomain.map(str::trim).filter(|d| !d.is_empty()) {
            Some(domain) => format!("{domain}\\{user}"),
            None => user.to_string(),
        };
        grants.push(format!("{principal}:(F)"));
    }
    grants
}

#[cfg(windows)]
fn restrict_key_permissions(path: &Path) {
    // `icacls` is a Windows built-in, not a project dependency: this is not
    // a new non-Rust component (docs/NON_RUST.md), it is the OS's own ACL
    // API surfaced as a command. The alternative — the `windows` crate's
    // `SetNamedSecurityInfoW` — would add a large dependency for one call.
    let grants = key_acl_grants(
        std::env::var("USERNAME").ok().as_deref(),
        std::env::var("USERDOMAIN").ok().as_deref(),
    );
    let result = std::process::Command::new("icacls.exe")
        .arg(path)
        .arg("/inheritance:r")
        .arg("/grant:r")
        .args(&grants)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    match result {
        Ok(status) if status.success() => {
            tracing::debug!(path = %path.display(), "restricted private-key ACL");
        }
        Ok(status) => {
            tracing::warn!(path = %path.display(), ?status, ?grants, "icacls did not restrict the private-key ACL");
        }
        Err(err) => {
            tracing::warn!(path = %path.display(), %err, "could not run icacls to restrict the private-key ACL");
        }
    }
}

#[cfg(not(windows))]
fn restrict_key_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Err(err) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)) {
        tracing::warn!(path = %path.display(), %err, "could not chmod the private key to 0600");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_acl_grants_omit_the_machine_account_under_the_service() {
        let system_and_admins = vec!["*S-1-5-18:(F)", "*S-1-5-32-544:(F)"];
        assert_eq!(
            key_acl_grants(Some("HUB-PC$"), Some("WORKGROUP")),
            system_and_admins
        );
        assert_eq!(key_acl_grants(None, None), system_and_admins);
        assert_eq!(key_acl_grants(Some(""), Some("X")), system_and_admins);
        assert_eq!(
            key_acl_grants(Some("SYSTEM"), Some("NT AUTHORITY")),
            system_and_admins
        );
    }

    #[test]
    fn key_acl_grants_name_the_interactive_user_with_their_domain() {
        assert_eq!(
            key_acl_grants(Some("LonzoSheffield"), Some("AzureAD")),
            vec![
                "*S-1-5-18:(F)",
                "*S-1-5-32-544:(F)",
                "AzureAD\\LonzoSheffield:(F)"
            ]
        );
        assert_eq!(
            key_acl_grants(Some("dev"), None).last().map(String::as_str),
            Some("dev:(F)")
        );
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "familyhub-pki-unit-{}-{}-{name}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn cert_source_defaults_to_self_signed_and_rejects_unknown_modes() {
        assert_eq!(
            CertSource::from_mode(None).expect("absent mode is the default"),
            CertSource::SelfSignedCa
        );
        assert_eq!(
            CertSource::from_mode(Some("self_signed")).expect("explicit default"),
            CertSource::SelfSignedCa
        );
        let err = CertSource::from_mode(Some("acme_dns01"))
            .expect_err("acme_dns01 is not implemented until T1.8");
        assert!(matches!(err, PkiError::UnknownCertSource(_)));
    }

    #[test]
    fn open_is_idempotent_and_keeps_the_same_ca() {
        let dir = scratch("idempotent");
        let first = SelfSignedCa::open(&dir).expect("first open generates the CA");
        let second = SelfSignedCa::open(&dir).expect("second open reuses it");
        assert_eq!(first.ca_pem(), second.ca_pem());
        assert_eq!(first.current().cert_pem, second.current().cert_pem);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_freshly_issued_leaf_is_not_due_for_renewal() {
        let dir = scratch("not-due");
        let ca = SelfSignedCa::open(&dir).expect("open");
        assert!(
            !ca.renew_if_due().expect("renewal check succeeds"),
            "a 397-day leaf must not be renewed on the spot"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_generated_ca_certificate_is_a_ca() {
        let dir = scratch("is-ca");
        let ca = SelfSignedCa::open(&dir).expect("open");
        assert!(is_certificate_authority(&ca.ca_pem()).expect("the CA PEM parses"));
        assert!(
            !is_certificate_authority(&ca.current().cert_pem).expect("the leaf PEM parses"),
            "the server leaf must not be a CA"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reissue_replaces_the_leaf_in_place_and_on_disk() {
        let dir = scratch("reissue");
        let ca = SelfSignedCa::open(&dir).expect("open");
        let before = ca.current();
        ca.reissue().expect("re-issue succeeds");
        let after = ca.current();
        assert_ne!(
            before.cert_pem, after.cert_pem,
            "re-issue must mint a different certificate"
        );
        let on_disk = std::fs::read_to_string(dir.join(LEAF_CERT_FILE)).expect("leaf.crt exists");
        assert_eq!(
            on_disk, after.cert_pem,
            "the re-issued leaf must be the one persisted"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
