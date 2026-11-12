//! Functionality supporting both the `[crate::sources::amqp]` source and `[crate::sinks::amqp]` sink.

use crate::tls::TlsConfig;
use lapin::tcp::{OwnedIdentity, OwnedTLSConfig};
use std::error::Error;
use tracing::warn;
use vector_lib::configurable::configurable_component;

/// AMQP connection options.
#[configurable_component]
#[derive(Clone, Debug)]
pub(crate) struct AmqpConfig {
    /// URI for the AMQP server.
    ///
    /// The URI has the format of
    /// `amqp://<user>:<password>@<host>:<port>/<vhost>?timeout=<seconds>`.
    ///
    /// The default vhost can be specified by using a value of `%2f`.
    ///
    /// To connect over TLS, a scheme of `amqps` can be specified instead. For example,
    /// `amqps://...`. Additional TLS settings, such as client certificate verification, can be
    /// configured under the `tls` section.
    #[configurable(metadata(
        docs::examples = "amqp://user:password@127.0.0.1:5672/%2f?timeout=10",
    ))]
    pub(crate) connection_string: String,

    pub(crate) tls: Option<crate::tls::TlsConfig>,
}

impl Default for AmqpConfig {
    fn default() -> Self {
        Self {
            connection_string: "amqp://127.0.0.1/%2f".to_string(),
            tls: None,
        }
    }
}

/// Polls the connection until a connection can be made.
/// Gives up after 5 attempts.
#[cfg(feature = "amqp-integration-tests")]
#[cfg(test)]
pub(crate) async fn await_connection(connection: &AmqpConfig) {
    let mut pause = tokio::time::Duration::from_millis(1);
    let mut attempts = 0;

    loop {
        let connection = connection.clone();
        if connection.connect().await.is_ok() {
            return;
        }
        attempts += 1;

        if attempts == 5 {
            return;
        }

        tokio::time::sleep(pause).await;
        pause *= 2;
    }
}

impl AmqpConfig {
    pub(crate) async fn connect(
        &self,
    ) -> Result<(lapin::Connection, lapin::Channel), Box<dyn std::error::Error + Send + Sync>> {
        let addr = self.connection_string.clone();
        let conn = match &self.tls {
            Some(tls) => {
                let cert_chain = Self::build_ca_cert(tls)?;

                let identity = Self::build_identity(tls)?;

                let tls_config = OwnedTLSConfig {
                    identity,
                    cert_chain,
                };
                lapin::Connection::connect_with_config(
                    &addr,
                    lapin::ConnectionProperties::default(),
                    tls_config,
                    async_rs::Runtime::tokio_current(),
                )
                .await
            }
            None => lapin::Connection::connect(&addr, lapin::ConnectionProperties::default()).await,
        }?;
        let channel = conn.create_channel().await?;
        Ok((conn, channel))
    }

    fn build_ca_cert(tls: &TlsConfig) -> Result<Option<String>, Box<dyn Error + Send + Sync>> {
        let authorities = tls.load_authorities()?;

        let cert_chain = if authorities.is_empty() {
            None
        } else {
            let mut pem_chain = String::new();
            for cert in authorities {
                let bytes = cert.to_pem()?;
                let pem_str = std::str::from_utf8(&bytes)?;
                pem_chain.push_str(pem_str);
            }
            Some(pem_chain)
        };
        Ok(cert_chain)
    }

