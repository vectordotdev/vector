use std::{fmt::Debug, sync::Arc};

use http::Uri;
use tower::ServiceBuilder;
use vector_lib::sensitive_string::SensitiveString;

use super::{
    healthcheck, NewRelicApiResponse, NewRelicApiService, NewRelicEncoder, NewRelicSink,
    NewRelicSinkError,
};

use crate::{http::HttpClient, sinks::prelude::*};

/// New Relic region.
#[configurable_component]
#[derive(Clone, Copy, Debug, Derivative, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum NewRelicRegion {
    /// US region.
    #[derivative(Default)]
    Us,

    /// EU region.
    Eu,
}

/// New Relic API endpoint.
#[configurable_component]
#[derive(Clone, Copy, Derivative, Debug, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum NewRelicApi {
    /// Events API.
    #[derivative(Default)]
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

    #[configurable(derived)]
    pub region: Option<NewRelicRegion>,

    #[configurable(derived)]
    pub api: NewRelicApi,

    #[configurable(derived)]
    #[serde(default = "Compression::gzip_default")]
    pub compression: Compression,

    #[configurable(derived)]
    #[serde(default, skip_serializing_if = "crate::serde::is_default")]
    pub encoding: Transformer,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<NewRelicDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    acknowledgements: AcknowledgementsConfig,

    #[serde(skip)]
    pub override_uri: Option<Uri>,
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
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let batcher_settings = self
            .batch
            .validate()?
            .limit_max_events(self.batch.max_events.unwrap_or(100))?
            .into_batcher_settings()?;

        let request_limits = self.request.into_settings();
        let tls_settings = TlsSettings::from_options(&None)?;
        let client = HttpClient::new(tls_settings, &cx.proxy)?;
        let credentials = Arc::from(NewRelicCredentials::from(self));

        let healthcheck = self.build_healthcheck(client.clone(), Arc::clone(&credentials))?;

        let service = ServiceBuilder::new()
            .settings(request_limits, NewRelicApiRetry)
            .service(NewRelicApiService { client });

        let sink = NewRelicSink {
            service,
            encoder: NewRelicEncoder {
                transformer: self.encoding.clone(),
                credentials: Arc::clone(&credentials),
            },
            credentials,
            compression: self.compression,
            batcher_settings,
        };

        Ok((super::VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::new(DataType::Log | DataType::Metric)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[derive(Debug, Clone)]
pub struct NewRelicCredentials {
    pub license_key: String,
    pub account_id: String,
    pub api: NewRelicApi,
    pub region: NewRelicRegion,
    pub override_uri: Option<Uri>,
}

impl NewRelicCredentials {
    pub fn get_uri(&self) -> Uri {
        if let Some(override_uri) = self.override_uri.as_ref() {
            return override_uri.clone();
        }

        match self.api {
            NewRelicApi::Events => match self.region {
                NewRelicRegion::Us => format!(
                    "https://insights-collector.newrelic.com/v1/accounts/{}/events",
                    self.account_id
                )
                .parse::<Uri>()
                .unwrap(),
                NewRelicRegion::Eu => format!(
                    "https://insights-collector.eu01.nr-data.net/v1/accounts/{}/events",
                    self.account_id
                )
                .parse::<Uri>()
                .unwrap(),
            },
            NewRelicApi::Metrics => match self.region {
                NewRelicRegion::Us => Uri::from_static("https://metric-api.newrelic.com/metric/v1"),
                NewRelicRegion::Eu => {
                    Uri::from_static("https://metric-api.eu.newrelic.com/metric/v1")
                }
            },
            NewRelicApi::Logs => match self.region {
                NewRelicRegion::Us => Uri::from_static("https://log-api.newrelic.com/log/v1"),
                NewRelicRegion::Eu => Uri::from_static("https://log-api.eu.newrelic.com/log/v1"),
            },
        }
    }
}

impl From<&NewRelicConfig> for NewRelicCredentials {
    fn from(config: &NewRelicConfig) -> Self {
        Self {
            license_key: config.license_key.inner().to_string(),
            account_id: config.account_id.inner().to_string(),
            api: config.api,
            region: config.region.unwrap_or(NewRelicRegion::Us),
            override_uri: config.override_uri.clone(),
        }
    }
}
