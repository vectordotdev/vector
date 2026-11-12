#![allow(missing_docs)]
use crate::{internal_events::KafkaStatisticsReceived, tls::TlsEnableableConfig};
use rdkafka::{ClientConfig, ClientContext, Statistics, consumer::ConsumerContext};
use snafu::Snafu;
use std::path::{Path, PathBuf};
use tracing::Span;
use vector_lib::tls::TlsSettings;
use vector_lib::{
    configurable::configurable_component, internal_event::emit, sensitive_string::SensitiveString,
};

#[derive(Debug, Snafu)]
enum KafkaError {
    #[snafu(display("invalid path: {:?}", path))]
    InvalidPath { path: PathBuf },
}

/// Supported compression types for Kafka.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum KafkaCompression {
    /// No compression.
    #[default]
    None,

    /// Gzip.
    Gzip,

    /// Snappy.
    Snappy,

    /// LZ4.
    Lz4,

    /// Zstandard.
    Zstd,
}

/// Kafka authentication configuration.
#[configurable_component]
#[derive(Clone, Debug, Default)]
pub struct KafkaAuthConfig {
    pub(crate) sasl: Option<KafkaSaslConfig>,

    pub(crate) tls: Option<TlsEnableableConfig>,
}

/// Configuration for SASL authentication when interacting with Kafka.
#[configurable_component]
#[derive(Clone, Debug, Default)]
pub struct KafkaSaslConfig {
    /// Enables SASL authentication.
    ///
    /// Only `PLAIN`- and `SCRAM`-based mechanisms are supported when configuring SASL authentication using `sasl.*`. For
    /// other mechanisms, `librdkafka_options.*` must be used directly to configure other `librdkafka`-specific values.
    /// If using `sasl.kerberos.*` as an example, where `*` is `service.name`, `principal`, `kinit.md`, etc., then
    /// `librdkafka_options.*` as a result becomes `librdkafka_options.sasl.kerberos.service.name`,
    /// `librdkafka_options.sasl.kerberos.principal`, etc.
    ///
    /// See the [librdkafka documentation](https://github.com/edenhill/librdkafka/blob/master/CONFIGURATION.md) for details.
    ///
    /// SASL authentication is not supported on Windows.
    pub(crate) enabled: Option<bool>,

    /// The SASL username.
    #[configurable(metadata(docs::examples = "username"))]
    pub(crate) username: Option<String>,

    /// The SASL password.
    #[configurable(metadata(docs::examples = "password"))]
    pub(crate) password: Option<SensitiveString>,

    /// The SASL mechanism to use.
    #[configurable(metadata(docs::examples = "SCRAM-SHA-256"))]
    #[configurable(metadata(docs::examples = "SCRAM-SHA-512"))]
    pub(crate) mechanism: Option<String>,
}

impl KafkaAuthConfig {
    pub(crate) fn apply(&self, client: &mut ClientConfig) -> crate::Result<()> {
        let sasl_enabled = self.sasl.as_ref().and_then(|s| s.enabled).unwrap_or(false);
        let tls_enabled = self.tls.as_ref().and_then(|s| s.enabled).unwrap_or(false);

        let protocol = match (sasl_enabled, tls_enabled) {
            (false, false) => "plaintext",
            (false, true) => "ssl",
            (true, false) => "sasl_plaintext",
            (true, true) => "sasl_ssl",
        };
        client.set("security.protocol", protocol);

        if sasl_enabled {
            let sasl = self.sasl.as_ref().unwrap();
            if let Some(username) = &sasl.username {
                client.set("sasl.username", username.as_str());
            }
            if let Some(password) = &sasl.password {
                client.set("sasl.password", password.inner());
            }
            if let Some(mechanism) = &sasl.mechanism {
                client.set("sasl.mechanism", mechanism);
            }
        }

        if tls_enabled {
            let tls = self.tls.as_ref().unwrap();

            if let Some(verify_certificate) = &tls.options.verify_certificate {
                client.set(
                    "enable.ssl.certificate.verification",
                    verify_certificate.to_string(),
                );
            }

            if let Some(verify_hostname) = &tls.options.verify_hostname {
                client.set(
                    "ssl.endpoint.identification.algorithm",
                    if *verify_hostname { "https" } else { "none" },
                );
            }

            let mut tls_options = tls.options.clone();
            if let Some(path) = &tls.options.ca_file
                && path.is_dir()
            {
                warn!(
                    "DEPRECATED: Passing a directory to `tls.ca_file` for a Kafka connection is deprecated, combine the CA certificates into a single PEM/DER file or inline value and pass it to `tls.ca_file` instead."
                );
                client.set("ssl.ca.location", pathbuf_to_string(path)?);

                // Admittedly, cloning `tls.options` to clear `ca_file` isn't pretty, but it preserves
                // backward compatibility for directory paths in `ca_file` (which `librdkafka` supports
                // via `ssl.ca.location`) while letting us reuse `TlsSettings::from_options` out of the box
                // instead of custom certificate-loading logic.
                // Remove this cloning once the deprecated directory fallback above is removed.
                tls_options.ca_file = None;
            }

            let tls_settings = TlsSettings::from_options(Some(&tls_options))?;

            let ca_pem: String = tls_settings
                .authorities_pem()
                .map(|pem| {
                    String::from_utf8(pem)
                        .expect("Failed to parse CA certificate for a Kafka connection")
                })
                .collect();

            if !ca_pem.is_empty() {
                client.set("ssl.ca.pem", ca_pem);
            }

            if let Some((cert_pem, key_pem)) = tls_settings.identity_pem() {
                client.set(
                    "ssl.certificate.pem",
                    String::from_utf8(cert_pem)
                        .expect("Failed to parse client certificate for a Kafka connection"),
                );
                client.set(
                    "ssl.key.pem",
                    String::from_utf8(key_pem)
                        .expect("Failed to parse private key for a Kafka connection"),
                );
            }
        }

        Ok(())
    }
}

