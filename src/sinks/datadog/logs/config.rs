use super::service::LogApiRetry;
use super::sink::{DatadogLogsJsonEncoding, LogSinkBuilder};
use crate::config::{DataType, GenerateConfig, SinkConfig, SinkContext};
use crate::http::HttpClient;
use crate::sinks::datadog::logs::healthcheck::healthcheck;
use crate::sinks::datadog::logs::service::LogApiService;
use crate::sinks::datadog::Region;
use crate::sinks::util::encoding::EncodingConfigFixed;
use crate::sinks::util::service::ServiceBuilderExt;
use crate::sinks::util::{BatchConfig, Compression, TowerRequestConfig};
use crate::sinks::util::{BatchSettings, Concurrency};
use crate::sinks::{Healthcheck, VectorSink};
use crate::tls::{MaybeTlsSettings, TlsConfig};
use futures::FutureExt;
use indoc::indoc;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use std::sync::Arc;
use tower::ServiceBuilder;
use vector_core::config::proxy::ProxyConfig;

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

const DEFAULT_BATCH_SETTINGS: BatchSettings<()> = BatchSettings::const_default()
    .bytes(BATCH_GOAL_BYTES)
    .events(BATCH_MAX_EVENTS)
    .timeout(BATCH_DEFAULT_TIMEOUT_SECS);

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
    batch: BatchConfig,

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
        let request_limits = self.request.unwrap_with(&TowerRequestConfig::default());

        // We forcefully cap the provided batch configuration to the size/log line limits imposed by
        // the Datadog Logs API, but we still allow them to be lowered if need be.
        let limited_batch = self
            .batch
            .limit_max_bytes(BATCH_GOAL_BYTES)
            .limit_max_events(BATCH_MAX_EVENTS);
        let batch = DEFAULT_BATCH_SETTINGS
            .parse_config(limited_batch)?
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

        Ok(VectorSink::Stream(Box::new(sink)))
    }

    pub fn build_healthcheck(&self, client: HttpClient) -> crate::Result<Healthcheck> {
        let healthcheck = healthcheck(client, self.get_uri(), self.default_api_key.clone()).boxed();
        Ok(healthcheck)
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
