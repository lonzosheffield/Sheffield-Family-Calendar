//! T1.3 acceptance suite — TLS + PKI + dual listener + mDNS + QR.
//!
//! One test per lettered assertion in `docs/reviews/PURPLE_TEAM.md` §P3,
//! row **T1.3**:
//!
//! | # | Assertion |
//! | - | --- |
//! | a | A `rustls` client with the generated CA in its root store completes a handshake against the live HTTPS listener and `GET /health` → 200 |
//! | b | `GET http://…/ca.crt` → 200, `application/x-x509-ca-cert`, parses as a valid X.509 CA |
//! | c | `GET http://127.0.0.1:8080/m` → 308 to `https://…:8443/m`; `GET /tv` → 200, not a redirect |
//! | d | Leaf `not_after - not_before` is ≥ 396 and ≤ 398 days; the SAN list contains every non-loopback IPv4 of this host |
//! | e | Injecting a leaf with 29 days remaining triggers re-issue and the listener serves the new cert **without restart** |
//! | f | An mDNS `A` query for `familyhub.local` issued from this host is answered with this host's IP |
//! | g | The QR SVG decodes to exactly `https://<ip>:8443/m` |
//!
//! Every assertion is driven against real sockets on this machine: (a) and
//! (e) speak TLS over a bound listener with `tokio-rustls`, (b) and (c) go
//! through the production HTTP router, (f) puts a hand-built DNS packet on
//! the wire, and (g) rasterises the component's own SVG output and decodes
//! it with an independent QR reader (`rqrr`) rather than trusting the
//! encoder's word for it.

#![cfg(feature = "server")]

use std::collections::BTreeSet;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine as _;
use family_calendar::client::components::qr::{phone_join_url, qr_svg, DEFAULT_PHONE_PORT};
use family_calendar::server::config::FamilyHubConfig;
use family_calendar::server::db;
use family_calendar::server::mdns;
use family_calendar::server::pki::{CertProvider, SelfSignedCa};
use family_calendar::server::router::{build_http_router, build_router};
use family_calendar::server::tls::{install_crypto_provider, TlsListener};
use rustls::pki_types::{CertificateDer, ServerName};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// One throwaway data directory per test binary, mirroring
/// `tests/router_tests.rs::init_test_env` (`db::pool()` is a process-wide
/// `OnceCell`, so the first `DATABASE_URL` wins for the whole binary).
fn init_test_env() -> PathBuf {
    static ONCE: std::sync::Once = std::sync::Once::new();
    let base = std::env::temp_dir().join(format!("familyhub-tls-tests-{}", std::process::id()));
    ONCE.call_once(|| {
        // Windows reuses PIDs: wipe any leftover scratch dir from an earlier run first.
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("test scratch directory is creatable");

        let db_path = base.join("family.db");
        let url = format!(
            "sqlite://{}",
            db_path.display().to_string().replace('\\', "/")
        );
        std::env::set_var("DATABASE_URL", url);

        let public = base.join("public");
        std::fs::create_dir_all(&public).expect("test public directory is creatable");
        std::env::set_var("DIOXUS_PUBLIC_PATH", &public);

        // HS9 (`docs/BACKLOG.md` B-3): this harness — never the shell — pins
        // the data directory, so nothing in this binary can resolve config to
        // the family's live `%ProgramData%\FamilyHub`.
        std::env::set_var("FAMILY_HUB_DATA_DIR", &base);

        install_crypto_provider();
    });
    base
}

/// A config rooted at the shared scratch directory. `tls_addr` is the real
/// default (`:8443`) because assertion (c) asserts the *port in the
/// redirect*, which is read from exactly this field.
fn test_config() -> FamilyHubConfig {
    FamilyHubConfig {
        data_dir: init_test_env(),
        http_addr: "127.0.0.1:0".parse().expect("valid socket address"),
        tls_addr: "0.0.0.0:8443".parse().expect("valid socket address"),
        screensaver_schedule_hour: None,
        log_level: None,
    }
}

