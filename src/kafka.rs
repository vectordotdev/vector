#![allow(missing_docs)]
use std::path::{Path, PathBuf};

use rdkafka::{consumer::ConsumerContext, ClientConfig, ClientContext, Statistics};
use snafu::Snafu;
use tracing::Span;
use vector_lib::configurable::configurable_component;
use vector_lib::sensitive_string::SensitiveString;

use crate::{
    internal_events::KafkaStatisticsReceived, tls::TlsEnableableConfig, tls::PEM_START_MARKER,
};

#[derive(Debug, Snafu)]
enum KafkaError {
    #[snafu(display("invalid path: {:?}", path))]
    InvalidPath { path: PathBuf },
}

/// Supported compression types for Kafka.
#[configurable_component]
#[derive(Clone, Copy, Debug, Derivative)]
#[derivative(Default)]
#[serde(rename_all = "lowercase")]
pub enum KafkaCompression {
    /// No compression.
    #[derivative(Default)]
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
    #[configurable(derived)]
    pub(crate) sasl: Option<KafkaSaslConfig>,

    #[configurable(derived)]
    #[configurable(metadata(docs::advanced))]
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
                    &verify_certificate.to_string(),
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
    pub span: Span,
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
