#![expect(
    clippy::let_underscore_must_use,
    reason = "derivative's Debug derive with ignored fields expands to a must_use let binding"
)]

use std::sync::Arc;

use derivative::Derivative;
use http::{Uri, header::HeaderValue};
use tower::ServiceBuilder;
use vector_lib::sensitive_string::SensitiveString;

use super::{
    NewRelicApiResponse, NewRelicApiService, NewRelicEncoder, NewRelicSink, NewRelicSinkError,
    healthcheck, service::NewRelicApiRequest,
};
use crate::{
    config::ValidatedSink,
    http::HttpClient,
    sinks::{prelude::*, util::HttpEndpoint, util::service::TowerRequestSettings},
};

/// New Relic region.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum NewRelicRegion {
    /// US region.
    #[default]
    Us,

    /// EU region.
    Eu,
}

/// New Relic API endpoint.
#[configurable_component]
#[derive(Clone, Copy, Default, Debug, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum NewRelicApi {
    /// Events API.
    #[default]
    Events,

    /// Metrics API.
    Metrics,

    /// Logs API.
    Logs,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NewRelicDefaultBatchSettings;

impl SinkBatchSettings for NewRelicDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(100);
    const MAX_BYTES: Option<usize> = Some(1_000_000);
    const TIMEOUT_SECS: f64 = 1.0;
}

#[derive(Debug, Default, Clone)]
pub struct NewRelicApiRetry;

impl RetryLogic for NewRelicApiRetry {
    type Error = NewRelicSinkError;
    type Request = NewRelicApiRequest;
    type Response = NewRelicApiResponse;

    fn is_retriable_error(&self, _error: &Self::Error) -> bool {
        // Never retry.
        false
    }
}

/// Configuration for the `new_relic` sink.
#[configurable_component(sink("new_relic", "Deliver events to New Relic."))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct NewRelicConfig {
    /// A valid New Relic license key.
    #[configurable(metadata(docs::examples = "xxxx"))]
    #[configurable(metadata(docs::examples = "${NEW_RELIC_LICENSE_KEY}"))]
    pub license_key: SensitiveString,

    /// The New Relic account ID.
    #[configurable(metadata(docs::examples = "xxxx"))]
    #[configurable(metadata(docs::examples = "${NEW_RELIC_ACCOUNT_KEY}"))]
    pub account_id: SensitiveString,

    pub region: Option<NewRelicRegion>,

    pub api: NewRelicApi,

    #[serde(default = "Compression::gzip_default")]
    pub compression: Compression,

    #[serde(default, skip_serializing_if = "crate::serde::is_default")]
    pub encoding: Transformer,

    #[serde(default)]
    pub batch: BatchConfig<NewRelicDefaultBatchSettings>,

    #[serde(default)]
    pub request: TowerRequestConfig,

    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    acknowledgements: AcknowledgementsConfig,

    #[serde(skip)]
    pub override_uri: Option<HttpEndpoint>,
}

impl_generate_config_from_default!(NewRelicConfig);

impl NewRelicConfig {
    pub fn build_healthcheck(
        &self,
        client: HttpClient,
        credentials: Arc<NewRelicCredentials>,
    ) -> crate::Result<super::Healthcheck> {
        Ok(healthcheck::healthcheck(client, credentials).boxed())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "new_relic")]