/// A config with its own private PKI directory, so a test that mutates
/// certificates on disk cannot disturb another test's listener.
fn isolated_config(name: &str) -> FamilyHubConfig {
    let base = init_test_env().join(format!("isolated-{name}"));
    std::fs::create_dir_all(&base).expect("isolated scratch directory is creatable");
    FamilyHubConfig {
        data_dir: base,
        http_addr: "127.0.0.1:0".parse().expect("valid socket address"),
        tls_addr: "0.0.0.0:8443".parse().expect("valid socket address"),
        screensaver_schedule_hour: None,
        log_level: None,
    }
}

/// Boot the plain-HTTP origin (`build_http_router`, i.e. the router the
/// :8080 listener actually serves) on an OS-assigned port.
async fn spawn_http_origin(config: &FamilyHubConfig) -> SocketAddr {
    db::pool().await.expect("test sqlite pool opens");

    let router = build_http_router(config);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind an ephemeral port");
    let addr = listener.local_addr().expect("listener has a local address");
    tokio::spawn(async move {
        let _ = axum::serve(listener, router.into_make_service()).await;
    });
    addr
}

/// Boot the HTTPS origin against `certs` on an OS-assigned port. Returns the
/// bound address and the live resolver, so a test can hot-swap the
/// certificate the running listener serves.
async fn spawn_https_origin(
    config: &FamilyHubConfig,
    certs: &SelfSignedCa,
) -> (
    SocketAddr,
    Arc<family_calendar::server::tls::HotReloadResolver>,
) {
    db::pool().await.expect("test sqlite pool opens");

    let listener = TlsListener::bind("127.0.0.1:0".parse().unwrap(), certs)
        .await
        .expect("the HTTPS listener binds");
    let addr = listener.local_addr;
    let resolver = listener.resolver();

    let router = build_router(config);
    tokio::spawn(listener.serve(router));
    (addr, resolver)
}

/// The DER bytes of the first `CERTIFICATE` block in `pem`.
fn first_certificate_der(pem: &str) -> Vec<u8> {
    let begin = "-----BEGIN CERTIFICATE-----";
    let end = "-----END CERTIFICATE-----";
    let start = pem.find(begin).expect("a CERTIFICATE block") + begin.len();
    let stop = pem[start..].find(end).expect("a closing END line") + start;
    let body: String = pem[start..stop]
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect();
    base64::engine::general_purpose::STANDARD
        .decode(body)
        .expect("the PEM body is base64")
}

/// A `rustls` client configuration whose *only* trust anchor is `ca_pem`.
/// Nothing from the OS trust store is added, so a successful handshake
/// proves the served leaf chains to the CA this hub generated.
fn client_config_trusting(ca_pem: &str) -> Arc<rustls::ClientConfig> {
    install_crypto_provider();
    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(CertificateDer::from(first_certificate_der(ca_pem)))
        .expect("the generated CA is a usable trust anchor");

    Arc::new(
        rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth(),
    )
}

/// Open a TLS connection to `addr`, send `request`, and return
/// `(response bytes, the peer certificate chain the server presented)`.
async fn tls_request(
    addr: SocketAddr,
    ca_pem: &str,
    request: &str,
) -> (String, Vec<CertificateDer<'static>>) {
    let connector = tokio_rustls::TlsConnector::from(client_config_trusting(ca_pem));
    let server_name = ServerName::try_from("127.0.0.1").expect("127.0.0.1 is a valid server name");

    let tcp = tokio::net::TcpStream::connect(addr)
        .await
        .expect("the HTTPS listener accepts TCP");
    let mut tls = connector
        .connect(server_name, tcp)
        .await
        .expect("the TLS handshake completes against the local CA");

    let chain = tls
        .get_ref()
        .1
        .peer_certificates()
        .expect("the server presented a certificate")
        .iter()
        .map(|der| der.clone().into_owned())
        .collect();

    tls.write_all(request.as_bytes())
        .await
        .expect("request is written");
    tls.flush().await.expect("request is flushed");

    let mut body = Vec::new();
    // `Connection: close` means the server drops the connection when done;
    // rustls surfaces that as an UnexpectedEof, which is the normal ending
    // here rather than an error.
    let _ = tls.read_to_end(&mut body).await;
    (String::from_utf8_lossy(&body).to_string(), chain)
}

