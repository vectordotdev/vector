use super::{
    healthcheck, Encoding, NewRelicApiResponse, NewRelicApiService, NewRelicSink, NewRelicSinkError,
};
use crate::{
    config::{DataType, SinkConfig, SinkContext},
    http::HttpClient,
    sinks::util::{
        encoding::EncodingConfigFixed, retries::RetryLogic, service::ServiceBuilderExt,
        BatchConfig, Compression, SinkBatchSettings, TowerRequestConfig,
    },
    tls::TlsSettings,
};
use futures::FutureExt;
use http::Uri;
use serde::{Deserialize, Serialize};
use std::{fmt::Debug, num::NonZeroU64, sync::Arc};
use tower::ServiceBuilder;

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Copy, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum NewRelicRegion {
    #[derivative(Default)]
    Us,
    Eu,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Copy, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum NewRelicApi {
    #[derivative(Default)]
    Events,
    Metrics,
    Logs,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NewRelicDefaultBatchSettings;

impl SinkBatchSettings for NewRelicDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(100);
    const MAX_BYTES: Option<usize> = Some(1_000_000);
    const TIMEOUT_SECS: NonZeroU64 = unsafe { NonZeroU64::new_unchecked(1) };
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

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct NewRelicConfig {
    pub license_key: String,
    pub account_id: String,
    pub region: Option<NewRelicRegion>,
    pub api: NewRelicApi,
    #[serde(default = "Compression::gzip_default")]
    pub compression: Compression,
    #[serde(
        skip_serializing_if = "crate::serde::skip_serializing_if_default",
        default
    )]
    pub encoding: EncodingConfigFixed<Encoding>,
    #[serde(default)]
    pub batch: BatchConfig<NewRelicDefaultBatchSettings>,
    #[serde(default)]
    pub request: TowerRequestConfig,
}

impl_generate_config_from_default!(NewRelicConfig);

impl NewRelicConfig {
    pub fn build_healthcheck(
        &self,
        client: HttpClient,
        credentials: Arc<NewRelicCredentials>,
    ) -> crate::Result<super::Healthcheck> {
        Ok(healthcheck(client, credentials).boxed())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "new_relic")]
impl SinkConfig for NewRelicConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let encoding = self.encoding.clone();

        let batcher_settings = self
            .batch
            .validate()?
            .limit_max_events(self.batch.max_events.unwrap_or(50))?
            .into_batcher_settings()?;

        let request_limits = self.request.unwrap_with(&Default::default());
        let tls_settings = TlsSettings::from_options(&None)?;
        let client = HttpClient::new(tls_settings, &cx.proxy)?;
        let credentials = Arc::from(NewRelicCredentials::from(self));

        let healthcheck = self.build_healthcheck(client.clone(), Arc::clone(&credentials))?;

        let service = ServiceBuilder::new()
            .settings(request_limits, NewRelicApiRetry)
            .service(NewRelicApiService { client });

        let sink = NewRelicSink {
            service,
            acker: cx.acker(),
            encoding,
            credentials,
            compression: self.compression,
            batcher_settings,
        };

        Ok((super::VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn sink_type(&self) -> &'static str {
        "new_relic"
    }
}

#[derive(Debug, Clone)]
pub struct NewRelicCredentials {
    pub license_key: String,
    pub account_id: String,
    pub api: NewRelicApi,
    pub region: NewRelicRegion,
}

impl NewRelicCredentials {
    pub fn get_uri(&self) -> Uri {
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
            license_key: config.license_key.clone(),
            account_id: config.account_id.clone(),
            api: config.api,
            region: config.region.unwrap_or(NewRelicRegion::Us),
        }
    }
}
