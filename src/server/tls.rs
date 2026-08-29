//! The HTTPS listener for the phone origin (PLAN v2 D3′, task T1.3).
//!
//! Built on `tokio-rustls` + `hyper-util`'s auto (h1/h2) server rather than
//! `axum-server` (PURPLE_TEAM.md §P5.4: `axum-server` 0.8.0 is ~9 months
//! stale and its dev-dependencies are still on axum 0.7; and
//! `axum-server-dual-protocol` is explicitly out).
//!
//! **Hot reload.** The `ServerConfig` handed to `TlsAcceptor` holds a
//! [`HotReloadResolver`], which reads the current leaf out of an
//! `RwLock` on every ClientHello. Re-issuing a certificate therefore takes
//! effect on the next handshake with no listener restart, no dropped
//! connection, and no coordination with the accept loop — which is what
//! makes "re-issue at 30 days remaining" safe to run on a background timer
//! against a display that is never power-cycled.

use std::net::SocketAddr;
use std::sync::{Arc, RwLock};

use axum::Router;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as AutoBuilder;
use hyper_util::service::TowerToHyperService;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::{ClientHello, ResolvesServerCert};
use rustls::sign::CertifiedKey;
use rustls::ServerConfig;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

use crate::server::pki::{CertProvider, IssuedLeaf, PkiError};

/// Install the `ring` crypto provider as rustls's process-wide default.
///
/// PURPLE_TEAM.md §P5.4 requires this to run before anything touches
/// rustls: with more than one provider linked in (`reqwest` also pulls
/// rustls), `CryptoProvider::get_default()` panics rather than guessing.
/// `ring` is chosen over `aws-lc-rs` deliberately — aws-lc-rs needs CMake
/// and NASM on Windows, neither of which this machine has.
///
/// Idempotent: a second call is a no-op, so the server entrypoint and every
/// test binary can both call it unconditionally.
pub fn install_crypto_provider() {
    // `install_default` returns Err when a provider is already installed,
    // which is exactly the no-op case.
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// Resolves the server certificate for every ClientHello by reading the
/// provider's *current* leaf, so a re-issue is picked up without rebuilding
/// the `ServerConfig` or restarting the listener.
#[derive(Debug)]
pub struct HotReloadResolver {
    current: RwLock<Arc<CertifiedKey>>,
}

impl HotReloadResolver {
    /// Build a resolver seeded with `leaf`.
    pub fn new(leaf: &IssuedLeaf) -> Result<Self, TlsError> {
        Ok(Self {
            current: RwLock::new(certified_key(leaf)?),
        })
    }

    /// Swap in a newly issued leaf. Takes effect on the next handshake.
    pub fn replace(&self, leaf: &IssuedLeaf) -> Result<(), TlsError> {
        let key = certified_key(leaf)?;
        *self
            .current
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = key;
        Ok(())
    }
}

impl ResolvesServerCert for HotReloadResolver {
    fn resolve(&self, _client_hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        Some(
            self.current
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone(),
        )
    }
}

/// Errors from building or running the TLS listener.
#[derive(Debug)]
pub enum TlsError {
    Pki(PkiError),
    Rustls(rustls::Error),
    Io(std::io::Error),
    /// A stored PEM did not contain the certificate or key it should.
    Pem(String),
}

impl std::fmt::Display for TlsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pki(err) => write!(f, "{err}"),
            Self::Rustls(err) => write!(f, "rustls error: {err}"),
            Self::Io(err) => write!(f, "tls i/o error: {err}"),
            Self::Pem(msg) => write!(f, "malformed PEM: {msg}"),
        }
    }
}

impl std::error::Error for TlsError {}

impl From<PkiError> for TlsError {
    fn from(err: PkiError) -> Self {
        Self::Pki(err)
    }
}

impl From<rustls::Error> for TlsError {
    fn from(err: rustls::Error) -> Self {
        Self::Rustls(err)
    }
}

impl From<std::io::Error> for TlsError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

/// Decode one PEM block of `label` into its DER payload. A hand-rolled
/// three-line reader beats adding `rustls-pemfile` for two call sites, and
/// keeps the dependency list in PURPLE_TEAM.md §P5.4 exactly as pinned.
fn pem_block(pem: &str, label: &str) -> Result<Vec<u8>, TlsError> {
    let begin = format!("-----BEGIN {label}-----");
    let end = format!("-----END {label}-----");
    let start = pem
        .find(&begin)
        .ok_or_else(|| TlsError::Pem(format!("no {begin} in the supplied PEM")))?
        + begin.len();
    let stop = pem[start..]
        .find(&end)
        .ok_or_else(|| TlsError::Pem(format!("no {end} in the supplied PEM")))?
        + start;

    let body: String = pem[start..stop]
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect();
    base64_decode(&body).ok_or_else(|| TlsError::Pem(format!("{label} body is not valid base64")))
}

/// Standard base64 (RFC 4648) decode. `base64` is already a dependency of
/// this crate for the legacy photo path, so use it rather than hand-rolling.
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.decode(input).ok()
}

