#![allow(missing_docs)]
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rdkafka::{
    ClientConfig, ClientContext, Statistics, client::OAuthToken, consumer::ConsumerContext,
};
use snafu::Snafu;
use tracing::{Span, warn};
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

#[derive(Clone, Debug)]
pub(crate) struct KafkaOAuthBearerConfig {
    pub(crate) token_url: String,
    pub(crate) client_id: Option<String>,
    pub(crate) client_secret: Option<String>,
    pub(crate) scope: Option<String>,
    pub(crate) principal_name: String,
    pub(crate) extra_params: Vec<(String, String)>,
}

/// Parses space-separated `name=value` pairs from `sasl.oauthbearer.config`.
/// Returns (principal_name, extra_http_params).
/// - `principal=<v>` → principal_name
/// - `extension_<KEY>=<v>` → silently ignored (SASL broker extensions not applicable to HTTP)
/// - anything else → extra HTTP POST body params (can override grant_type, add code=, etc.)
fn parse_oauthbearer_config(config: Option<&str>) -> (String, Vec<(String, String)>) {
    let mut principal_name = String::new();
    let mut extra_params: Vec<(String, String)> = Vec::new();

    if let Some(s) = config {
        for pair in s.split_whitespace() {
            if let Some((key, value)) = pair.split_once('=') {
                if key == "principal" {
                    principal_name = value.to_owned();
                } else if key.starts_with("extension_") {
                    // SASL broker extensions — not sent to HTTP token endpoint; skip.
                } else {
                    extra_params.push((key.to_owned(), value.to_owned()));
                }
            }
        }
    }

    (principal_name, extra_params)
}

/// Reads OAUTHBEARER-relevant keys from `librdkafka_options` and returns a
/// `KafkaOAuthBearerConfig` if Vector should activate its token-fetch callback.
///
/// Returns `None` when `sasl.oauthbearer.token.endpoint.url` is absent,
/// or when `sasl.oauthbearer.method` is `"oidc"` (librdkafka handles it natively).
pub(crate) fn extract_oauthbearer_config(
    options: &HashMap<String, String>,
) -> Option<KafkaOAuthBearerConfig> {
    if options.get("sasl.oauthbearer.method").map(String::as_str) == Some("oidc") {
        return None;
    }

    let token_url = options.get("sasl.oauthbearer.token.endpoint.url")?.clone();

    let client_id = options.get("sasl.oauthbearer.client.id").cloned();
    let client_secret = options.get("sasl.oauthbearer.client.secret").cloned();
    let scope = options.get("sasl.oauthbearer.scope").cloned();

    let (mut principal_name, extra_params) =
        parse_oauthbearer_config(options.get("sasl.oauthbearer.config").map(String::as_str));

    if principal_name.is_empty() {
        principal_name = client_id.as_deref().unwrap_or_default().to_owned();
    }

    Some(KafkaOAuthBearerConfig {
        token_url,
        client_id,
        client_secret,
        scope,
        principal_name,
        extra_params,
    })
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
    pub span: Span,
    pub(crate) oauthbearer: Option<KafkaOAuthBearerConfig>,
}

impl ClientContext for KafkaStatisticsContext {
    const ENABLE_REFRESH_OAUTH_TOKEN: bool = true;

    fn stats(&self, statistics: Statistics) {
        // This callback get executed on a separate thread within the rdkafka library, so we need
        // to propagate the span here to attach the component tags to the emitted events.
        let _entered = self.span.enter();
        emit!(KafkaStatisticsReceived {
            statistics: &statistics,
            expose_lag_metrics: self.expose_lag_metrics,
        });
    }

