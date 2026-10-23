//! Application-level authorization of TLS client certificates.
//!
//! Chain validation against the configured CA only establishes that a client holds *some*
//! certificate issued by that CA. When a CA is shared by many workloads, a server may need to
//! restrict which of those certificates it accepts. A [`ClientCertVerifier`] installed with
//! [`TlsSettings::with_client_cert_verifier`](super::TlsSettings::with_client_cert_verifier)
//! makes that decision during the handshake.

use openssl::{
    stack::StackRef,
    x509::{X509, X509StoreContextRef, X509VerifyResult},
};

/// Decides whether a TLS server accepts a client certificate.
///
/// It runs during the handshake, once per connection (session resumption is disabled while a
/// verifier is installed). OpenSSL calls it after building the client's chain to the trust anchor
/// and checking its extensions, signatures, and validity periods, but before checking name
/// constraints and certificate policies: a chain the verifier accepts can still be rejected by
/// those later checks, so it must not rely on them having passed. It never relaxes validation: a
/// client whose chain fails an earlier check is rejected without calling the verifier.
///
/// Implementations run on the connection's handshake path, so they must be fast and must not
/// block. They must not panic: the call crosses an FFI boundary, where a panic aborts the process.
pub trait ClientCertVerifier: Send + Sync {
    /// Returns whether to accept the client. `chain` is the verified chain, leaf first.
    fn verify(&self, chain: &StackRef<X509>) -> bool;
}

impl<F> ClientCertVerifier for F
where
    F: Fn(&StackRef<X509>) -> bool + Send + Sync,
{
    fn verify(&self, chain: &StackRef<X509>) -> bool {
        self(chain)
    }
}

/// The OpenSSL verify callback. OpenSSL calls it once per certificate in the chain, from the trust
/// anchor (highest depth) down to the leaf (depth 0); `preverified` is its own verdict for that
/// certificate.
pub(super) fn verify_client_cert(
    verifier: &dyn ClientCertVerifier,
    preverified: bool,
    context: &mut X509StoreContextRef,
) -> bool {
    if !preverified {
        return false;
    }
    // Only the leaf identifies the client. Above it, OpenSSL's verdict stands.
    if context.error_depth() != 0 {
        return true;
    }
    if context.chain().is_some_and(|chain| verifier.verify(chain)) {
        return true;
    }
    // Without an error code, OpenSSL reports an unspecified verification error.
    context.set_error(X509VerifyResult::APPLICATION_VERIFICATION);
    false
}

#[cfg(test)]
mod tests {
    use std::{
        net::SocketAddr,
        pin::Pin,
        sync::{Arc, Mutex, MutexGuard},
    };

    use openssl::{
        ssl::{
            SslConnector, SslConnectorBuilder, SslFiletype, SslMethod, SslSession,
            SslSessionCacheMode,
        },
        stack::StackRef,
        x509::{X509, X509VerifyResult},
    };
    use tokio::net::TcpStream;

    use super::ClientCertVerifier;
    use crate::tls::{
        MaybeTls, MaybeTlsSettings, TEST_PEM_CA_PATH, TEST_PEM_CLIENT_CRT_PATH,
        TEST_PEM_CLIENT_KEY_PATH, TEST_PEM_CRT_PATH, TEST_PEM_KEY_PATH, TlsAcceptorReloader,
        TlsConfig, TlsEnableableConfig, TlsError, TlsSettings,
        test_util::{common_name, self_signed},
    };

    /// A verifier that records how often it ran and the chain it last saw.
    #[derive(Default)]
    struct RecordingVerifier {
        accept: bool,
        inner: Mutex<RecordingVerifierInner>,
    }

    #[derive(Default)]
    struct RecordingVerifierInner {
        calls: usize,
        leaf_common_name: Option<String>,
        chain_len: usize,
    }

    impl RecordingVerifier {
        fn new(accept: bool) -> Arc<Self> {
            Arc::new(Self {
                accept,
                ..Self::default()
            })
        }

