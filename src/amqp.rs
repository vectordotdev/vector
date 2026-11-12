//! Functionality supporting both the `[crate::sources::amqp]` source and `[crate::sinks::amqp]` sink.

use crate::tls::TlsConfig;
use lapin::tcp::{OwnedIdentity, OwnedTLSConfig};
use std::error::Error;
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

                let identity = if let Some(identity) = &tls.key_file {
                    let der = tokio::fs::read(identity.to_owned()).await?;
                    Some(OwnedIdentity::PKCS12 {
                        der,
                        password: tls.key_pass.clone().unwrap_or_else(String::default),
                    })
                } else {
                    None
                };
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tls::{TEST_PEM_CA_PATH, TEST_PEM_INTERMEDIATE_CA_PATH};
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
}