    fn generate_oauth_token(
        &self,
        _oauthbearer_config: Option<&str>,
    ) -> Result<OAuthToken, Box<dyn std::error::Error>> {
        let config = self.oauthbearer.as_ref().ok_or(
            "sasl.oauthbearer.token.endpoint.url not configured; \
             set it in librdkafka_options to use method=default token fetch",
        )?;

        // Build form params: start with default grant_type, then merge extra_params which can override.
        let mut params: Vec<(String, String)> =
            vec![("grant_type".to_owned(), "client_credentials".to_owned())];

        if let Some(id) = &config.client_id {
            params.push(("client_id".to_owned(), id.clone()));
        }
        if let Some(secret) = &config.client_secret {
            params.push(("client_secret".to_owned(), secret.clone()));
        }
        if let Some(scope) = &config.scope {
            params.push(("scope".to_owned(), scope.clone()));
        }

        // extra_params can override defaults (e.g. grant_type=authorization_code).
        for (key, value) in &config.extra_params {
            params.retain(|(k, _)| k != key);
            params.push((key.clone(), value.clone()));
        }

        let url = config.token_url.clone();

        let resp: serde_json::Value = match tokio::runtime::Handle::try_current() {
            Ok(handle) => tokio::task::block_in_place(|| {
                handle.block_on(async move {
                    reqwest::Client::new()
                        .post(&url)
                        .form(&params)
                        .send()
                        .await?
                        .json::<serde_json::Value>()
                        .await
                })
            })?,
            Err(_) => tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?
                .block_on(async move {
                    reqwest::Client::new()
                        .post(&url)
                        .form(&params)
                        .send()
                        .await?
                        .json::<serde_json::Value>()
                        .await
                })?,
        };

        let token = resp["access_token"]
            .as_str()
            .ok_or("missing access_token in token endpoint response")?
            .to_owned();

        let expires_in = match resp["expires_in"].as_u64() {
            Some(v) => v,
            None => {
                warn!(message = "Expires_in missing from OAUTHBEARER token response, defaulting to 3600s.");
                3600
            }
        };

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis() as i64;

        Ok(OAuthToken {
            token,
            principal_name: config.principal_name.clone(),
            lifetime_ms: now_ms + (expires_in as i64 * 1000),
        })
    }
}

impl ConsumerContext for KafkaStatisticsContext {}