impl SinkConfig for NewRelicConfig {
    fn input(&self) -> Input {
        Input::new(DataType::Log | DataType::Metric)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[derive(Clone, Derivative)]
#[derivative(Debug)]
pub struct ValidatedNewRelic {
    batcher_settings: BatcherSettings,
    request_limits: TowerRequestSettings,
    // The credentials contain the license key and account ID, so they are
    // intentionally omitted from diagnostics.
    #[derivative(Debug = "ignore")]
    credentials: Arc<NewRelicCredentials>,
}

#[async_trait::async_trait]
impl ValidatedSink for NewRelicConfig {
    type Validated = ValidatedNewRelic;

    fn validate(&self) -> crate::Result<ValidatedNewRelic> {
        let batcher_settings = self
            .batch
            .validate()?
            .limit_max_events(self.batch.max_events.unwrap_or(100))?
            .into_batcher_settings()?;
        let request_limits = self.request.into_settings();
        let credentials = Arc::from(NewRelicCredentials::try_from_config(self)?);
        credentials.try_get_uri()?;

        Ok(ValidatedNewRelic {
            batcher_settings,
            request_limits,
            credentials,
        })
    }

    async fn build(
        &self,
        validated: &ValidatedNewRelic,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let ValidatedNewRelic {
            batcher_settings,
            request_limits,
            credentials,
        } = validated;

        let tls_settings = TlsSettings::from_options(None)?;
        let client = HttpClient::new(tls_settings, &cx.proxy)?;

        let healthcheck = self.build_healthcheck(client.clone(), Arc::clone(credentials))?;

        let service = ServiceBuilder::new()
            .settings(request_limits.clone(), NewRelicApiRetry)
            .service(NewRelicApiService { client });

        let sink = NewRelicSink {
            service,
            encoder: NewRelicEncoder {
                transformer: self.encoding.clone(),
                credentials: Arc::clone(credentials),
            },
            credentials: Arc::clone(credentials),
            compression: self.compression,
            batcher_settings: *batcher_settings,
        };

        Ok((super::VectorSink::from_event_streamsink(sink), healthcheck))
    }
}

#[derive(Debug, Clone)]
pub struct NewRelicCredentials {
    pub license_key: HeaderValue,
    pub account_id: String,
    pub api: NewRelicApi,
    pub region: NewRelicRegion,
    pub override_uri: Option<HttpEndpoint>,
}

impl NewRelicCredentials {
    pub fn get_uri(&self) -> Uri {
        self.try_get_uri().expect("URI should be valid")
    }

    pub fn try_get_uri(&self) -> crate::Result<Uri> {
        if let Some(override_uri) = self.override_uri.as_ref() {
            return Ok(override_uri.as_uri().clone());
        }

        match self.api {
            NewRelicApi::Events => match self.region {
                NewRelicRegion::Us => Ok(format!(
                    "https://insights-collector.newrelic.com/v1/accounts/{}/events",
                    self.account_id
                )
                .parse::<Uri>()?),
                NewRelicRegion::Eu => Ok(format!(
                    "https://insights-collector.eu01.nr-data.net/v1/accounts/{}/events",
                    self.account_id
                )
                .parse::<Uri>()?),
            },
            NewRelicApi::Metrics => match self.region {
                NewRelicRegion::Us => Ok(Uri::from_static(
                    "https://metric-api.newrelic.com/metric/v1",
                )),
                NewRelicRegion::Eu => Ok(Uri::from_static(
                    "https://metric-api.eu.newrelic.com/metric/v1",
                )),
            },
            NewRelicApi::Logs => match self.region {
                NewRelicRegion::Us => Ok(Uri::from_static("https://log-api.newrelic.com/log/v1")),
                NewRelicRegion::Eu => {
                    Ok(Uri::from_static("https://log-api.eu.newrelic.com/log/v1"))
                }
            },
        }
    }
}

impl NewRelicCredentials {
    pub fn try_from_config(config: &NewRelicConfig) -> crate::Result<Self> {
        let license_key = HeaderValue::from_str(config.license_key.inner())
            .map_err(|_| "New Relic `license_key` must be a valid HTTP header value".to_string())?;
        Ok(Self {
            license_key,
            account_id: config.account_id.inner().to_string(),
            api: config.api,
            region: config.region.unwrap_or(NewRelicRegion::Us),
            override_uri: config.override_uri.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_returns_usable_values() {
        let config: NewRelicConfig = serde_json::from_value(NewRelicConfig::generate_config())
            .expect("config should be valid");

        let validated = config.validate().expect("validation should succeed");
        assert_eq!(validated.credentials.api, NewRelicApi::Events);
        assert_eq!(validated.credentials.region, NewRelicRegion::Us);
        assert!(
            validated
                .credentials
                .get_uri()
                .to_string()
                .starts_with("https://insights-collector.newrelic.com/v1/accounts/")
        );
    }

    #[test]
    fn validate_rejects_uri_invalid_account_id() {
        let mut config: NewRelicConfig = serde_json::from_value(NewRelicConfig::generate_config())
            .expect("config should be valid");
        config.account_id = SensitiveString::from("bad id".to_string());

        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_rejects_license_key_invalid_header_value() {
        let mut config: NewRelicConfig = serde_json::from_value(NewRelicConfig::generate_config())
            .expect("config should be valid");
        config.license_key = SensitiveString::from("bad\nkey".to_string());

        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_accepts_override_uri() {
        let mut config: NewRelicConfig = serde_json::from_value(NewRelicConfig::generate_config())
            .expect("config should be valid");
        config.override_uri = Some(
            HttpEndpoint::new("https://newrelic.example.com/collector".parse().unwrap()).unwrap(),
        );

        let validated = config.validate().expect("validation should succeed");
        assert_eq!(
            validated.credentials.get_uri().to_string(),
            "https://newrelic.example.com/collector"
        );
    }

    #[test]
    fn debug_omits_credentials() {
        let mut config: NewRelicConfig = serde_json::from_value(NewRelicConfig::generate_config())
            .expect("config should be valid");
        config.license_key = SensitiveString::from("super-secret-license-key".to_string());
        config.account_id = SensitiveString::from("super-secret-account-id".to_string());

        let validated = config.validate().expect("validation should succeed");
        let debug = format!("{:?}", validated);

        assert!(!debug.contains("super-secret-license-key"));
        assert!(!debug.contains("super-secret-account-id"));
    }
}
