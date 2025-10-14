//! Configuration for the `sentry` sink.

use futures::FutureExt;
use sentry::{IntoDsn, types::Dsn};
use vector_lib::configurable::configurable_component;

use crate::{
    codecs::EncodingConfig,
    http::HttpClient,
    internal_events::SentryInvalidDsnError,
    sinks::{
        prelude::*,
        util::{
            BatchConfig, RealtimeSizeBasedDefaultBatchSettings, ServiceBuilderExt,
            http::{HttpService, RequestConfig, http_response_retry_logic},
        },
    },
};

use super::encoder::SentryEncoder;
use super::request_builder::SentryRequestBuilder;
use super::service::SentryServiceRequestBuilder;
use super::sink::SentrySink;

/// Configuration for the Sentry sink.
#[configurable_component(sink("sentry"))]
#[derive(Clone, Debug)]
pub struct SentryConfig {
    /// Sentry Data Source Name (DSN).
    ///
    /// The DSN tells the SDK where to send events so they are associated with the correct project.
    /// Format: {PROTOCOL}://{PUBLIC_KEY}@{HOST}/{PROJECT_ID}
    #[configurable(metadata(
        docs::examples = "https://abcdef1234567890@o123456.ingest.sentry.io/9876543"
    ))]
    pub dsn: String,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: RequestConfig,

    #[configurable(derived)]
    pub tls: Option<TlsConfig>,

    #[configurable(derived)]
    pub encoding: EncodingConfig,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl GenerateConfig for SentryConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"dsn = "https://your-public-key@your-sentry-host/your-project-id"
encoding.codec = "json""#,
        )
        .unwrap()
    }
}

#[async_trait]
#[typetag::serde(name = "sentry")]
impl SinkConfig for SentryConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let batch_settings = self.batch.validate()?.into_batcher_settings()?;

        let tls = TlsSettings::from_options(self.tls.as_ref())?;
        let client = HttpClient::new(tls, cx.proxy())?;

        let dsn: Dsn = self
            .dsn
            .clone()
            .into_dsn()
            .map_err(|e| {
                emit!(SentryInvalidDsnError {
                    error: format!("{:?}", e),
                    dsn: self.dsn.clone(),
                });
                format!("Invalid DSN: {:?}", e)
            })?
            .ok_or_else(|| {
                emit!(SentryInvalidDsnError {
                    error: "Failed to parse DSN".to_string(),
                    dsn: self.dsn.clone(),
                });
                "Failed to parse DSN"
            })?;

        let transformer = self.encoding.transformer();
        let encoder = SentryEncoder::new(transformer);
        let request_builder = SentryRequestBuilder::new(encoder);
        let sentry_service_request_builder = SentryServiceRequestBuilder::new(dsn);

        let service = HttpService::new(client, sentry_service_request_builder);

        let request_limits = self.request.tower.into_settings();
        let service = ServiceBuilder::new()
            .settings(request_limits, http_response_retry_logic())
            .service(service);

        let sink = SentrySink::new(service, batch_settings, request_builder)?;

        // Healthcheck validates DSN format
        let healthcheck = healthcheck_dsn(self.dsn.clone()).boxed();

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

async fn healthcheck_dsn(dsn_str: String) -> crate::Result<()> {
    let _dsn: Dsn = dsn_str
        .clone()
        .into_dsn()
        .map_err(|e| {
            emit!(SentryInvalidDsnError {
                error: format!("{:?}", e),
                dsn: dsn_str.clone(),
            });
            format!("Invalid DSN: {:?}", e)
        })?
        .ok_or_else(|| {
            emit!(SentryInvalidDsnError {
                error: "Failed to parse DSN".to_string(),
                dsn: dsn_str.clone(),
            });
            "Failed to parse DSN".to_string()
        })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<SentryConfig>();
    }

    #[test]
    fn parse_config() {
        let cfg = toml::from_str::<SentryConfig>(
            r#"
            dsn = "https://abcdef1234567890@o123456.ingest.sentry.io/9876543"
            encoding.codec = "json"
        "#,
        )
        .unwrap();

        assert_eq!(
            cfg.dsn,
            "https://abcdef1234567890@o123456.ingest.sentry.io/9876543"
        );
    }

    #[test]
    fn parse_config_with_batch_settings() {
        let cfg = toml::from_str::<SentryConfig>(
            r#"
            dsn = "https://key@host/project"
            encoding.codec = "json"
            batch.max_events = 100
            batch.timeout_secs = 5
        "#,
        )
        .unwrap();

        assert_eq!(cfg.dsn, "https://key@host/project");
        assert_eq!(cfg.batch.max_events, Some(100));
        assert_eq!(cfg.batch.timeout_secs, Some(5.0));
    }

    #[test]
    fn parse_config_invalid_dsn_format() {
        let result = toml::from_str::<SentryConfig>(
            r#"
            dsn = "invalid-dsn-format"
            encoding.codec = "json"
        "#,
        );

        // This should parse successfully at the config level,
        // DSN validation happens at build time
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn healthcheck_dsn_valid() {
        let result = healthcheck_dsn(
            "https://abcdef1234567890@o123456.ingest.sentry.io/9876543".to_string(),
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn healthcheck_dsn_invalid() {
        let result = healthcheck_dsn("invalid-dsn".to_string()).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid DSN"));
    }

    #[tokio::test]
    async fn healthcheck_dsn_empty() {
        let result = healthcheck_dsn("".to_string()).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Failed to parse DSN")
        );
    }
}
