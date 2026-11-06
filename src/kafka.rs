#![allow(missing_docs)]
use std::path::{Path, PathBuf};

use rdkafka::{ClientConfig, ClientContext, Statistics, consumer::ConsumerContext};
use snafu::Snafu;
use tracing::Span;
use vector_lib::{configurable::configurable_component, sensitive_string::SensitiveString};

#[cfg(feature = "azure-core")]
use crate::sinks::azure_common::config::AzureAuthentication;
use crate::{
    internal_events::KafkaStatisticsReceived,
    tls::{PEM_START_MARKER, TlsEnableableConfig},
};

#[cfg(all(feature = "sources-kafka", feature = "azure-core"))]
mod azure;
#[cfg(feature = "sources-kafka")]
pub(crate) mod oauth;

const OAUTHBEARER: &str = "OAUTHBEARER";

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
    /// If this is not set, SASL authentication is enabled only when `azure` is set.
    ///
    /// The `sasl.*` options configure the `PLAIN`- and `SCRAM`-based mechanisms with `username`,
    /// `password`, and `mechanism`, and the `OAUTHBEARER` mechanism with Microsoft Entra ID tokens
    /// with `azure` (`kafka` source only). For
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

    /// Authenticate with a Microsoft Entra ID token through the `OAUTHBEARER` SASL mechanism.
    ///
    /// Use this to read from Azure Event Hubs through its Kafka endpoint with a managed identity or
    /// workload identity instead of a shared access key. Vector sets `mechanism` to `OAUTHBEARER`,
    /// requests tokens with `auth`, and refreshes them before they expire.
    ///
    /// This option enables SASL authentication when `enabled` is not set, and is ignored when
    /// `enabled` is `false`. It requires `tls.enabled: true`, because the token is a bearer
    /// credential.
    ///
    /// Do not set `username`, `password`, or a `mechanism` other than `OAUTHBEARER` with this
    /// option. In `librdkafka_options`, do not set `bootstrap.servers`, `metadata.broker.list`,
    /// `security.protocol`, `sasl.mechanism`, `sasl.username`, `sasl.password`,
    /// `sasl.oauthbearer.*`, or `enable.sasl.oauthbearer.unsecure.jwt`.
    ///
    /// Token requests do not use the global `proxy` option. See the "Azure Event Hubs" section under
    /// "How it works" for the proxy settings that they use.
    ///
    /// Only the `kafka` source supports this option. It requires Vector to be built with the
    /// `azure-core` feature.
    pub(crate) azure: Option<KafkaSaslAzureConfig>,
}

/// Microsoft Entra ID authentication for SASL/OAUTHBEARER.
#[configurable_component]
#[derive(Clone, Debug)]
// Without `azure-core`, there is no `auth` field, so `auth` must parse as an unknown field for the
// source to fail with the error that names the feature.
#[cfg_attr(feature = "azure-core", serde(deny_unknown_fields))]
pub struct KafkaSaslAzureConfig {
    /// The scope to request tokens for.
    ///
    /// For Azure Event Hubs, use `https://<namespace>.servicebus.windows.net/.default`, where
    /// `<namespace>` is the Event Hubs namespace in `bootstrap_servers`. Vector sends the token to
    /// the brokers in `bootstrap_servers` and to the brokers that they advertise, so name only that
    /// namespace, never a broader resource.
    #[configurable(metadata(
        docs::examples = "https://mynamespace.servicebus.windows.net/.default"
    ))]
    pub(crate) token_scope: String,

    /// The Azure credential to request tokens with.
    #[cfg(feature = "azure-core")]
    pub(crate) auth: AzureAuthentication,
}

impl KafkaSaslConfig {
    /// Returns whether SASL is enabled. If `enabled` is not set, `azure` enables SASL.
    fn is_enabled(&self) -> bool {
        self.enabled.unwrap_or(self.azure.is_some())
    }
}

impl KafkaAuthConfig {
    /// Returns `sasl.azure` if SASL is enabled. `sasl.azure` enables SASL when `sasl.enabled` is
    /// not set, and is ignored when `sasl.enabled` is `false`.
    pub(crate) fn sasl_azure(&self) -> Option<&KafkaSaslAzureConfig> {
        self.sasl
            .as_ref()
            .filter(|sasl| sasl.is_enabled())?
            .azure
            .as_ref()
    }

