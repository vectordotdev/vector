#![allow(missing_docs)]
use std::path::{Path, PathBuf};

use rdkafka::{ClientConfig, ClientContext, Statistics, consumer::ConsumerContext};
use snafu::Snafu;
use tracing::Span;
use vector_lib::{configurable::configurable_component, sensitive_string::SensitiveString};

use crate::{
    internal_events::KafkaStatisticsReceived,
    tls::{PEM_START_MARKER, TlsEnableableConfig},
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

            // librdkafka exposes no protocol version knobs, so only the host's OpenSSL policy can guarantee a bound.
            if tls.options.has_protocol_version_bounds() {
                vector_lib::tls::enforce_protocol_version_bounds(
                    tls.options.min_version,
                    tls.options.max_version,
                    "librdkafka",
                    vector_lib::tls::host_protocol_version_range()?,
                )?;
            }

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

            if let Some(path) = &tls.options.ca_file {
                let text = pathbuf_to_string(path)?;
                if text.contains(PEM_START_MARKER) {
                    client.set("ssl.ca.pem", text);
                } else {
                    client.set("ssl.ca.location", text);
                }
            }

            if let Some(path) = &tls.options.crt_file {
                let text = pathbuf_to_string(path)?;
                if text.contains(PEM_START_MARKER) {
                    client.set("ssl.certificate.pem", text);
                } else {
                    client.set("ssl.certificate.location", text);
                }
            }

            if let Some(path) = &tls.options.key_file {
                let text = pathbuf_to_string(path)?;
                if text.contains(PEM_START_MARKER) {
                    client.set("ssl.key.pem", text);
                } else {
                    client.set("ssl.key.location", text);
                }
            }

            if let Some(pass) = &tls.options.key_pass {
                client.set("ssl.key.password", pass);
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
        emit!(KafkaStatisticsReceived {
            statistics: &statistics,
            expose_lag_metrics: self.expose_lag_metrics,
        });
    }
}

impl ConsumerContext for KafkaStatisticsContext {}

#[cfg(test)]
mod tests {
    use vector_lib::tls::{TlsConfig, TlsVersion};

    use super::*;

    fn auth_with_min_version(min_version: TlsVersion) -> KafkaAuthConfig {
        KafkaAuthConfig {
            tls: Some(TlsEnableableConfig {
                enabled: Some(true),
                options: TlsConfig {
                    min_version: Some(min_version),
                    ..Default::default()
                },
            }),
            ..Default::default()
        }
    }

    #[test]
    fn apply_rejects_a_minimum_the_host_policy_cannot_guarantee() {
        let mut client_config = ClientConfig::new();
        auth_with_min_version(TlsVersion::Tls13)
            .apply(&mut client_config)
            .expect_err("librdkafka cannot enforce a TLS v1.3 minimum beyond host policy");
    }

    #[test]
    fn apply_accepts_a_minimum_the_host_policy_already_guarantees() {
        let mut client_config = ClientConfig::new();
        auth_with_min_version(TlsVersion::Tls12)
            .apply(&mut client_config)
            .expect("the default security level already refuses TLS v1.0 and v1.1");
    }
}
