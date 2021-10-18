use crate::config::{SinkConfig, SinkContext, DataType, GenerateConfig};
use crate::sinks::{VectorSink, Healthcheck};
use std::time::Duration;
use vector_core::ByteSizeOf;
use crate::sinks::util::{Batch, PartitionInnerBuffer, Concurrency, TowerRequestConfig, PartitionBuffer, BatchSettings, BatchConfig, JsonArrayBuffer};
use crate::sinks::util::http::{HttpSink, PartitionHttpSink};
use crate::sinks::datadog::{ApiKey, healthcheck};
use crate::tls::{MaybeTlsSettings, TlsConfig};
use crate::http::HttpClient;
use futures::{FutureExt, SinkExt};
use serde::{Serialize, Deserialize};
use crate::sinks::datadog::events::service::DatadogEventsService;
use indoc::indoc;
use crate::sinks::datadog::events::sink::DatadogEventsSink;
use tower::ServiceBuilder;
use crate::sinks::util::ServiceBuilderExt;
use crate::sinks::util::encoding::{EncodingConfigWithDefault, TimestampFormat, EncodingConfigFixed, StandardJsonEncoding};


#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct DatadogEventsConfig {
    pub endpoint: Option<String>,

    #[serde(default = "default_site")]
    pub site: String,
    pub default_api_key: String,

    pub tls: Option<TlsConfig>,

    #[serde(default)]
    pub request: TowerRequestConfig,
}

fn default_site() -> String {
    "datadoghq.com".to_owned()
}

impl GenerateConfig for DatadogEventsConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(indoc! {r#"
            default_api_key = "${DATADOG_API_KEY_ENV_VAR}"
        "#})
            .unwrap()
    }
}

impl DatadogEventsConfig {
    pub fn get_uri(&self) -> String {
        format!("{}/api/v1/events", self.get_api_endpoint())
    }

    pub fn get_api_endpoint(&self) -> String {
        self.endpoint
            .clone()
            .unwrap_or_else(|| format!("https://api.{}", &self.site))
    }
}


#[async_trait::async_trait]
#[typetag::serde(name = "datadog_events")]
impl SinkConfig for DatadogEventsConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        // Datadog Event API doesn't support batching.
        // let batch_settings = BatchSettings::default()
        //     .bytes(100_000)
        //     .events(1)
        //     .timeout(0)
        //     .parse_config(BatchConfig::default())?;

        let tls_settings = MaybeTlsSettings::from_config(
            &Some(self.tls.clone().unwrap_or_else(TlsConfig::enabled)),
            false,
        )?;

        let client = HttpClient::new(tls_settings, cx.proxy())?;

        let service = DatadogEventsService::new(
            &self.get_uri(),
            &self.default_api_key,
            client.clone()
        );

        let mut request_opts = self.request;
        let request_settings = request_opts.unwrap_with(&TowerRequestConfig::default());


        let healthcheck = healthcheck(
            self.get_api_endpoint(),
            self.default_api_key.clone(),
            client.clone(),
        )
            .boxed();
        // let sink = PartitionHttpSink::new(
        //     service,
        //     PartitionBuffer::new(batch),
        //     request_settings,
        //     timeout,
        //     client,
        //     cx.acker(),
        // )
        //     .sink_map_err(|error| error!(message = "Fatal datadog_events sink error.", %error));

        let sink = DatadogEventsSink {
            service
        };

        let service = ServiceBuilder::new()
            .settings(request_settings, DatadogEventsRetryLogic)
            .service(ElasticSearchService::new(http_client, http_request_builder));

        Ok((VectorSink::Stream(Box::new(sink)), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "datadog_events"
    }
}

#[cfg(test)]
mod tests {
    use crate::sinks::datadog::events::config::DatadogEventsConfig;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<DatadogEventsConfig>();
    }
}