    fn build_identity(
        tls: &TlsConfig,
    ) -> Result<Option<OwnedIdentity>, Box<dyn Error + Send + Sync>> {
        // legacy fallback: for details see https://github.com/vectordotdev/vector/pull/27130#discussion_r4540620744
        if let (None, Some(path)) = (&tls.crt_file, &tls.key_file) {
            warn!(
                "DEPRECATED: Specifying a PKCS#12 archive in `tls.key_file` for an AMQP connection is deprecated, pass the archive via `tls.crt_file` instead."
            );
            return Ok(Some(OwnedIdentity::PKCS12 {
                der: std::fs::read(path)?,
                password: tls.key_pass.clone().unwrap_or_default(),
            }));
        }

        match tls.load_identity()? {
            Some(identity_store) => {
                let password = tls.key_pass.clone().unwrap_or_default();
                let der = identity_store.to_pkcs12(&password)?;
                Ok(Some(OwnedIdentity::PKCS12 { der, password }))
            }
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tls::{
        TEST_PEM_CA_PATH, TEST_PEM_CLIENT_CRT_PATH, TEST_PEM_CLIENT_KEY_PATH,
        TEST_PEM_INTERMEDIATE_CA_PATH,
    };
    use openssl::x509::X509;
    use vector_lib::tls::TlsConfig;

    #[test]
    fn build_ca_cert_from_pem_file() {
        let tls = TlsConfig {
            ca_file: Some(TEST_PEM_CA_PATH.into()),
            ..Default::default()
        };

        let ca_cert = AmqpConfig::build_ca_cert(&tls).expect("failed to build CA cert chain");
        assert!(ca_cert.is_some());

        let parsed_cert = X509::from_pem(ca_cert.unwrap().as_bytes())
            .expect("returned string must be a valid parseable X509 PEM");

        assert!(parsed_cert.subject_name().entries().count() > 0);
    }

    #[test]
    fn build_ca_cert_intermediate_pem_file() {
        let tls = TlsConfig {
            ca_file: Some(TEST_PEM_INTERMEDIATE_CA_PATH.into()),
            ..Default::default()
        };

        let ca_cert = AmqpConfig::build_ca_cert(&tls).expect("failed to build CA cert chain");
        assert!(ca_cert.is_some());

        let parsed_cert = X509::from_pem(ca_cert.unwrap().as_bytes())
            .expect("returned string must be a valid parseable X509 PEM");

        let subject_der = Result::unwrap(parsed_cert.subject_name().to_der());
        let issuer_der = Result::unwrap(parsed_cert.issuer_name().to_der());

        assert_ne!(
            subject_der, issuer_der,
            "Intermediate CA must have different Subject and Issuer"
        );
    }

    #[test]
    fn build_ca_cert_from_pem_inline() {
        let inline_pem =
            std::fs::read_to_string(TEST_PEM_CA_PATH).expect("failed to read test CA fixture");

        let tls = TlsConfig {
            ca_file: Some(inline_pem.into()),
            ..Default::default()
        };

        let ca_cert = AmqpConfig::build_ca_cert(&tls).expect("failed to build CA cert chain");
        assert!(ca_cert.is_some());

        let parsed_cert = X509::from_pem(ca_cert.unwrap().as_bytes())
            .expect("returned string must be a valid parseable X509 PEM");

        assert!(parsed_cert.subject_name().entries().count() > 0);
    }

    #[test]
    fn build_ca_cert_from_der_file() {
        let pem_bytes = std::fs::read(TEST_PEM_CA_PATH).expect("failed to read test CA fixture");
        let pem_cert = X509::from_pem(&pem_bytes).expect("fixture must be valid PEM");

        let mut temp_der_file = tempfile::NamedTempFile::new().expect("failed to create temp file");
        std::io::Write::write_all(&mut temp_der_file, &Result::unwrap(pem_cert.to_der()))
            .expect("failed to write DER bytes");

        let tls = TlsConfig {
            ca_file: Some(temp_der_file.path().into()),
            ..Default::default()
        };

        let ca_cert = AmqpConfig::build_ca_cert(&tls).expect("failed to build CA cert chain");
        assert!(ca_cert.is_some());

        let parsed_cert = X509::from_pem(ca_cert.unwrap().as_bytes())
            .expect("returned string must be a valid parseable X509 PEM");

        assert!(parsed_cert.subject_name().entries().count() > 0);
    }

    #[test]
    fn build_identity() {
        let tls = TlsConfig {
            ca_file: Some(TEST_PEM_INTERMEDIATE_CA_PATH.into()),
            crt_file: Some(TEST_PEM_CLIENT_CRT_PATH.into()),
            key_file: Some(TEST_PEM_CLIENT_KEY_PATH.into()),
            key_pass: Some("valid-password".into()),
            ..Default::default()
        };

        let identity = AmqpConfig::build_identity(&tls).expect("failed to build OwnerIdentity");

        let Some(OwnedIdentity::PKCS12 { der, password }) = identity else {
            panic!("expected OwnedIdentity::PKCS12")
        };

        let pkcs_12 = openssl::pkcs12::Pkcs12::from_der(&der)
            .expect("payload must be a valid DER-encoded PKCS#12 archive");

        let parsed_archive = pkcs_12
            .parse2(&password)
            .expect("PKCS#12 archive must decrypt cleanly with the actual password");

        assert!(
            parsed_archive.cert.is_some(),
            "archive must contain client certificate"
        );
        assert!(
            parsed_archive.pkey.is_some(),
            "archive must contain private key"
        );
        assert_eq!(
            parsed_archive.ca.map_or(0, |stack| stack.len()),
            2,
            "archive must contain two intermediate CA"
        );
    }

    #[test]
    fn build_identity_fails_with_wrong_password() {
        let tls = TlsConfig {
            ca_file: Some(TEST_PEM_INTERMEDIATE_CA_PATH.into()),
            crt_file: Some(TEST_PEM_CLIENT_CRT_PATH.into()),
            key_file: Some(TEST_PEM_CLIENT_KEY_PATH.into()),
            key_pass: Some("some-password".into()),
            ..Default::default()
        };

        let identity = AmqpConfig::build_identity(&tls).expect("failed to build identity");

        let Some(OwnedIdentity::PKCS12 { der, .. }) = identity else {
            panic!("expected OwnedIdentity::PKCS12")
        };

        let pkcs_12 = openssl::pkcs12::Pkcs12::from_der(&der)
            .expect("payload must be a valid DER-encoded PKCS#12 archive");

        assert!(
            pkcs_12.parse2("definitely-wrong-password").is_err(),
            "PKCS#12 archive must fail decryption with an incorrect password"
        );
    }

    #[test]
    fn build_identity_via_key_file_as_deprecated_fallback() {
        let mut temp_file = tempfile::NamedTempFile::new().unwrap();
        let expected_der = b"dummy-pkcs12-der-bytes";
        std::io::Write::write_all(&mut temp_file, expected_der).unwrap();

        let expected_password = "some-password";
        let tls = TlsConfig {
            ca_file: None,
            key_file: Some(temp_file.path().to_path_buf()),
            key_pass: Some(expected_password.into()),
            ..Default::default()
        };

        let identity = AmqpConfig::build_identity(&tls).expect("failed to build OwnerIdentity");

        let Some(OwnedIdentity::PKCS12 { der, password }) = identity else {
            panic!("expected OwnedIdentity::PKCS12")
        };

        assert_eq!(der, expected_der);
        assert_eq!(password, expected_password);
    }

    #[test]
    fn build_identity_disabled_when_no_certs() {
        let tls = TlsConfig::default();
        let identity = AmqpConfig::build_identity(&tls).expect("should succeed");
        assert!(
            identity.is_none(),
            "identity must be None if certs are omitted"
        );
    }
}