fn http_client_no_redirect() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(15))
        .build()
        .expect("reqwest client builds")
}

// ---------------------------------------------------------------------------
// (a) A rustls client with the generated CA handshakes and GETs /health
// ---------------------------------------------------------------------------

#[tokio::test]
async fn tls_a_rustls_client_with_the_local_ca_gets_health_200_over_https() {
    let config = isolated_config("handshake");
    let certs = SelfSignedCa::open(config.pki_dir()).expect("the local CA opens");
    let (addr, _resolver) = spawn_https_origin(&config, &certs).await;

    let (response, chain) = tls_request(
        addr,
        &certs.ca_pem(),
        "GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    )
    .await;

    assert!(
        response.starts_with("HTTP/1.1 200"),
        "expected 200 from /health over TLS, got:\n{}",
        response.lines().next().unwrap_or("<empty>")
    );
    assert!(
        response.contains("application/json"),
        "/health must still answer as JSON over the HTTPS origin"
    );
    assert_eq!(
        chain.len(),
        1,
        "the listener should present exactly the end-entity leaf"
    );
}

// ---------------------------------------------------------------------------
// (b) /ca.crt over plain HTTP is a parseable X.509 CA
// ---------------------------------------------------------------------------

#[tokio::test]
async fn tls_b_ca_crt_is_served_over_plain_http_and_parses_as_a_certificate_authority() {
    let config = test_config();
    let addr = spawn_http_origin(&config).await;

    let response = http_client_no_redirect()
        .get(format!("http://{addr}/ca.crt"))
        .send()
        .await
        .expect("GET /ca.crt should respond");

    assert_eq!(
        response.status().as_u16(),
        200,
        "/ca.crt must be reachable on the plain-HTTP origin: a phone that \
         does not trust the CA yet cannot fetch it over TLS secured by it"
    );
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(
        content_type.starts_with("application/x-x509-ca-cert"),
        "expected application/x-x509-ca-cert, got {content_type:?}"
    );

    let pem = response.text().await.expect("response body");
    let der = first_certificate_der(&pem);
    let (_, parsed) =
        x509_parser::parse_x509_certificate(&der).expect("the served body parses as X.509");
    let basic_constraints = parsed
        .basic_constraints()
        .expect("basicConstraints is well formed")
        .expect("a CA certificate carries basicConstraints");
    assert!(
        basic_constraints.value.ca,
        "the certificate served at /ca.crt must have CA:TRUE"
    );
    assert!(
        parsed
            .subject()
            .to_string()
            .contains("Sheffield Family Hub Local CA"),
        "unexpected CA subject: {}",
        parsed.subject()
    );
}

// ---------------------------------------------------------------------------
// (c) The HTTP origin 308s the phone surface only
// ---------------------------------------------------------------------------

#[tokio::test]
async fn tls_c_http_origin_redirects_the_phone_surface_and_serves_the_tv() {
    let config = test_config();
    let addr = spawn_http_origin(&config).await;
    let client = http_client_no_redirect();

    // `Host: 127.0.0.1:8080` makes this exactly the request the acceptance
    // row names (`GET http://127.0.0.1:8080/m`); the listener itself is on
    // an OS-assigned port so the suite never fights whatever else on this
    // machine wants 8080.
    let response = client
        .get(format!("http://{addr}/m"))
        .header("host", "127.0.0.1:8080")
        .send()
        .await
        .expect("GET /m should respond");

    assert_eq!(
        response.status().as_u16(),
        308,
        "the plain-HTTP origin must permanently redirect the phone surface"
    );
    let location = response
        .headers()
        .get("location")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    assert_eq!(
        location, "https://127.0.0.1:8443/m",
        "GET http://127.0.0.1:8080/m must 308 to the HTTPS phone origin"
    );

    // The TV surface is served, not redirected: a broken certificate must
    // never be able to take the kiosk down (PLAN v2 D3').
    let tv = client
        .get(format!("http://{addr}/tv"))
        .header("host", "127.0.0.1:8080")
        .header("accept", "text/html")
        .send()
        .await
        .expect("GET /tv should respond");
    assert_eq!(
        tv.status().as_u16(),
        200,
        "/tv must be 200 on the HTTP origin, not a redirect"
    );
    let body = tv.text().await.expect("response body");
    assert!(
        body.contains("Morning Routine"),
        "/tv on the HTTP origin must render the kiosk, not a redirect stub"
    );

    // The other two upgraded paths, for completeness of the D3' rule.
    for path in ["/manifest.webmanifest", "/sw.js"] {
        let response = client
            .get(format!("http://{addr}{path}"))
            .header("host", "127.0.0.1:8080")
            .send()
            .await
            .unwrap_or_else(|err| panic!("GET {path} should respond: {err}"));
        assert_eq!(
            response.status().as_u16(),
            308,
            "{path} is part of the phone surface and must be upgraded"
        );
    }

    // ...and one that must not be.
    let health = client
        .get(format!("http://{addr}/health"))
        .header("host", "127.0.0.1:8080")
        .send()
        .await
        .expect("GET /health should respond");
    assert_eq!(
        health.status().as_u16(),
        200,
        "/health must stay on the HTTP origin for the TV's staleness badge"
    );
}

// ---------------------------------------------------------------------------
// (d) Leaf validity is 397 days and the SANs cover every host IPv4
// ---------------------------------------------------------------------------

#[tokio::test]
async fn tls_d_leaf_is_397_days_and_covers_every_non_loopback_host_ipv4() {
    let config = isolated_config("validity");
    let certs = SelfSignedCa::open(config.pki_dir()).expect("the local CA opens");
    let leaf = certs.current();

    let der = first_certificate_der(&leaf.cert_pem);
    let (_, parsed) = x509_parser::parse_x509_certificate(&der).expect("the leaf parses as X.509");

    // Read the window back out of the certificate itself rather than out of
    // whatever the issuing code believed it wrote.
    let validity = parsed.validity();
    let days = (validity.not_after.timestamp() - validity.not_before.timestamp()) / 86_400;
    assert!(
        (396..=398).contains(&days),
        "leaf validity must be 396-398 days (Apple rejects > 398); got {days}"
    );

    // ...and prove the rcgen default was actually overridden: rcgen's
    // untouched `not_after` is the year 4096.
    assert!(
        validity.not_after.to_datetime().year() < 2100,
        "not_after was left at rcgen's year-4096 default"
    );

    let mut sans: BTreeSet<String> = BTreeSet::new();
    for name in &parsed
        .subject_alternative_name()
        .expect("SAN extension is well formed")
        .expect("the leaf carries a SAN extension")
        .value
        .general_names
    {
        match name {
            x509_parser::extensions::GeneralName::DNSName(dns) => {
                sans.insert((*dns).to_string());
            }
            x509_parser::extensions::GeneralName::IPAddress(bytes) => {
                if let Ok(octets) = <[u8; 4]>::try_from(*bytes) {
                    sans.insert(Ipv4Addr::from(octets).to_string());
                }
            }
            _ => {}
        }
    }

    // Enumerate this host's addresses independently of the code under test.
    let mut expected: BTreeSet<String> = BTreeSet::new();
    for interface in if_addrs::get_if_addrs().expect("this host's interfaces are enumerable") {
        if interface.is_loopback() {
            continue;
        }
        if let IpAddr::V4(v4) = interface.ip() {
            if !v4.is_unspecified() {
                expected.insert(v4.to_string());
            }
        }
    }
    assert!(
        !expected.is_empty(),
        "this host has no non-loopback IPv4 address; the acceptance row \
         assumes a LAN-connected machine"
    );

    for address in &expected {
        assert!(
            sans.contains(address),
            "leaf SANs {sans:?} are missing this host's IPv4 {address}"
        );
    }

    // The route the OS would actually pick for LAN traffic — a second,
    // independent witness that the SAN set is the reachable one.
    if let Some(primary) = family_calendar::server::pki::primary_ipv4_address() {
        assert!(
            sans.contains(&primary.to_string()),
            "leaf SANs {sans:?} are missing the primary LAN address {primary}"
        );
    }

    for name in ["familyhub.local", "localhost", "127.0.0.1"] {
        assert!(
            sans.contains(name),
            "leaf SANs {sans:?} are missing the required name {name}"
        );
    }
}

// ---------------------------------------------------------------------------
// (e) A leaf with 29 days left is re-issued and hot-reloaded, no restart
// ---------------------------------------------------------------------------

/// Overwrite `<pki>/leaf.crt` + `leaf.key` with a leaf signed by the CA
/// already in that directory but expiring in `days_remaining` days. The
/// injection deliberately uses `rcgen` directly rather than any helper from
/// the crate under test, so nothing about the renewal path is assumed.
fn inject_leaf_expiring_in(pki_dir: &std::path::Path, days_remaining: i64) {
    use rcgen::{CertificateParams, DnType, ExtendedKeyUsagePurpose, Issuer, KeyPair, SanType};
    use time::{Duration, OffsetDateTime};

    let ca_pem = std::fs::read_to_string(pki_dir.join("ca.crt")).expect("ca.crt exists");
    let ca_key_pem = std::fs::read_to_string(pki_dir.join("ca.key")).expect("ca.key exists");
    let ca_key = KeyPair::from_pem(&ca_key_pem).expect("the CA key parses");
    let issuer = Issuer::from_ca_cert_pem(&ca_pem, ca_key).expect("the CA cert parses");

    let now = OffsetDateTime::now_utc();
    let mut params = CertificateParams::default();
    params
        .distinguished_name
        .push(DnType::CommonName, "familyhub.local");
    params.not_after = now + Duration::days(days_remaining);
    params.not_before = params.not_after - Duration::days(397);
    params.subject_alt_names = vec![
        SanType::IpAddress(IpAddr::V4(Ipv4Addr::LOCALHOST)),
        SanType::DnsName("localhost".try_into().expect("valid IA5 string")),
    ];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];

    let key = KeyPair::generate().expect("a leaf key is generated");
    let cert = params
        .signed_by(&key, &issuer)
        .expect("the CA signs the leaf");

    std::fs::write(pki_dir.join("leaf.crt"), cert.pem()).expect("leaf.crt is writable");
    std::fs::write(pki_dir.join("leaf.key"), key.serialize_pem()).expect("leaf.key is writable");
}