fn pathbuf_to_string(path: &Path) -> crate::Result<&str> {
    path.to_str()
        .ok_or_else(|| KafkaError::InvalidPath { path: path.into() }.into())
}

pub(crate) struct KafkaStatisticsContext {
    pub(crate) expose_lag_metrics: bool,
    pub(crate) span: Span,
}

impl ClientContext for KafkaStatisticsContext {
    fn stats(&self, statistics: Statistics) {
        // This callback get executed on a separate thread within the rdkafka library, so we need
        // to propagate the span here to attach the component tags to the emitted events.
        let _entered = self.span.enter();
        emit(KafkaStatisticsReceived {
            statistics: &statistics,
            expose_lag_metrics: self.expose_lag_metrics,
        });
    }
}

impl ConsumerContext for KafkaStatisticsContext {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tls::{TEST_PEM_CA_PATH, TEST_PEM_CRT_PATH, TEST_PEM_KEY_PATH};
    use vector_lib::tls::TlsConfig;

    #[test]
    fn tls_pem_config() {
        let auth = build_kafka_auth_config(TlsConfig {
            ca_file: Some(TEST_PEM_CA_PATH.into()),
            crt_file: Some(TEST_PEM_CRT_PATH.into()),
            key_file: Some(TEST_PEM_KEY_PATH.into()),
            key_pass: Some("test_password".into()),
            ..Default::default()
        });

        let mut client_config = ClientConfig::new();
        auth.apply(&mut client_config).unwrap();

        assert!(
            client_config
                .get("ssl.ca.pem")
                .unwrap()
                .contains("-----BEGIN CERTIFICATE-----")
        );
        assert!(
            client_config
                .get("ssl.certificate.pem")
                .unwrap()
                .contains("-----BEGIN CERTIFICATE-----")
        );
        assert!(
            client_config
                .get("ssl.key.pem")
                .unwrap()
                .contains("PRIVATE KEY-----")
        );

        assert_eq!(client_config.get("ssl.ca.location"), None);
        assert_eq!(client_config.get("ssl.certificate.location"), None);
        assert_eq!(client_config.get("ssl.key.location"), None);
        assert_eq!(client_config.get("ssl.key.password"), None);
    }

    #[test]
    fn tls_ca_directory_fallback() {
        let ca_dir = tempfile::tempdir().unwrap();

        let auth = build_kafka_auth_config(TlsConfig {
            ca_file: Some(ca_dir.path().to_path_buf()),
            ..Default::default()
        });

        let mut client_config = ClientConfig::new();
        auth.apply(&mut client_config).unwrap();

        assert_eq!(
            client_config.get("ssl.ca.location"),
            Some(ca_dir.path().to_str().unwrap())
        );
        assert_eq!(client_config.get("ssl.ca.pem"), None);
    }

    fn build_kafka_auth_config(options: TlsConfig) -> KafkaAuthConfig {
        KafkaAuthConfig {
            sasl: None,
            tls: Some(TlsEnableableConfig {
                enabled: Some(true),
                options,
            }),
        }
    }
}
