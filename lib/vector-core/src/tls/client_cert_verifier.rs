//! Application-level authorization of TLS client certificates beyond CA chain validation.

use std::panic::{AssertUnwindSafe, catch_unwind};

use openssl::{
    stack::StackRef,
    x509::{X509, X509StoreContextRef, X509VerifyResult},
};

/// Decides whether a TLS server accepts a client certificate.
///
/// Runs once per handshake, only for chains OpenSSL has validated, but before name constraint and
/// policy checks, so it must not rely on those. Must be fast and non-blocking; a panic rejects the
/// client.
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
    // Never overrule OpenSSL's rejection.
    if !preverified {
        return false;
    }
    // CA certs: keep OpenSSL's passing verdict.
    if context.error_depth() != 0 {
        return true;
    }
    // Leaf only: ask the `ClientCertVerifier`.
    // Unwinding across the FFI boundary would abort the process.
    let accepted = context.chain().is_some_and(|chain| {
        catch_unwind(AssertUnwindSafe(|| verifier.verify(chain))).unwrap_or_else(|payload| {
            let panic = payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("unknown");
            error!(
                message = "Client certificate verifier panicked; rejecting client.",
                %panic,
            );
            false
        })
    });
    if accepted {
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
        sync::{Arc, Mutex},
    };

    use openssl::{
        ssl::{
            SslConnector, SslConnectorBuilder, SslFiletype, SslMethod, SslOptions, SslSession,
            SslVersion,
        },
        stack::StackRef,
        x509::{X509, X509VerifyResult},
    };
    use tokio::{io::AsyncWriteExt as _, net::TcpStream, task::JoinHandle};
    use tokio_openssl::SslStream;

    use super::ClientCertVerifier;
    use crate::tls::{
        MaybeTls, MaybeTlsSettings, TEST_PEM_CA_PATH, TEST_PEM_CLIENT_CRT_PATH,
        TEST_PEM_CLIENT_KEY_PATH, TEST_PEM_CRT_PATH, TEST_PEM_KEY_PATH, TlsAcceptorReloader,
        TlsConfig, TlsEnableableConfig, TlsError, TlsSettings,
        test_util::{extract_common_name, self_signed_ca},
    };

    /// A verifier that records the common names of every chain it sees, leaf first.
    struct RecordingVerifier {
        accept: bool,
        chains: Mutex<Vec<Vec<String>>>,
    }

    impl RecordingVerifier {
        fn new(accept: bool) -> Arc<Self> {
            Arc::new(Self {
                accept,
                chains: Mutex::default(),
            })
        }

        fn chains(&self) -> Vec<Vec<String>> {
            self.chains.lock().unwrap().clone()
        }
    }

    impl ClientCertVerifier for RecordingVerifier {
        fn verify(&self, chain: &StackRef<X509>) -> bool {
            let names = chain.iter().map(extract_common_name).collect();
            self.chains.lock().unwrap().push(names);
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

    fn connector(client: Client) -> SslConnector {
        connector_builder(client).build()
    }

    /// A trusted client that resumes sessions by session ID only.
    fn session_id_connector() -> SslConnector {
        let mut builder = connector_builder(Client::Trusted);
        builder.set_options(SslOptions::NO_TICKET);
        builder.build()
    }

    fn connector_builder(client: Client) -> SslConnectorBuilder {
        let mut builder = SslConnector::builder(SslMethod::tls()).unwrap();
        builder.set_ca_file(TEST_PEM_CA_PATH).unwrap();
        // Under TLS 1.2, the session is available as soon as the handshake completes.
        builder
            .set_max_proto_version(Some(SslVersion::TLS1_2))
            .unwrap();
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
                let (cert, key) = self_signed_ca("localhost");
                builder.set_certificate(&cert).unwrap();
                builder.set_private_key(&key).unwrap();
            }
            Client::Anonymous => {}
        }
        builder
    }

    /// Runs one client handshake, offering `session` for resumption. The server decides whether
    /// the handshake succeeded: see [`serve`].
    async fn connect(
        connector: &SslConnector,
        addr: SocketAddr,
        session: Option<&SslSession>,
    ) -> SslStream<TcpStream> {
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
        let mut stream = SslStream::new(ssl, tcp).unwrap();
        Pin::new(&mut stream).connect().await.ok();
        // OpenSSL never resumes the session of a connection that was not shut down cleanly.
        stream.shutdown().await.ok();
        stream
    }

    /// Serves `connections` connections with `settings`, or with the acceptor `reloader` holds,
    /// and returns the outcome of each server-side handshake. The server side is the reliable
    /// signal: the server may reject a client after the client's side of the handshake completes.
    async fn serve(
        settings: &TlsSettings,
        reloader: Option<TlsAcceptorReloader>,
        connections: usize,
    ) -> (SocketAddr, JoinHandle<Vec<crate::tls::Result<()>>>) {
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let mut listener = MaybeTlsSettings::Tls(settings.clone())
            .bind_reloadable(&addr, reloader)
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let mut outcomes = Vec::with_capacity(connections);
            for _ in 0..connections {
                let mut stream = listener.accept().await.unwrap();
                let outcome = stream.handshake().await;

                if outcome.is_ok() {
                    // Otherwise, OpenSSL evicts the session from the server's session cache.
                    stream.shutdown().await.ok();
                }
                outcomes.push(outcome);
            }
            outcomes
        });
        (addr, server)
    }

    /// Serves one connection from `client` and returns the server-side handshake outcome.
    async fn server_handshake(
        settings: &TlsSettings,
        reloader: Option<TlsAcceptorReloader>,
        client: Client,
    ) -> crate::tls::Result<()> {
        let (addr, server) = serve(settings, reloader, 1).await;
        connect(&connector(client), addr, None).await;
        server.await.unwrap().pop().unwrap()
    }

    #[tokio::test]
    async fn accepting_verifier_sees_the_verified_chain_leaf_first() {
        let verifier = RecordingVerifier::new(true);
        let settings = server_settings().with_client_cert_verifier(verifier.clone());

        server_handshake(&settings, None, Client::Trusted)
            .await
            .unwrap();
        // Runs once, for the leaf only, and sees the leaf, intermediate CA, and root CA.
        assert_eq!(
            verifier.chains(),
            [["localhost", "Vector Intermediate Client CA", "Vector CA"]]
        );
    }

    #[tokio::test]
    async fn rejecting_verifier_fails_the_handshake() {
        let verifier = RecordingVerifier::new(false);
        let settings = server_settings().with_client_cert_verifier(verifier.clone());

        let error = server_handshake(&settings, None, Client::Trusted)
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
        assert_eq!(verifier.chains().len(), 1);
    }

    #[tokio::test]
    async fn panicking_verifier_rejects_the_client() {
        let settings =
            server_settings().with_client_cert_verifier(Arc::new(|_: &StackRef<X509>| -> bool {
                panic!("verifier panicked")
            }));

        let error = server_handshake(&settings, None, Client::Trusted)
            .await
            .expect_err("a panicking verifier rejects the client");
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
    }

    #[tokio::test]
    async fn server_survives_a_panicking_verifier() {
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = Arc::clone(&calls);
        let settings =
            server_settings().with_client_cert_verifier(Arc::new(move |_: &StackRef<X509>| {
                let previous = counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                assert!(previous > 0, "first call panics");
                true
            }));

        let (addr, server) = serve(&settings, None, 2).await;
        let connector = connector(Client::Trusted);
        connect(&connector, addr, None).await;
        connect(&connector, addr, None).await;

        let outcomes = server.await.unwrap();
        assert!(outcomes[0].is_err() && outcomes[1].is_ok(), "{outcomes:?}");
    }

    #[tokio::test]
    async fn verifier_never_runs_for_an_untrusted_chain() {
        let verifier = RecordingVerifier::new(true);
        let settings = server_settings().with_client_cert_verifier(verifier.clone());

        let error = server_handshake(&settings, None, Client::Untrusted)
            .await
            .expect_err("the chain is untrusted");
        assert_rejected_by_openssl(&error);
        assert_eq!(verifier.chains().len(), 0);
    }

    fn assert_rejected_by_openssl(error: &TlsError) {
        assert!(
            matches!(
                error,
                TlsError::PeerCertificateRejected { reason, .. }
                    if *reason != X509VerifyResult::OK
                        && *reason != X509VerifyResult::APPLICATION_VERIFICATION
            ),
            "unexpected error: {error}"
        );
    }

    #[tokio::test]
    async fn untrusted_chain_without_a_verifier_is_reported_as_rejected() {
        let settings = server_settings_from(TlsConfig {
            verify_certificate: Some(true),
            ..server_config()
        });

        let error = server_handshake(&settings, None, Client::Untrusted)
            .await
            .expect_err("the chain is untrusted");
        assert_rejected_by_openssl(&error);
    }

    #[tokio::test]
    async fn missing_client_certificate_is_a_handshake_error() {
        let settings = server_settings_from(TlsConfig {
            verify_certificate: Some(true),
            ..server_config()
        });

        let error = server_handshake(&settings, None, Client::Anonymous)
            .await
            .expect_err("a client certificate is required");
        assert!(
            matches!(error, TlsError::Handshake { .. }),
            "unexpected error: {error}"
        );
    }

    #[tokio::test]
    async fn verifier_requires_a_client_certificate() {
        let verifier = RecordingVerifier::new(true);
        let settings = server_settings().with_client_cert_verifier(verifier.clone());

        assert!(
            server_handshake(&settings, None, Client::Anonymous)
                .await
                .is_err()
        );
        assert_eq!(verifier.chains().len(), 0);
    }

    #[tokio::test]
    async fn anonymous_client_is_accepted_without_a_verifier() {
        // Control for `verifier_requires_a_client_certificate`: the same settings without a
        // verifier do not request client certificates.
        server_handshake(&server_settings(), None, Client::Anonymous)
            .await
            .unwrap();
    }

    /// Connects twice, offering the first connection's session on the second, and returns
    /// whether the server resumed it.
    async fn resumes_session(settings: &TlsSettings, connector: &SslConnector) -> bool {
        let (addr, server) = serve(settings, None, 2).await;
        let first = connect(connector, addr, None).await;
        let session = first.ssl().session().unwrap().to_owned();
        let second = connect(connector, addr, Some(&session)).await;

        assert!(server.await.unwrap().iter().all(Result::is_ok));
        second.ssl().session_reused()
    }

    #[tokio::test]
    async fn resumed_session_does_not_bypass_the_verifier() {
        for connector in [connector(Client::Trusted), session_id_connector()] {
            let verifier = RecordingVerifier::new(true);
            let settings = server_settings().with_client_cert_verifier(verifier.clone());

            assert!(
                !resumes_session(&settings, &connector).await,
                "resuming a session would skip the verifier"
            );
            assert_eq!(verifier.chains().len(), 2);
        }
    }

    #[tokio::test]
    async fn session_id_is_resumed_without_a_verifier() {
        // Control for `resumed_session_does_not_bypass_the_verifier`.
        assert!(resumes_session(&server_settings(), &session_id_connector()).await);
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

    #[test]
    fn verifier_rejects_system_roots() {
        let settings = server_settings()
            .with_system_roots()
            .with_client_cert_verifier(RecordingVerifier::new(true));

        assert!(matches!(
            settings.acceptor(),
            Err(TlsError::ClientCertVerifierWithSystemRoots)
        ));
    }

    #[tokio::test]
    async fn reload_without_a_verifier_keeps_it() {
        let verifier = RecordingVerifier::new(false);
        let reloader =
            MaybeTlsSettings::Tls(server_settings().with_client_cert_verifier(verifier.clone()))
                .reloadable_acceptor()
                .unwrap()
                .unwrap();
        reloader.reload(&server_settings()).unwrap();
        reloader.reload(&server_settings()).unwrap();

        assert!(
            server_handshake(&server_settings(), Some(reloader), Client::Trusted)
                .await
                .is_err()
        );
        assert_eq!(verifier.chains().len(), 1);
    }

    #[tokio::test]
    async fn reload_without_a_verifier_keeps_the_replaced_one() {
        let old = RecordingVerifier::new(true);
        let new = RecordingVerifier::new(false);
        let reloader =
            MaybeTlsSettings::Tls(server_settings().with_client_cert_verifier(old.clone()))
                .reloadable_acceptor()
                .unwrap()
                .unwrap();
        reloader
            .reload(&server_settings().with_client_cert_verifier(new.clone()))
            .unwrap();
        reloader.reload(&server_settings()).unwrap();

        assert!(
            server_handshake(&server_settings(), Some(reloader), Client::Trusted)
                .await
                .is_err()
        );
        assert_eq!(old.chains().len(), 0);
        assert_eq!(new.chains().len(), 1);
    }

    #[test]
    fn reload_without_a_verifier_or_a_ca_fails() {
        let reloader = MaybeTlsSettings::Tls(
            server_settings().with_client_cert_verifier(RecordingVerifier::new(true)),
        )
        .reloadable_acceptor()
        .unwrap()
        .unwrap();
        let without_ca = server_settings_from(TlsConfig {
            ca_file: None,
            ..server_config()
        });

        assert!(matches!(
            reloader.reload(&without_ca),
            Err(TlsError::ClientCertVerifierWithoutCa)
        ));
    }

    #[tokio::test]
    async fn reload_replaces_the_verifier() {
        let old = RecordingVerifier::new(false);
        let new = RecordingVerifier::new(true);
        let reloader =
            MaybeTlsSettings::Tls(server_settings().with_client_cert_verifier(old.clone()))
                .reloadable_acceptor()
                .unwrap()
                .unwrap();
        reloader
            .reload(&server_settings().with_client_cert_verifier(new.clone()))
            .unwrap();

        server_handshake(&server_settings(), Some(reloader), Client::Trusted)
            .await
            .unwrap();
        assert_eq!(old.chains().len(), 0);
        assert_eq!(new.chains().len(), 1);
    }

    #[tokio::test]
    async fn reload_adds_a_verifier() {
        let verifier = RecordingVerifier::new(false);
        let reloader = MaybeTlsSettings::Tls(server_settings())
            .reloadable_acceptor()
            .unwrap()
            .unwrap();
        reloader
            .reload(&server_settings().with_client_cert_verifier(verifier.clone()))
            .unwrap();

        assert!(
            server_handshake(&server_settings(), Some(reloader), Client::Trusted)
                .await
                .is_err()
        );
        assert_eq!(verifier.chains().len(), 1);
    }

    #[tokio::test]
    async fn failed_reload_keeps_the_verifier() {
        let old = RecordingVerifier::new(false);
        let reloader =
            MaybeTlsSettings::Tls(server_settings().with_client_cert_verifier(old.clone()))
                .reloadable_acceptor()
                .unwrap()
                .unwrap();
        let invalid = server_settings_from(TlsConfig {
            ca_file: None,
            ..server_config()
        })
        .with_client_cert_verifier(RecordingVerifier::new(true));
        assert!(reloader.reload(&invalid).is_err());

        assert!(
            server_handshake(&server_settings(), Some(reloader), Client::Trusted)
                .await
                .is_err()
        );
        assert_eq!(old.chains().len(), 1);
    }
}
