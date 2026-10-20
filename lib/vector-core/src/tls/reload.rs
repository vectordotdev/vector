//! A TLS acceptor that can be swapped at runtime.
//!
//! A [`TlsAcceptorReloader`] wraps the acceptor a bound [`MaybeTlsListener`](super::MaybeTlsListener)
//! serves. Handing the same reloader to
//! [`MaybeTlsSettings::bind_reloadable`](super::MaybeTlsSettings::bind_reloadable) lets a background
//! task swap in freshly built material with [`reload`](TlsAcceptorReloader::reload); each new
//! connection then handshakes with the latest acceptor while in-flight connections keep what they
//! negotiated.

use std::sync::{Arc, Weak};

use arc_swap::ArcSwap;
use openssl::ssl::SslAcceptor;

use super::{MaybeTlsSettings, TlsSettings};

/// A cloneable handle to a server TLS acceptor that can be swapped at runtime.
///
/// Connections accepted after [`reload`](Self::reload) use the new acceptor; connections already
/// established keep whatever they negotiated at handshake time.
#[derive(Clone)]
pub struct TlsAcceptorReloader {
    acceptor: Arc<ArcSwap<SslAcceptor>>,
}

impl TlsAcceptorReloader {
    /// Wrap an initial acceptor in a swappable cell.
    pub(super) fn new(acceptor: SslAcceptor) -> Self {
        Self {
            acceptor: Arc::new(ArcSwap::from_pointee(acceptor)),
        }
    }

    /// The shared cell the bound listener reads from on each accept.
    pub(super) fn shared(&self) -> Arc<ArcSwap<SslAcceptor>> {
        Arc::clone(&self.acceptor)
    }

    /// Swap in a freshly built acceptor from `settings`. New connections pick it up
    /// immediately; the previous acceptor is dropped once its last in-flight handshake completes.
    pub fn reload(&self, settings: &TlsSettings) -> crate::tls::Result<()> {
        self.acceptor.store(Arc::new(settings.acceptor()?));
        Ok(())
    }

    /// Downgrade to a [`WeakTlsAcceptorReloader`] that does not keep the served acceptor alive.
    pub fn downgrade(&self) -> WeakTlsAcceptorReloader {
        WeakTlsAcceptorReloader {
            acceptor: Arc::downgrade(&self.acceptor),
        }
    }
}

/// A non-owning handle to a served TLS acceptor, obtained from [`TlsAcceptorReloader::downgrade`].
#[derive(Clone)]
pub struct WeakTlsAcceptorReloader {
    acceptor: Weak<ArcSwap<SslAcceptor>>,
}

impl WeakTlsAcceptorReloader {
    /// Return the live [`TlsAcceptorReloader`], or `None` once the bound listener has been dropped.
    pub fn upgrade(&self) -> Option<TlsAcceptorReloader> {
        self.acceptor
            .upgrade()
            .map(|acceptor| TlsAcceptorReloader { acceptor })
    }
}

impl MaybeTlsSettings {
    /// Build a reloadable acceptor handle for server use, or `None` when TLS is disabled.
    ///
    /// Pass the returned handle to [`bind_reloadable`](Self::bind_reloadable) so the bound listener
    /// serves it, and keep a clone to call [`reload`](TlsAcceptorReloader::reload) when the
    /// certificate material rotates.
    pub fn reloadable_acceptor(&self) -> crate::tls::Result<Option<TlsAcceptorReloader>> {
        match self {
            Self::Tls(tls) => Ok(Some(TlsAcceptorReloader::new(tls.acceptor()?))),
            Self::Raw(()) => Ok(None),
        }
    }
}

#[cfg(test)]
mod test {
    use std::{net::SocketAddr, pin::Pin};

    use openssl::{
        asn1::Asn1Time,
        bn::{BigNum, MsbOption},
        hash::MessageDigest,
        nid::Nid,
        pkey::PKey,
        rsa::Rsa,
        ssl::{SslConnector, SslMethod, SslVerifyMode},
        x509::{X509, X509NameBuilder},
    };

    use crate::tls::{MaybeTls, MaybeTlsSettings, TlsConfig, TlsEnableableConfig};

    #[test]
    fn no_reloadable_acceptor_without_tls() {
        assert!(
            MaybeTlsSettings::Raw(())
                .reloadable_acceptor()
                .unwrap()
                .is_none(),
            "plaintext settings have no acceptor to reload"
        );
    }