/// Turn an [`IssuedLeaf`] into the rustls type the resolver hands back.
fn certified_key(leaf: &IssuedLeaf) -> Result<Arc<CertifiedKey>, TlsError> {
    let cert_der = CertificateDer::from(pem_block(&leaf.cert_pem, "CERTIFICATE")?);
    let key_der = PrivateKeyDer::try_from(pem_block(&leaf.key_pem, "PRIVATE KEY")?)
        .map_err(|err| TlsError::Pem(format!("private key is not a PKCS#8 key: {err}")))?;

    let provider = rustls::crypto::CryptoProvider::get_default()
        .cloned()
        .unwrap_or_else(|| Arc::new(rustls::crypto::ring::default_provider()));

    Ok(Arc::new(CertifiedKey::from_der(
        vec![cert_der.into_owned()],
        key_der,
        provider.as_ref(),
    )?))
}

/// A bound HTTPS listener plus the resolver that feeds it certificates.
pub struct TlsListener {
    pub local_addr: SocketAddr,
    listener: TcpListener,
    acceptor: TlsAcceptor,
    resolver: Arc<HotReloadResolver>,
}

impl TlsListener {
    /// Bind `addr` and build a rustls `ServerConfig` seeded from `certs`.
    pub async fn bind(addr: SocketAddr, certs: &dyn CertProvider) -> Result<TlsListener, TlsError> {
        install_crypto_provider();

        let resolver = Arc::new(HotReloadResolver::new(&certs.current())?);
        let mut config = ServerConfig::builder()
            .with_no_client_auth()
            .with_cert_resolver(resolver.clone());
        // h2 first: phones negotiate it and the whiteboard's WebSocket
        // upgrade still works because `serve_connection_with_upgrades`
        // handles the h1 fallback path.
        config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

        let listener = TcpListener::bind(addr).await?;
        let local_addr = listener.local_addr()?;

        Ok(TlsListener {
            local_addr,
            listener,
            acceptor: TlsAcceptor::from(Arc::new(config)),
            resolver,
        })
    }

    /// The resolver backing this listener; hand it to the renewal task so a
    /// re-issued leaf reaches live connections without a restart.
    pub fn resolver(&self) -> Arc<HotReloadResolver> {
        self.resolver.clone()
    }

    /// Accept forever, serving `router` over TLS. Each connection is
    /// spawned, and a failed handshake (a phone that has not installed the
    /// CA, a port scanner) is logged at debug and dropped — never fatal.
    pub async fn serve(self, router: Router) {
        let TlsListener {
            listener, acceptor, ..
        } = self;

        loop {
            let (stream, peer) = match listener.accept().await {
                Ok(accepted) => accepted,
                Err(err) => {
                    tracing::warn!(%err, "https accept failed");
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    continue;
                }
            };

            let acceptor = acceptor.clone();
            let router = router.clone();
            tokio::spawn(async move {
                let tls = match acceptor.accept(stream).await {
                    Ok(tls) => tls,
                    Err(err) => {
                        tracing::debug!(%peer, %err, "tls handshake failed");
                        return;
                    }
                };

                // axum's `Router` is already a `tower::Service<Request<B>>`
                // for any hyper body, so the whole adapter is hyper-util's
                // tower→hyper shim — no `axum-server` needed.
                let service = TowerToHyperService::new(router);

                if let Err(err) = AutoBuilder::new(TokioExecutor::new())
                    .serve_connection_with_upgrades(TokioIo::new(tls), service)
                    .await
                {
                    tracing::debug!(%peer, %err, "https connection ended");
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::pki::SelfSignedCa;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("familyhub-tls-unit-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn installing_the_ring_provider_twice_is_a_no_op() {
        install_crypto_provider();
        install_crypto_provider();
        assert!(
            rustls::crypto::CryptoProvider::get_default().is_some(),
            "a default crypto provider must be installed"
        );
    }

    #[test]
    fn a_leaf_pem_pair_becomes_a_rustls_certified_key() {
        install_crypto_provider();
        let dir = scratch("certified-key");
        let ca = SelfSignedCa::open(&dir).expect("open");
        let key = certified_key(&ca.current()).expect("the issued leaf loads into rustls");
        assert_eq!(key.cert.len(), 1, "one end-entity certificate in the chain");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_resolver_hands_back_whatever_leaf_was_last_installed() {
        install_crypto_provider();
        let dir = scratch("resolver-swap");
        let ca = SelfSignedCa::open(&dir).expect("open");

        let resolver = HotReloadResolver::new(&ca.current()).expect("seed resolver");
        let first = resolver
            .current
            .read()
            .expect("uncontended lock")
            .cert
            .clone();

        ca.reissue().expect("re-issue");
        resolver
            .replace(&ca.current())
            .expect("swap in the new leaf");
        let second = resolver
            .current
            .read()
            .expect("uncontended lock")
            .cert
            .clone();

        assert_ne!(
            first, second,
            "the resolver must serve the re-issued certificate after replace()"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pem_block_rejects_a_missing_label() {
        let err = pem_block("not a pem at all", "CERTIFICATE")
            .expect_err("a missing BEGIN line must be an error, not an empty chain");
        assert!(matches!(err, TlsError::Pem(_)));
    }
}