fn not_after_of(chain: &[CertificateDer<'static>]) -> i64 {
    let (_, parsed) =
        x509_parser::parse_x509_certificate(chain.first().expect("a served certificate"))
            .expect("the served certificate parses");
    parsed.validity().not_after.timestamp()
}

#[tokio::test]
async fn tls_e_a_leaf_with_29_days_left_is_reissued_and_served_without_a_restart() {
    let config = isolated_config("renewal");
    let pki_dir = config.pki_dir();

    // 1. Establish the CA (and a healthy 397-day leaf).
    SelfSignedCa::open(&pki_dir).expect("the local CA opens");

    // 2. Inject a leaf with 29 days remaining, then re-open so it is the
    //    one being served. `open` deliberately does not renew, so the
    //    listener genuinely starts out on the nearly-expired certificate.
    inject_leaf_expiring_in(&pki_dir, 29);
    let certs = SelfSignedCa::open(&pki_dir).expect("the injected leaf loads");
    let days_before = certs.current().days_remaining();
    assert!(
        (27..=29).contains(&days_before),
        "test setup: expected ~29 days remaining, got {days_before}"
    );

    // 3. Boot the listener on the nearly-expired leaf and record what it
    //    actually serves.
    let (addr, resolver) = spawn_https_origin(&config, &certs).await;
    let ca_pem = certs.ca_pem();
    let (response, chain_before) = tls_request(
        addr,
        &ca_pem,
        "GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(response.starts_with("HTTP/1.1 200"));
    let expiry_before = not_after_of(&chain_before);

    // 4. The renewal predicate fires at < 30 days remaining.
    assert!(
        certs.renew_if_due().expect("the renewal check succeeds"),
        "a leaf with 29 days remaining must be re-issued (window is 30 days)"
    );
    resolver
        .replace(&certs.current())
        .expect("the re-issued leaf loads into the live resolver");

    // 5. The *same* listener — never restarted, never rebound — now serves
    //    the new certificate.
    let (response, chain_after) = tls_request(
        addr,
        &ca_pem,
        "GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(
        response.starts_with("HTTP/1.1 200"),
        "the listener must keep serving across a certificate swap"
    );

    let expiry_after = not_after_of(&chain_after);
    assert!(
        expiry_after > expiry_before,
        "the hot-reloaded certificate must expire later than the one it replaced"
    );
    assert_ne!(
        chain_before, chain_after,
        "the listener is still presenting the nearly-expired certificate"
    );
    let days_after = certs.current().days_remaining();
    assert!(
        (395..=397).contains(&days_after),
        "the re-issued leaf should be a fresh 397-day certificate, got {days_after} days"
    );

    // And a healthy leaf is left alone.
    assert!(
        !certs.renew_if_due().expect("the renewal check succeeds"),
        "a freshly re-issued leaf must not be re-issued again on the next tick"
    );
}

// ---------------------------------------------------------------------------
// (f) mDNS answers an A query for familyhub.local with this host's IP
// ---------------------------------------------------------------------------

/// Encode `name` (e.g. `familyhub.local`) as a DNS QNAME.
fn encode_qname(name: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for label in name.trim_end_matches('.').split('.') {
        out.push(label.len() as u8);
        out.extend_from_slice(label.as_bytes());
    }
    out.push(0);
    out
}

/// A minimal DNS query packet: one question, type A, class IN.
fn build_a_query(name: &str, id: u16) -> Vec<u8> {
    let mut packet = Vec::new();
    packet.extend_from_slice(&id.to_be_bytes()); // ID
    packet.extend_from_slice(&0u16.to_be_bytes()); // flags: standard query
    packet.extend_from_slice(&1u16.to_be_bytes()); // QDCOUNT
    packet.extend_from_slice(&0u16.to_be_bytes()); // ANCOUNT
    packet.extend_from_slice(&0u16.to_be_bytes()); // NSCOUNT
    packet.extend_from_slice(&0u16.to_be_bytes()); // ARCOUNT
    packet.extend_from_slice(&encode_qname(name));
    packet.extend_from_slice(&1u16.to_be_bytes()); // QTYPE = A
    packet.extend_from_slice(&1u16.to_be_bytes()); // QCLASS = IN
    packet
}

/// Advance `pos` past a (possibly compressed) DNS name.
fn skip_name(packet: &[u8], pos: &mut usize) -> Option<()> {
    loop {
        let len = *packet.get(*pos)?;
        if len & 0xC0 == 0xC0 {
            *pos += 2; // compression pointer, name ends here
            return Some(());
        }
        *pos += 1;
        if len == 0 {
            return Some(());
        }
        *pos += len as usize;
    }
}

/// Pull every A record out of a DNS response's answer section.
fn parse_a_records(packet: &[u8]) -> Vec<Ipv4Addr> {
    let mut out = Vec::new();
    if packet.len() < 12 {
        return out;
    }
    let qdcount = u16::from_be_bytes([packet[4], packet[5]]);
    let ancount = u16::from_be_bytes([packet[6], packet[7]]);

    let mut pos = 12;
    for _ in 0..qdcount {
        if skip_name(packet, &mut pos).is_none() {
            return out;
        }
        pos += 4; // QTYPE + QCLASS
    }

    for _ in 0..ancount {
        if skip_name(packet, &mut pos).is_none() || pos + 10 > packet.len() {
            return out;
        }
        let rtype = u16::from_be_bytes([packet[pos], packet[pos + 1]]);
        let rdlength = u16::from_be_bytes([packet[pos + 8], packet[pos + 9]]) as usize;
        pos += 10;
        if pos + rdlength > packet.len() {
            return out;
        }
        if rtype == 1 && rdlength == 4 {
            out.push(Ipv4Addr::new(
                packet[pos],
                packet[pos + 1],
                packet[pos + 2],
                packet[pos + 3],
            ));
        }
        pos += rdlength;
    }
    out
}

#[test]
fn tls_f_mdns_answers_an_a_query_for_familyhub_local_with_this_hosts_ip() {
    let host_addresses = family_calendar::server::pki::host_ipv4_addresses();
    assert!(
        !host_addresses.is_empty(),
        "this host has no non-loopback IPv4 address; the acceptance row \
         assumes a LAN-connected machine"
    );

    // The responder under test: exactly one ServiceDaemon, advertising
    // familyhub.local. on this host's addresses.
    let advertised = mdns::register(8080, 8443).expect("mDNS registration succeeds on this host");
    assert_eq!(
        advertised, host_addresses,
        "the A record must advertise this host's addresses"
    );

    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).expect("an ephemeral UDP socket");
    socket
        .set_read_timeout(Some(Duration::from_millis(500)))
        .expect("read timeout is settable");
    // RFC 6762 responses carry TTL 255; ours only has to leave the host.
    let _ = socket.set_multicast_ttl_v4(255);
    let _ = socket.set_multicast_loop_v4(true);

    let query = build_a_query("familyhub.local", 0x4142);

    // Destinations, in order of fidelity: the mDNS multicast group first,
    // then direct unicast to 5353 on each of this host's own addresses.
    // mdns-sd answers a querier whose source port is not 5353 with an
    // RFC 6762 6.7 legacy-unicast reply, which is what lets an ordinary
    // ephemeral socket see the answer at all.
    let mut destinations: Vec<SocketAddr> =
        vec![SocketAddr::from((Ipv4Addr::new(224, 0, 0, 251), 5353))];
    destinations.extend(
        host_addresses
            .iter()
            .map(|ip| SocketAddr::from((*ip, 5353u16))),
    );

    // The daemon has to finish probing/announcing before it answers, which
    // takes a couple of seconds on a quiet LAN.
    let deadline = Instant::now() + Duration::from_secs(45);
    let mut buffer = [0u8; 4096];
    let mut seen: BTreeSet<Ipv4Addr> = BTreeSet::new();

    while Instant::now() < deadline && seen.is_empty() {
        for destination in &destinations {
            let _ = socket.send_to(&query, destination);
        }
        // Drain whatever came back before the next round of queries.
        let round = Instant::now() + Duration::from_millis(1200);
        while Instant::now() < round {
            match socket.recv_from(&mut buffer) {
                Ok((len, _from)) => {
                    for address in parse_a_records(&buffer[..len]) {
                        if host_addresses.contains(&address) {
                            seen.insert(address);
                        }
                    }
                }
                Err(_) => break, // timeout: nothing pending this round
            }
        }
    }

    assert!(
        !seen.is_empty(),
        "no mDNS A record for familyhub.local came back within 45s; \
         expected one of this host's addresses {host_addresses:?}"
    );
    for address in &seen {
        assert!(
            host_addresses.contains(address),
            "familyhub.local resolved to {address}, which is not an address of this host"
        );
    }
}

// ---------------------------------------------------------------------------
// (g) The QR SVG decodes to exactly https://<ip>:8443/m
// ---------------------------------------------------------------------------

/// Rasterise `svg` and read the QR code back out of the pixels with an
/// independent decoder (`rqrr`). This deliberately goes through the *SVG*
/// the component renders, not the encoder's module matrix, so the assertion
/// covers everything between "encode this URL" and "what a phone camera
/// sees".
fn decode_qr_svg(svg: &str, size: u32) -> String {
    use resvg::tiny_skia;
    use resvg::usvg;

    let tree =
        usvg::Tree::from_str(svg, &usvg::Options::default()).expect("the QR SVG parses as SVG");
    let mut pixmap = tiny_skia::Pixmap::new(size, size).expect("a pixmap of the requested size");
    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());

    let gray = image::GrayImage::from_fn(size, size, |x, y| {
        let pixel = pixmap
            .pixel(x, y)
            .expect("every pixel inside the pixmap exists");
        // The QR is pure black on white; luminance of the (premultiplied,
        // fully opaque) sample is all the decoder needs.
        let luma = (u32::from(pixel.red()) * 299
            + u32::from(pixel.green()) * 587
            + u32::from(pixel.blue()) * 114)
            / 1000;
        image::Luma([luma as u8])
    });

    let mut prepared = rqrr::PreparedImage::prepare(gray);
    let grids = prepared.detect_grids();
    assert_eq!(
        grids.len(),
        1,
        "expected exactly one QR grid in the rendered SVG, found {}",
        grids.len()
    );
    let (_meta, content) = grids[0].decode().expect("the detected grid decodes");
    content
}

