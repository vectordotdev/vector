use super::service::LogApiRetry;
use crate::config::{DataType, GenerateConfig, SinkConfig, SinkContext};
use crate::http::HttpClient;
use crate::sinks::datadog::logs::healthcheck::healthcheck;
use crate::sinks::datadog::logs::service::LogApiService;
use crate::sinks::datadog::logs::sink::LogSink;
use crate::sinks::datadog::Region;
use crate::sinks::util::encoding::EncodingConfigWithDefault;
use crate::sinks::util::service::ServiceBuilderExt;
use crate::sinks::util::Concurrency;
use crate::sinks::util::{BatchConfig, Compression, TowerRequestConfig};
use crate::sinks::{Healthcheck, VectorSink};
use crate::tls::{MaybeTlsSettings, TlsConfig};
use futures::FutureExt;
use indoc::indoc;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceBuilder;
use vector_core::config::proxy::ProxyConfig;

const DEFAULT_REQUEST_LIMITS: TowerRequestConfig = {
    TowerRequestConfig::const_new(Concurrency::Fixed(50), Concurrency::Fixed(50))
        .rate_limit_num(250)
};

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
    pub(crate) encoding: EncodingConfigWithDefault<Encoding>,
    tls: Option<TlsConfig>,

    #[serde(default)]
    compression: Option<Compression>,

    #[serde(default)]
    batch: BatchConfig,

    #[serde(default)]
    request: TowerRequestConfig,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum Encoding {
    #[derivative(Default)]
    Json,
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
                    .map(|s| format!("https://http-intake.logs.{}/v1/input", s))
            })
            .unwrap_or_else(|| match self.region {
                Some(Region::Eu) => "https://http-intake.logs.datadoghq.eu/v1/input".to_string(),
                None | Some(Region::Us) => {
                    "https://http-intake.logs.datadoghq.com/v1/input".to_string()
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
        let request_limits = self.request.unwrap_with(&DEFAULT_REQUEST_LIMITS);
        let batch_timeout = self.batch.timeout_secs.map(Duration::from_secs);

        let service = ServiceBuilder::new()
            .settings(request_limits, LogApiRetry)
            .service(LogApiService::new(client, self.get_uri()));
        let sink = LogSink::new(service, cx)
            .batch_timeout(batch_timeout)
            .encoding(self.encoding.clone())
            .default_api_key(default_api_key)
            .compression(self.compression.unwrap_or_default())
            .log_schema(vector_core::config::log_schema())
            .build();

        Ok(VectorSink::Stream(Box::new(sink)))
    }

    pub fn build_healthcheck(&self, client: HttpClient) -> crate::Result<Healthcheck> {
        let healthcheck =
            healthcheck(client, self.get_uri(), self.default_api_key.clone()).boxed();
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

    //     let tls_settings = MaybeTlsSettings::from_config(
    //         &Some(self.tls.clone().unwrap_or_else(TlsConfig::enabled)),
    //         false,
    //     )?;

    //     let request_settings = self.request.unwrap_with(&TowerRequestConfig::default());
    //     let client = HttpClient::new(tls_settings, cx.proxy())?;
    //     let healthcheck =
    //         healthcheck(client.clone(), self.get_uri(), self.default_api_key.clone()).boxed();

    //     let mut client = ServiceBuilder::new()
    //         .rate_limit(
    //             request_settings.rate_limit_num,
    //             request_settings.rate_limit_duration,
    //         )
    //         // Ideally we'd use ARC but the type constraints don't quite line
    //         // up. We'll address that in a follow-up pull request.
    //         //
    //         // TODO we are supposed to call poll_ready before the Service is
    //         // safe to call but once concurrency_limit is introduced the whole
    //         // thing falls apart and vector dies with 'max requests in-flight;
    //         // poll_ready must be called first'
    //         //
    //         // .concurrency_limit(1024)
    //         .retry(LogApiRetry)
    //         .timeout(request_settings.timeout)
    //         .service(client);

    //     // Before we start we need to prime the pump on our http client.
    //     poll_fn(|cx| client.poll_ready(cx)).await?;

    //     let default_api_key: Arc<str> = Arc::from(self.default_api_key.clone().as_str());
    //     let log_api = LogApi::new();
    //     let log_api = if let Some(batch_timeout) = self.batch.timeout_secs {
    //         log_api.batch_timeout(Duration::from_secs(batch_timeout))
    //     } else {
    //         log_api
    //     };
    //     let log_api = log_api
    //         .bytes_stored_limit(
    //             self.batch
    //                 .max_bytes
    //                 .map(|bytes| bytes as u64)
    //                 .unwrap_or_else(|| bytesize::mib(5_u32) as u64),
    //         )
    //         .compression(self.compression.unwrap_or_default())
    //         .datadog_uri(self.get_uri())
    //         .default_api_key(default_api_key)
    //         .encoding(self.encoding.clone())
    //         .http_client(client)
    //         .log_schema(vector_core::config::log_schema())
    //         .build()?;
    //     let sink = VectorSink::Stream(Box::new(log_api));

    //     Ok((sink, healthcheck))
    // }

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