    pub(crate) fn apply(&self, client: &mut ClientConfig) -> crate::Result<()> {
        let sasl_enabled = self.sasl.as_ref().is_some_and(KafkaSaslConfig::is_enabled);
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
            if sasl.azure.is_some() {
                // The client context gets each token from the `sasl.azure` token provider.
                // `KafkaAuthConfig::oauth_token_provider` checks the other settings first.
                client.set("sasl.mechanism", OAUTHBEARER);
            } else {
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
    use rdkafka::ClientConfig;

    use super::*;

    fn auth(sasl_options: &str) -> KafkaAuthConfig {
        serde_yaml::from_str(&format!(
            r"
            sasl:
              enabled: true
              {sasl_options}
            tls:
              enabled: true
            "
        ))
        .expect("the authentication settings should parse")
    }

    fn azure_auth(sasl_options: &str) -> KafkaAuthConfig {
        auth(&format!(
            r#"{sasl_options}
              azure:
                token_scope: "https://mynamespace.servicebus.windows.net/.default"
                auth:
                  azure_credential_kind: workload_identity"#
        ))
    }

    fn apply(auth: &KafkaAuthConfig) -> crate::Result<ClientConfig> {
        let mut client = ClientConfig::new();
        auth.apply(&mut client)?;
        Ok(client)
    }

    #[test]
    fn sasl_azure_uses_oauthbearer() {
        // `sasl.azure` also enables SASL when `sasl.enabled` is not set.
        for enabled in [Some(true), None] {
            let mut auth = azure_auth("");
            auth.sasl.as_mut().unwrap().enabled = enabled;
            assert_eq!(
                auth.sasl_azure().map(|azure| azure.token_scope.as_str()),
                Some("https://mynamespace.servicebus.windows.net/.default"),
                "enabled: {enabled:?}"
            );

            let client = apply(&auth).unwrap();
            assert_eq!(client.get("security.protocol"), Some("sasl_ssl"));
            assert_eq!(client.get("sasl.mechanism"), Some("OAUTHBEARER"));
            assert_eq!(client.get("sasl.username"), None);
            assert_eq!(client.get("sasl.password"), None);
        }
    }

    #[test]
    fn disabled_sasl_ignores_azure() {
        let mut auth = azure_auth("mechanism: PLAIN");
        auth.sasl.as_mut().unwrap().enabled = Some(false);
        assert!(auth.sasl_azure().is_none());

        let client = apply(&auth).unwrap();
        assert_eq!(client.get("security.protocol"), Some("ssl"));
        assert_eq!(client.get("sasl.mechanism"), None);
    }

    #[test]
    fn sasl_without_azure_is_unchanged() {
        let auth = auth(
            "mechanism: SCRAM-SHA-512\n              username: user\n              password: secret",
        );
        assert!(auth.sasl_azure().is_none());

        let client = apply(&auth).unwrap();
        assert_eq!(client.get("security.protocol"), Some("sasl_ssl"));
        assert_eq!(client.get("sasl.mechanism"), Some("SCRAM-SHA-512"));
        assert_eq!(client.get("sasl.username"), Some("user"));
        assert_eq!(client.get("sasl.password"), Some("secret"));

        // Without `sasl.azure`, SASL stays disabled when `sasl.enabled` is not set.
        let mut auth = auth;
        auth.sasl.as_mut().unwrap().enabled = None;
        let client = apply(&auth).unwrap();
        assert_eq!(client.get("security.protocol"), Some("ssl"));
        assert_eq!(client.get("sasl.mechanism"), None);
    }

    #[cfg(feature = "azure-core")]
    #[test]
    fn sasl_azure_rejects_unknown_fields() {
        let error = serde_yaml::from_str::<KafkaAuthConfig>(
            "sasl:\n  enabled: true\n  azure:\n    token_scope: https://example.com/.default\n    \
             auth:\n      azure_credential_kind: managed_identity\n    resource: https://example.com\n",
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("resource"), "{error}");
    }
}