#[test]
fn tls_g_the_join_qr_svg_decodes_to_the_https_phone_url() {
    // The URL of record: the raw-IP HTTPS phone origin (never the .local
    // name — Fire OS 7/8 cannot resolve it, RR-7).
    let url = phone_join_url("10.0.0.42", DEFAULT_PHONE_PORT);
    assert_eq!(url, "https://10.0.0.42:8443/m");

    let svg = qr_svg(&url, 512).expect("the join URL encodes as a QR code");
    assert!(
        svg.starts_with("<svg "),
        "qr_svg must return an SVG document"
    );

    assert_eq!(
        decode_qr_svg(&svg, 512),
        url,
        "the rendered QR must decode to exactly the HTTPS phone URL"
    );

    // And the same for whatever this host's real primary address is, since
    // that is what the kiosk overlay will actually show.
    if let Some(ip) = family_calendar::server::pki::primary_ipv4_address() {
        let live = phone_join_url(&ip.to_string(), DEFAULT_PHONE_PORT);
        let svg = qr_svg(&live, 512).expect("the live join URL encodes");
        assert_eq!(decode_qr_svg(&svg, 512), live);
    }
}

// ---------------------------------------------------------------------------
// (h) Same-origin POST over HTTP/2 passes the origin gate
// ---------------------------------------------------------------------------