#[cfg(test)]
mod tests {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_string_contains, method, path},
    };

    use super::*;

    fn opts(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn extract_returns_none_when_url_absent() {
        let o = opts(&[("sasl.oauthbearer.method", "default")]);
        assert!(extract_oauthbearer_config(&o).is_none());
    }

    #[test]
    fn extract_returns_none_when_method_oidc() {
        let o = opts(&[
            ("sasl.oauthbearer.method", "oidc"),
            (
                "sasl.oauthbearer.token.endpoint.url",
                "https://example.com/token",
            ),
        ]);
        assert!(extract_oauthbearer_config(&o).is_none());
    }

    #[test]
    fn extract_minimal_config() {
        let o = opts(&[(
            "sasl.oauthbearer.token.endpoint.url",
            "https://example.com/token",
        )]);
        let c = extract_oauthbearer_config(&o).unwrap();
        assert_eq!(c.token_url, "https://example.com/token");
        assert!(c.client_id.is_none());
        assert!(c.client_secret.is_none());
        assert!(c.scope.is_none());
        assert_eq!(c.principal_name, "");
        assert!(c.extra_params.is_empty());
    }

    #[test]
    fn extract_full_config_with_credentials() {
        let o = opts(&[
            (
                "sasl.oauthbearer.token.endpoint.url",
                "https://example.com/token",
            ),
            ("sasl.oauthbearer.client.id", "my-client"),
            ("sasl.oauthbearer.client.secret", "my-secret"),
            ("sasl.oauthbearer.scope", "kafka:write"),
        ]);
        let c = extract_oauthbearer_config(&o).unwrap();
        assert_eq!(c.client_id.as_deref(), Some("my-client"));
        assert_eq!(c.client_secret.as_deref(), Some("my-secret"));
        assert_eq!(c.scope.as_deref(), Some("kafka:write"));
        assert_eq!(c.principal_name, "my-client"); // falls back to client_id
    }

    #[test]
    fn extract_absent_method_treated_as_default() {
        let o = opts(&[(
            "sasl.oauthbearer.token.endpoint.url",
            "https://example.com/token",
        )]);
        assert!(extract_oauthbearer_config(&o).is_some());
    }

    #[test]
    fn extract_explicit_default_method() {
        let o = opts(&[
            ("sasl.oauthbearer.method", "default"),
            (
                "sasl.oauthbearer.token.endpoint.url",
                "https://example.com/token",
            ),
        ]);
        assert!(extract_oauthbearer_config(&o).is_some());
    }

    #[test]
    fn parse_empty_config() {
        let (principal, params) = parse_oauthbearer_config(None);
        assert_eq!(principal, "");
        assert!(params.is_empty());
    }

    #[test]
    fn parse_principal_only() {
        let (principal, params) = parse_oauthbearer_config(Some("principal=my-service"));
        assert_eq!(principal, "my-service");
        assert!(params.is_empty());
    }

    #[test]
    fn parse_extra_http_params() {
        let (principal, params) =
            parse_oauthbearer_config(Some("grant_type=authorization_code code=my-pac"));
        assert_eq!(principal, "");
        assert_eq!(
            params,
            vec![
                ("grant_type".to_string(), "authorization_code".to_string()),
                ("code".to_string(), "my-pac".to_string()),
            ]
        );
    }

    #[test]
    fn parse_extension_pairs_ignored() {
        let (principal, params) =
            parse_oauthbearer_config(Some("principal=svc extension_traceId=abc extra=x"));
        assert_eq!(principal, "svc");
        assert_eq!(params, vec![("extra".to_string(), "x".to_string())]);
    }

    #[test]
    fn extract_extra_params_from_config_string() {
        let o = opts(&[
            (
                "sasl.oauthbearer.token.endpoint.url",
                "https://example.com/token",
            ),
            ("sasl.oauthbearer.client.id", "c"),
            (
                "sasl.oauthbearer.config",
                "grant_type=authorization_code code=PAC123",
            ),
        ]);
        let c = extract_oauthbearer_config(&o).unwrap();
        assert_eq!(
            c.extra_params,
            vec![
                ("grant_type".to_string(), "authorization_code".to_string()),
                ("code".to_string(), "PAC123".to_string()),
            ]
        );
        assert_eq!(c.principal_name, "c");
    }

    #[test]
    fn extract_principal_from_config_string_overrides_client_id() {
        let o = opts(&[
            (
                "sasl.oauthbearer.token.endpoint.url",
                "https://example.com/token",
            ),
            ("sasl.oauthbearer.client.id", "client-id"),
            ("sasl.oauthbearer.config", "principal=my-service"),
        ]);
        let c = extract_oauthbearer_config(&o).unwrap();
        assert_eq!(c.principal_name, "my-service");
    }

    #[test]
    fn parse_malformed_pair_without_equals_ignored() {
        let (principal, params) = parse_oauthbearer_config(Some("noequalssign valid=ok"));
        assert_eq!(principal, "");
        assert_eq!(params, vec![("valid".to_string(), "ok".to_string())]);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn generate_token_client_credentials() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .and(body_string_contains("grant_type=client_credentials"))
            .and(body_string_contains("client_id=my-client"))
            .and(body_string_contains("client_secret=my-secret"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "tok123",
                "token_type": "bearer",
                "expires_in": 3600u64,
            })))
            .mount(&mock)
            .await;

        let ctx = KafkaStatisticsContext {
            expose_lag_metrics: false,
            span: tracing::Span::current(),
            oauthbearer: Some(KafkaOAuthBearerConfig {
                token_url: format!("{}/token", mock.uri()),
                client_id: Some("my-client".into()),
                client_secret: Some("my-secret".into()),
                scope: None,
                principal_name: "my-client".into(),
                extra_params: vec![],
            }),
        };

        let token = ctx.generate_oauth_token(None).unwrap();
        assert_eq!(token.token, "tok123");
        assert_eq!(token.principal_name, "my-client");
        assert!(token.lifetime_ms > 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn generate_token_with_extra_params_override_grant_type() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .and(body_string_contains("grant_type=authorization_code"))
            .and(body_string_contains("code=PAC123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "ims-tok",
                "expires_in": 86399u64,
            })))
            .mount(&mock)
            .await;

        let ctx = KafkaStatisticsContext {
            expose_lag_metrics: false,
            span: tracing::Span::current(),
            oauthbearer: Some(KafkaOAuthBearerConfig {
                token_url: format!("{}/token", mock.uri()),
                client_id: None,
                client_secret: None,
                scope: None,
                principal_name: "".into(),
                extra_params: vec![
                    ("grant_type".into(), "authorization_code".into()),
                    ("code".into(), "PAC123".into()),
                ],
            }),
        };

        let token = ctx.generate_oauth_token(None).unwrap();
        assert_eq!(token.token, "ims-tok");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn generate_token_missing_expires_in_defaults_to_3600() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "no-expiry-tok",
            })))
            .mount(&mock)
            .await;

        let ctx = KafkaStatisticsContext {
            expose_lag_metrics: false,
            span: tracing::Span::current(),
            oauthbearer: Some(KafkaOAuthBearerConfig {
                token_url: format!("{}/token", mock.uri()),
                client_id: None,
                client_secret: None,
                scope: None,
                principal_name: "".into(),
                extra_params: vec![],
            }),
        };

        let token = ctx.generate_oauth_token(None).unwrap();
        assert_eq!(token.token, "no-expiry-tok");
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        assert!(token.lifetime_ms >= now_ms + 3_595_000);
        assert!(token.lifetime_ms <= now_ms + 3_605_000);
    }

    #[test]
    fn generate_token_returns_err_when_oauthbearer_not_configured() {
        let ctx = KafkaStatisticsContext {
            expose_lag_metrics: false,
            span: tracing::Span::current(),
            oauthbearer: None,
        };
        assert!(ctx.generate_oauth_token(None).is_err());
    }
}
