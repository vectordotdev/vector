//! Application-level authorization of TLS client certificates.
//!
//! Chain validation against the configured CA only establishes that a client holds *some*
//! certificate issued by that CA. When a CA is shared by many workloads, a server may need to
//! restrict which of those certificates it accepts. A [`ClientCertVerifier`] installed with
//! [`TlsSettings::with_client_cert_verifier`](super::TlsSettings::with_client_cert_verifier)
//! makes that decision during the handshake.

use openssl::{stack::StackRef, x509::X509, x509::X509StoreContextRef};

/// Decides whether a TLS server accepts a client certificate.
///
/// It runs during the handshake, once per connection, and only after OpenSSL has validated the
/// client's certificate chain (trust anchor, validity period, signatures). It never relaxes that
/// validation: a client whose chain fails to verify is rejected without calling the verifier.
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
    context.chain().is_some_and(|chain| verifier.verify(chain))
}

#[cfg(test)]
mod tests {
    use std::{
        net::SocketAddr,
        pin::Pin,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use openssl::{
        nid::Nid,
        ssl::{SslConnector, SslFiletype, SslMethod},
        stack::StackRef,
        x509::X509,
    };
    use tokio::net::TcpStream;

    use super::ClientCertVerifier;
    use crate::tls::{
        MaybeTls, MaybeTlsSettings, TEST_PEM_CA_PATH, TEST_PEM_CLIENT_CRT_PATH,
        TEST_PEM_CLIENT_KEY_PATH, TEST_PEM_CRT_PATH, TEST_PEM_KEY_PATH, TlsConfig,
        TlsEnableableConfig, TlsSettings,
    };

    /// A verifier that records how often it ran and the chain it last saw.
    #[derive(Default)]
    struct RecordingVerifier {
        accept: bool,
        calls: AtomicUsize,
        leaf_common_name: std::sync::Mutex<Option<String>>,
        chain_len: AtomicUsize,
    }

    impl RecordingVerifier {
        fn new(accept: bool) -> Arc<Self> {
            Arc::new(Self {
                accept,
                ..Self::default()
            })
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    impl ClientCertVerifier for RecordingVerifier {
        fn verify(&self, chain: &StackRef<X509>) -> bool {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.chain_len.store(chain.len(), Ordering::SeqCst);
            *self.leaf_common_name.lock().unwrap() = chain.iter().next().map(common_name);
            self.accept
        }
    }

    fn common_name(cert: &openssl::x509::X509Ref) -> String {
        cert.subject_name()
            .entries_by_nid(Nid::COMMONNAME)
            .next()
            .unwrap()
            .data()
            .to_string()
            .unwrap()
    }

    /// Server settings trusting the test CA. `verify_certificate` is left unset (off by default
    /// for servers): installing a verifier must request and require client certificates anyway.
    fn server_settings() -> TlsSettings {
        let config = TlsEnableableConfig {
            enabled: Some(true),
            options: TlsConfig {
                ca_file: Some(TEST_PEM_CA_PATH.into()),
                crt_file: Some(TEST_PEM_CRT_PATH.into()),
                key_file: Some(TEST_PEM_KEY_PATH.into()),
                ..Default::default()
            },
        };
        match MaybeTlsSettings::from_config(Some(&config), true).unwrap() {
            MaybeTls::Tls(settings) => settings,
            MaybeTls::Raw(()) => unreachable!("TLS is enabled"),
        }
    }

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
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let mut listener = MaybeTlsSettings::Tls(settings.clone())
            .bind(&addr)
            .await
            .unwrap();
        let local_addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let mut stream = listener.accept().await.unwrap();
            stream.handshake().await.is_ok()
        });
        connect(local_addr, client).await;
        server.await.unwrap()
    }

    async fn connect(addr: SocketAddr, client: Client) {
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
                let (cert, key) = self_signed_identity();
                builder.set_certificate(&cert).unwrap();
                builder.set_private_key(&key).unwrap();
            }
            Client::Anonymous => {}
        }
        let ssl = builder
            .build()
            .configure()
            .unwrap()
            .into_ssl("localhost")
            .unwrap();
        let tcp = TcpStream::connect(addr).await.unwrap();
        let mut stream = tokio_openssl::SslStream::new(ssl, tcp).unwrap();
        // Rejections surface on either side depending on the TLS version; the server decides.
        Pin::new(&mut stream).connect().await.ok();
        tokio::io::AsyncReadExt::read(&mut stream, &mut [0; 1])
            .await
            .ok();
    }

    fn self_signed_identity() -> (X509, openssl::pkey::PKey<openssl::pkey::Private>) {
        use openssl::{
            asn1::Asn1Time, hash::MessageDigest, pkey::PKey, rsa::Rsa, x509::X509NameBuilder,
        };

        let key = PKey::from_rsa(Rsa::generate(2048).unwrap()).unwrap();
        let mut name = X509NameBuilder::new().unwrap();
        name.append_entry_by_text("CN", "localhost").unwrap();
        let name = name.build();
        let mut builder = X509::builder().unwrap();
        builder.set_version(2).unwrap();
        builder.set_subject_name(&name).unwrap();
        builder.set_issuer_name(&name).unwrap();
        builder.set_pubkey(&key).unwrap();
        builder
            .set_not_before(&Asn1Time::days_from_now(0).unwrap())
            .unwrap();
        builder
            .set_not_after(&Asn1Time::days_from_now(1).unwrap())
            .unwrap();
        builder.sign(&key, MessageDigest::sha256()).unwrap();
        (builder.build(), key)
    }

    #[tokio::test]
    async fn accepting_verifier_sees_the_verified_chain_leaf_first() {
        let verifier = RecordingVerifier::new(true);
        let settings = server_settings().with_client_cert_verifier(verifier.clone());

        assert!(server_accepts(&settings, Client::Trusted).await);
        assert_eq!(verifier.calls(), 1, "runs once, for the leaf only");
        assert_eq!(
            verifier.leaf_common_name.lock().unwrap().as_deref(),
            Some("localhost")
        );
        // Leaf, intermediate CA, and root CA.
        assert_eq!(verifier.chain_len.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn rejecting_verifier_fails_the_handshake() {
        let verifier = RecordingVerifier::new(false);
        let settings = server_settings().with_client_cert_verifier(verifier.clone());

        assert!(!server_accepts(&settings, Client::Trusted).await);
        assert_eq!(verifier.calls(), 1);
    }

    #[tokio::test]
    async fn verifier_never_runs_for_an_untrusted_chain() {
        let verifier = RecordingVerifier::new(true);
        let settings = server_settings().with_client_cert_verifier(verifier.clone());

        assert!(!server_accepts(&settings, Client::Untrusted).await);
        assert_eq!(verifier.calls(), 0);
    }

    #[tokio::test]
    async fn verifier_requires_a_client_certificate() {
        let verifier = RecordingVerifier::new(true);
        let settings = server_settings().with_client_cert_verifier(verifier.clone());

        assert!(!server_accepts(&settings, Client::Anonymous).await);
        assert_eq!(verifier.calls(), 0);
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
    async fn reloaded_acceptor_applies_the_verifier_of_its_settings() {
        let verifier = RecordingVerifier::new(false);
        let settings = server_settings();
        let reloader = MaybeTlsSettings::Tls(settings.clone())
            .reloadable_acceptor()
            .unwrap()
            .unwrap();
        reloader
            .reload(&settings.with_client_cert_verifier(verifier.clone()))
            .unwrap();

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

        assert!(!server.await.unwrap());
        assert_eq!(verifier.calls(), 1);
    }
}