    #[tokio::test]
    async fn reloadable_acceptor_swaps_and_detects_shutdown() {
        let settings =
            MaybeTlsSettings::from_config(Some(&TlsEnableableConfig::test_config()), true).unwrap();
        let tls = match &settings {
            MaybeTls::Tls(tls) => tls.clone(),
            MaybeTls::Raw(()) => panic!("expected TLS to be enabled"),
        };

        let reloader = settings
            .reloadable_acceptor()
            .unwrap()
            .expect("tls enabled, so an acceptor should exist");
        let weak = reloader.downgrade();

        // Binding takes over the reloader's (sole) strong reference to the served acceptor.
        let addr = "127.0.0.1:0".parse().unwrap();
        let listener = settings
            .bind_reloadable(&addr, Some(reloader))
            .await
            .unwrap();

        weak.upgrade()
            .expect("listener alive, so the weak handle upgrades")
            .reload(&tls)
            .unwrap();

        // Once the listener (the last strong owner) is dropped, the weak handle no longer upgrades.
        drop(listener);
        assert!(
            weak.upgrade().is_none(),
            "weak handle must not upgrade after the listener is dropped"
        );
    }

    /// End-to-end: bind a reloadable TLS listener, complete a real handshake and confirm the served
    /// leaf certificate, then reload with a different certificate and confirm a fresh connection is
    /// served the new one.
    #[tokio::test]
    async fn served_certificate_changes_after_reload() {
        let (crt_a, key_a) = self_signed("old.example");
        let (crt_b, key_b) = self_signed("new.example");

        let settings = server_settings(&crt_a, &key_a);
        let reloader = settings
            .reloadable_acceptor()
            .unwrap()
            .expect("tls enabled, so an acceptor should exist");

        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let mut listener = settings
            .bind_reloadable(&addr, Some(reloader.clone()))
            .await
            .unwrap();
        let local_addr = listener.local_addr().unwrap();

        // Accept and complete the server side of each handshake until the test drops the listener.
        let server = tokio::spawn(async move {
            while let Ok(mut stream) = listener.accept().await {
                // The client may drop as soon as it has the cert, so a handshake error is expected.
                stream.handshake().await.ok();
            }
        });

        // Before any reload, the original certificate is served.
        assert_eq!(served_common_name(local_addr).await, "old.example");

        // Reload with a different certificate...
        let settings_b = server_settings(&crt_b, &key_b);
        let tls_b = match &settings_b {
            MaybeTls::Tls(tls) => tls.clone(),
            MaybeTls::Raw(()) => unreachable!(),
        };
        reloader.reload(&tls_b).unwrap();

        // ...and a new connection is served the rotated certificate.
        assert_eq!(served_common_name(local_addr).await, "new.example");

        server.abort();
    }

    /// Connect as a TLS client (trusting any server cert) and return the CN of the presented leaf.
    async fn served_common_name(addr: SocketAddr) -> String {
        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();

        let mut builder = SslConnector::builder(SslMethod::tls()).unwrap();
        builder.set_verify(SslVerifyMode::NONE);
        let mut config = builder.build().configure().unwrap();
        config.set_verify_hostname(false);
        let ssl = config.into_ssl("localhost").unwrap();

        let mut stream = tokio_openssl::SslStream::new(ssl, tcp).unwrap();
        Pin::new(&mut stream).connect().await.unwrap();

        let cert = stream
            .ssl()
            .peer_certificate()
            .expect("server presents a certificate");
        cert.subject_name()
            .entries_by_nid(Nid::COMMONNAME)
            .next()
            .unwrap()
            .data()
            .to_string()
            .unwrap()
    }

    fn server_settings(crt_pem: &str, key_pem: &str) -> MaybeTlsSettings {
        // `crt_file`/`key_file` accept inline PEM (detected by the `-----BEGIN ` marker), so no
        // temp files are needed.
        let config = TlsEnableableConfig {
            enabled: Some(true),
            options: TlsConfig {
                crt_file: Some(crt_pem.into()),
                key_file: Some(key_pem.into()),
                ..Default::default()
            },
        };
        MaybeTlsSettings::from_config(Some(&config), true).unwrap()
    }

    /// Generate a self-signed certificate/key pair (PEM) with the given common name.
    fn self_signed(common_name: &str) -> (String, String) {
        let key = PKey::from_rsa(Rsa::generate(2048).unwrap()).unwrap();

        let mut name = X509NameBuilder::new().unwrap();
        name.append_entry_by_text("CN", common_name).unwrap();
        let name = name.build();

        let mut serial = BigNum::new().unwrap();
        serial.rand(128, MsbOption::MAYBE_ZERO, false).unwrap();

        let mut builder = X509::builder().unwrap();
        builder.set_version(2).unwrap();
        builder
            .set_serial_number(&serial.to_asn1_integer().unwrap())
            .unwrap();
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
        let cert = builder.build();

        (
            String::from_utf8(cert.to_pem().unwrap()).unwrap(),
            String::from_utf8(key.private_key_to_pem_pkcs8().unwrap()).unwrap(),
        )
    }
}
