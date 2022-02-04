use std::{convert::TryFrom, num::NonZeroU64, sync::Arc};

use futures::FutureExt;
use indoc::indoc;
use serde::{Deserialize, Serialize};
use tower::ServiceBuilder;
use vector_core::config::proxy::ProxyConfig;

use super::{
    service::LogApiRetry,
    sink::{DatadogLogsJsonEncoding, LogSinkBuilder},
};
use crate::{
    config::{DataType, GenerateConfig, SinkConfig, SinkContext},
    http::HttpClient,
    sinks::{
        datadog::{get_api_validate_endpoint, healthcheck, logs::service::LogApiService, Region},
        util::{
            encoding::EncodingConfigFixed, service::ServiceBuilderExt, BatchConfig, Compression,
            SinkBatchSettings, TowerRequestConfig,
        },
        Healthcheck, VectorSink,
    },
    tls::{MaybeTlsSettings, TlsConfig},
};

// The Datadog API has a hard limit of 5MB for uncompressed payloads. Above this
// threshold the API will toss results. We previously serialized Events as they
// came in -- a very CPU intensive process -- and to avoid that we only batch up
// to 750KB below the max and then build our payloads. This does mean that in
// some situations we'll kick out over-large payloads -- for instance, a string
// of escaped double-quotes -- but we believe this should be very rare in
// practice.
pub const MAX_PAYLOAD_BYTES: usize = 5_000_000;
pub const BATCH_GOAL_BYTES: usize = 4_250_000;
pub const BATCH_MAX_EVENTS: usize = 1_000;
pub const BATCH_DEFAULT_TIMEOUT_SECS: u64 = 5;

#[derive(Clone, Copy, Debug, Default)]
pub struct DatadogLogsDefaultBatchSettings;

impl SinkBatchSettings for DatadogLogsDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(BATCH_MAX_EVENTS);
    const MAX_BYTES: Option<usize> = Some(BATCH_GOAL_BYTES);
    const TIMEOUT_SECS: NonZeroU64 =
        unsafe { NonZeroU64::new_unchecked(BATCH_DEFAULT_TIMEOUT_SECS) };
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct DatadogLogsConfig {
    pub(crate) endpoint: Option<String>,
    // Deprecated, replaced by the site option
    region: Option<Region>,
    site: Option<String>,
    // Deprecated name
    #[serde(alias = "api_key")]
    default_api_key: String,
    #[serde(
        skip_serializing_if = "crate::serde::skip_serializing_if_default",
        default
    )]
    encoding: EncodingConfigFixed<DatadogLogsJsonEncoding>,
    tls: Option<TlsConfig>,

    #[serde(default)]
    compression: Option<Compression>,

    #[serde(default)]
    batch: BatchConfig<DatadogLogsDefaultBatchSettings>,

    #[serde(default)]
    request: TowerRequestConfig,
}

impl GenerateConfig for DatadogLogsConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(indoc! {r#"
            default_api_key = "${DATADOG_API_KEY_ENV_VAR}"
        "#})
        .unwrap()
    }
}

impl DatadogLogsConfig {
    // TODO: We should probably hoist this type of base URI generation so that all DD sinks can
    // utilize it, since it all follows the same pattern.
    fn get_uri(&self) -> http::Uri {
        let endpoint = self
            .endpoint
            .clone()
            .or_else(|| {
                self.site
                    .as_ref()
                    .map(|s| format!("https://http-intake.logs.{}/api/v2/logs", s))
            })
            .unwrap_or_else(|| match self.region {
                Some(Region::Eu) => "https://http-intake.logs.datadoghq.eu/api/v2/logs".to_string(),
                None | Some(Region::Us) => {
                    "https://http-intake.logs.datadoghq.com/api/v2/logs".to_string()
                }
            });
        http::Uri::try_from(endpoint).expect("URI not valid")
    }
}

impl DatadogLogsConfig {
    pub fn build_processor(
        &self,
        client: HttpClient,
        cx: SinkContext,
    ) -> crate::Result<VectorSink> {
        let default_api_key: Arc<str> = Arc::from(self.default_api_key.clone().as_str());
        let request_limits = self.request.unwrap_with(&Default::default());

        // We forcefully cap the provided batch configuration to the size/log line limits imposed by
        // the Datadog Logs API, but we still allow them to be lowered if need be.
        let batch = self
            .batch
            .validate()?
            .limit_max_bytes(BATCH_GOAL_BYTES)?
            .limit_max_events(BATCH_MAX_EVENTS)?
            .into_batcher_settings()?;

        let service = ServiceBuilder::new()
            .settings(request_limits, LogApiRetry)
            .service(LogApiService::new(
                client,
                self.get_uri(),
                cx.globals.enterprise,
            ));
        let sink = LogSinkBuilder::new(service, cx, default_api_key, batch)
            .encoding(self.encoding.clone())
            .compression(self.compression.unwrap_or_default())
            .build();

        Ok(VectorSink::from_event_streamsink(sink))
    }

    pub fn build_healthcheck(&self, client: HttpClient) -> crate::Result<Healthcheck> {
        let validate_endpoint =
            get_api_validate_endpoint(self.endpoint.as_ref(), self.site.as_ref(), self.region)?;
        Ok(healthcheck(client, validate_endpoint, self.default_api_key.clone()).boxed())
    }

    pub fn create_client(&self, proxy: &ProxyConfig) -> crate::Result<HttpClient> {
        let tls_settings = MaybeTlsSettings::from_config(
            &Some(self.tls.clone().unwrap_or_else(TlsConfig::enabled)),
            false,
        )?;
        Ok(HttpClient::new(tls_settings, proxy)?)
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "datadog_logs")]
impl SinkConfig for DatadogLogsConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let client = self.create_client(&cx.proxy)?;
        let healthcheck = self.build_healthcheck(client.clone())?;
        let sink = self.build_processor(client, cx)?;
        Ok((sink, healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "datadog_logs"
    }
}

#[cfg(test)]
mod test {
    use crate::sinks::datadog::logs::DatadogLogsConfig;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<DatadogLogsConfig>();
    }
}