        fn inner(&self) -> MutexGuard<'_, RecordingVerifierInner> {
            self.inner.lock().unwrap()
        }
    }

    impl ClientCertVerifier for RecordingVerifier {
        fn verify(&self, chain: &StackRef<X509>) -> bool {
            let mut inner = self.inner();
            inner.calls += 1;
            inner.leaf_common_name = chain.iter().next().map(common_name);
            inner.chain_len = chain.len();
            self.accept
        }
    }

    /// Server settings trusting the test CA. `verify_certificate` is left unset (off by default
    /// for servers): installing a verifier must request and require client certificates anyway.
    fn server_settings() -> TlsSettings {
        server_settings_from(server_config())
    }

    fn server_config() -> TlsConfig {
        TlsConfig {
            ca_file: Some(TEST_PEM_CA_PATH.into()),
            crt_file: Some(TEST_PEM_CRT_PATH.into()),
            key_file: Some(TEST_PEM_KEY_PATH.into()),
            ..Default::default()
        }
    }

    fn server_settings_from(options: TlsConfig) -> TlsSettings {
        let config = TlsEnableableConfig {
            enabled: Some(true),
            options,
        };
        match MaybeTlsSettings::from_config(Some(&config), true).unwrap() {
            MaybeTls::Tls(settings) => settings,
            MaybeTls::Raw(()) => unreachable!("TLS should be enabled"),
        }
    }

    #[derive(Clone, Copy)]
    enum Client {
        /// Presents the test client certificate, issued under the test CA.
        Trusted,
        /// Presents a certificate the server's CA did not issue.
        Untrusted,
        /// Presents no certificate.
        Anonymous,
    }

    /// Serves one connection with `settings` and returns whether the server-side handshake
    /// succeeded. The server side is the reliable signal: under TLS 1.3 the client may finish its
    /// handshake before the server rejects its certificate.
    async fn server_accepts(settings: &TlsSettings, client: Client) -> bool {
        server_handshake(settings, client).await.is_ok()
    }

    /// Serves one connection with `settings` and returns the outcome of the server-side handshake.
    async fn server_handshake(settings: &TlsSettings, client: Client) -> crate::tls::Result<()> {
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let mut listener = MaybeTlsSettings::Tls(settings.clone())
            .bind(&addr)
            .await
            .unwrap();
        let local_addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let mut stream = listener.accept().await.unwrap();
            stream.handshake().await
        });
        connect(local_addr, client).await;
        server.await.unwrap()
    }

    async fn connect(addr: SocketAddr, client: Client) {
        handshake(&connector_builder(client).build(), addr, None).await;
    }

    fn connector_builder(client: Client) -> SslConnectorBuilder {
        let mut builder = SslConnector::builder(SslMethod::tls()).unwrap();
        builder.set_ca_file(TEST_PEM_CA_PATH).unwrap();
        match client {
            Client::Trusted => {
                builder
                    .set_certificate_chain_file(TEST_PEM_CLIENT_CRT_PATH)
                    .unwrap();
                builder
                    .set_private_key_file(TEST_PEM_CLIENT_KEY_PATH, SslFiletype::PEM)
                    .unwrap();
            }
            Client::Untrusted => {
                let (cert, key) = self_signed("localhost");
                builder.set_certificate(&cert).unwrap();
                builder.set_private_key(&key).unwrap();
            }
            Client::Anonymous => {}
        }
        builder
    }

    /// Runs one client handshake, offering `session` for resumption. Returns whether the session
    /// was resumed.
    async fn handshake(
        connector: &SslConnector,
        addr: SocketAddr,
        session: Option<&SslSession>,
    ) -> bool {
        let mut ssl = connector
            .configure()
            .unwrap()
            .into_ssl("localhost")
            .unwrap();
        if let Some(session) = session {
            // SAFETY: the session was issued to a connection of the same connector.
            unsafe { ssl.set_session(session).unwrap() };
        }
        let tcp = TcpStream::connect(addr).await.unwrap();
        let mut stream = tokio_openssl::SslStream::new(ssl, tcp).unwrap();
        // Rejections surface on either side depending on the TLS version; the server decides.
        Pin::new(&mut stream).connect().await.ok();
        // Also lets the client receive the server's TLS 1.3 session tickets.
        tokio::io::AsyncReadExt::read(&mut stream, &mut [0; 1])
            .await
            .ok();
        // OpenSSL marks the session of a connection that was not shut down cleanly unresumable.
        tokio::io::AsyncWriteExt::shutdown(&mut stream).await.ok();
        stream.ssl().session_reused()
    }

    /// Connects as `Client::Trusted` twice, offering on the second connection any session the
    /// server issued on the first. Returns whether the server accepted each handshake and whether
    /// the second one resumed the session.
    async fn server_accepts_reconnection(settings: &TlsSettings) -> (bool, bool, bool) {
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let mut listener = MaybeTlsSettings::Tls(settings.clone())
            .bind(&addr)
            .await
            .unwrap();
        let local_addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let mut accepted = [false; 2];
            for accepted in &mut accepted {
                let mut stream = listener.accept().await.unwrap();
                *accepted = stream.handshake().await.is_ok();
            }
            accepted
        });

        let session = Arc::new(Mutex::new(None));
        let mut builder = connector_builder(Client::Trusted);
        builder.set_session_cache_mode(SslSessionCacheMode::CLIENT);
        let issued = Arc::clone(&session);
        builder.set_new_session_callback(move |_, session| {
            *issued.lock().unwrap() = Some(session);
        });
        let connector = builder.build();

        handshake(&connector, local_addr, None).await;
        let session = session.lock().unwrap().take();
        let resumed = handshake(&connector, local_addr, session.as_ref()).await;
        let [first, second] = server.await.unwrap();
        (first, second, resumed)
    }

    #[tokio::test]
    async fn accepting_verifier_sees_the_verified_chain_leaf_first() {
        let verifier = RecordingVerifier::new(true);
        let settings = server_settings().with_client_cert_verifier(verifier.clone());

        assert!(server_accepts(&settings, Client::Trusted).await);
        let inner = verifier.inner();
        assert_eq!(inner.calls, 1, "runs once, for the leaf only");
        assert_eq!(inner.leaf_common_name.as_deref(), Some("localhost"));
        // Leaf, intermediate CA, and root CA.
        assert_eq!(inner.chain_len, 3);
    }

    #[tokio::test]
    async fn rejecting_verifier_fails_the_handshake() {
        let verifier = RecordingVerifier::new(false);
        let settings = server_settings().with_client_cert_verifier(verifier.clone());

        let error = server_handshake(&settings, Client::Trusted)
            .await
            .expect_err("the verifier rejects the client");
        assert!(
            matches!(
                error,
                TlsError::PeerCertificateRejected {
                    reason: X509VerifyResult::APPLICATION_VERIFICATION,
                    ..
                }
            ),
            "unexpected error: {error}"
        );
        assert_eq!(verifier.inner().calls, 1);
    }

    #[tokio::test]
    async fn verifier_never_runs_for_an_untrusted_chain() {
        let verifier = RecordingVerifier::new(true);
        let settings = server_settings().with_client_cert_verifier(verifier.clone());

        assert!(!server_accepts(&settings, Client::Untrusted).await);
        assert_eq!(verifier.inner().calls, 0);
    }

    #[tokio::test]
    async fn verifier_requires_a_client_certificate() {
        let verifier = RecordingVerifier::new(true);
        let settings = server_settings().with_client_cert_verifier(verifier.clone());

        assert!(!server_accepts(&settings, Client::Anonymous).await);
        assert_eq!(verifier.inner().calls, 0);
    }

    #[tokio::test]
    async fn verifier_runs_on_every_connection() {
        let verifier = RecordingVerifier::new(true);
        let settings = server_settings().with_client_cert_verifier(verifier.clone());

        let (first, second, resumed) = server_accepts_reconnection(&settings).await;
        assert!(first && second);
        assert!(!resumed, "resuming a session would skip the verifier");
        assert_eq!(verifier.inner().calls, 2);
    }

    #[tokio::test]
    async fn verified_client_can_resume_its_session() {
        let settings = server_settings_from(TlsConfig {
            verify_certificate: Some(true),
            ..server_config()
        });

        let (first, second, resumed) = server_accepts_reconnection(&settings).await;
        assert!(first && second);
        assert!(resumed);
    }

    #[test]
    fn verifier_requires_a_ca() {
        let settings = server_settings_from(TlsConfig {
            ca_file: None,
            ..server_config()
        })
        .with_client_cert_verifier(RecordingVerifier::new(true));

        assert!(matches!(
            settings.acceptor(),
            Err(TlsError::ClientCertVerifierWithoutCa)
        ));
    }

    #[tokio::test]
    async fn anonymous_client_is_accepted_without_a_verifier() {
        // Control for `verifier_requires_a_client_certificate`: the same settings without a
        // verifier do not request client certificates.
        assert!(server_accepts(&server_settings(), Client::Anonymous).await);
    }

    #[tokio::test]
    async fn closures_can_be_verifiers() {
        let settings = server_settings()
            .with_client_cert_verifier(Arc::new(|chain: &StackRef<X509>| !chain.is_empty()));

        assert!(server_accepts(&settings, Client::Trusted).await);
    }

    #[tokio::test]
    async fn reload_keeps_the_verifier() {
        let verifier = RecordingVerifier::new(false);
        let reloader =
            MaybeTlsSettings::Tls(server_settings().with_client_cert_verifier(verifier.clone()))
                .reloadable_acceptor()
                .unwrap()
                .unwrap();
        // Rotation rebuilds settings from configuration, without the verifier. Reloading through a
        // weak handle, as a background rotation task would, must keep it too.
        reloader
            .downgrade()
            .upgrade()
            .unwrap()
            .reload(&server_settings())
            .unwrap();

        assert!(!server_accepts_after_reload(reloader).await);
        assert_eq!(verifier.inner().calls, 1);
    }

    #[tokio::test]
    async fn reload_does_not_add_a_verifier() {
        let verifier = RecordingVerifier::new(false);
        let reloader = MaybeTlsSettings::Tls(server_settings())
            .reloadable_acceptor()
            .unwrap()
            .unwrap();
        reloader
            .reload(&server_settings().with_client_cert_verifier(verifier.clone()))
            .unwrap();

        assert!(server_accepts_after_reload(reloader).await);
        assert_eq!(verifier.inner().calls, 0);
    }

    /// Like [`server_accepts`] for `Client::Trusted`, serving the acceptor `reloader` holds.
    async fn server_accepts_after_reload(reloader: TlsAcceptorReloader) -> bool {
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let mut listener = MaybeTlsSettings::Tls(server_settings())
            .bind_reloadable(&addr, Some(reloader))
            .await
            .unwrap();
        let local_addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let mut stream = listener.accept().await.unwrap();
            stream.handshake().await.is_ok()
        });
        connect(local_addr, Client::Trusted).await;
        server.await.unwrap()
    }
}