/// Owner checklist step 4 (2026-08-31): every phone negotiates HTTP/2 with
/// the `:8443` listener (`tls.rs` offers `h2` first), and under h2 there is
/// no `Host` header — the authority rides in `:authority`, which hyper
/// exposes on the request URI. `auth::same_origin_or_absent` compared
/// `Origin` against `Host` alone, so every real browser's same-origin setup
/// and login came back `403 cross-origin ... not allowed` while an HTTP/1.1
/// `curl` passed. This drives the real listener with a real h2 client
/// (reqwest + rustls negotiate it over ALPN) and asserts the gate lets a
/// same-origin request through to the actual code check.
#[tokio::test]
async fn tls_h_same_origin_setup_over_http2_reaches_the_code_check_not_403() {
    let config = isolated_config("h2-origin");
    let certs = SelfSignedCa::open(config.pki_dir()).expect("the local CA opens");
    let (addr, _resolver) = spawn_https_origin(&config, &certs).await;
    let origin = format!("https://127.0.0.1:{}", addr.port());

    let client = reqwest::Client::builder()
        // The leaf is issued for the LAN addresses, not loopback (tls_d);
        // trust is not what this test is about.
        .danger_accept_invalid_certs(true)
        .build()
        .expect("reqwest client builds");
    let send = |site: Option<&'static str>, origin_header: String| {
        let mut request = client
            .post(format!("{origin}/api/setup"))
            .header("origin", origin_header)
            .header("content-type", "application/json")
            .body(r#"{"setup_code":"000000","pin":"246810"}"#);
        if let Some(site) = site {
            request = request.header("sec-fetch-site", site);
        }
        request.send()
    };

    // Safari < 16.4 shape: Origin only.
    let response = send(None, origin.clone()).await.expect("request completes");
    assert_eq!(
        response.version(),
        reqwest::Version::HTTP_2,
        "the client must have negotiated HTTP/2 for this test to mean anything"
    );
    assert_eq!(
        response.status(),
        reqwest::StatusCode::UNAUTHORIZED,
        "a same-origin POST /api/setup over h2 must reach the code check (401 for a wrong code), not the origin gate (403): {}",
        response.text().await.unwrap_or_default()
    );

    // Modern browser shape: Origin + Sec-Fetch-Site: same-origin.
    let response = send(Some("same-origin"), origin.clone())
        .await
        .expect("request completes");
    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);

    // The gate itself still works over h2: a foreign Origin is refused.
    let response = send(None, "https://evil.example".to_string())
        .await
        .expect("request completes");
    assert_eq!(
        response.status(),
        reqwest::StatusCode::FORBIDDEN,
        "a cross-origin POST /api/setup must still be refused over h2"
    );
}
