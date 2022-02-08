use futures::FutureExt;
use indoc::indoc;
use serde::{Deserialize, Serialize};
use tower::ServiceBuilder;
use vector_core::config::proxy::ProxyConfig;

use crate::{
    config::{DataType, GenerateConfig, SinkConfig, SinkContext},
    http::HttpClient,
    sinks::{
        datadog::{
            events::{
                service::{DatadogEventsResponse, DatadogEventsService},
                sink::DatadogEventsSink,
            },
            get_api_base_endpoint, get_api_validate_endpoint, healthcheck, Region,
        },
        util::{http::HttpStatusRetryLogic, ServiceBuilderExt, TowerRequestConfig},
        Healthcheck, VectorSink,
    },
    tls::TlsConfig,
};

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct DatadogEventsConfig {
    pub endpoint: Option<String>,
    // Deprecated, replaced by the site option
    pub region: Option<Region>,
    pub site: Option<String>,
    pub default_api_key: String,

    // Deprecated, not sure it actually makes sense to allow messing with TLS configuration?
    pub tls: Option<TlsConfig>,

    #[serde(default)]
    pub request: TowerRequestConfig,
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
    fn get_api_events_endpoint(&self) -> String {
        let api_base_endpoint =
            get_api_base_endpoint(self.endpoint.as_ref(), self.site.as_ref(), self.region);
        format!("{}/api/v1/events", api_base_endpoint)
    }

    fn build_client(&self, proxy: &ProxyConfig) -> crate::Result<HttpClient> {
        let client = HttpClient::new(None, proxy)?;
        Ok(client)
    }

    fn build_healthcheck(&self, client: HttpClient) -> crate::Result<Healthcheck> {
        let validate_endpoint =
            get_api_validate_endpoint(self.endpoint.as_ref(), self.site.as_ref(), self.region)?;
        Ok(healthcheck(client, validate_endpoint, self.default_api_key.clone()).boxed())
    }

    fn build_sink(&self, client: HttpClient, cx: SinkContext) -> crate::Result<VectorSink> {
        let service = DatadogEventsService::new(
            self.get_api_events_endpoint(),
            self.default_api_key.clone(),
            client,
        );

        let request_opts = self.request;
        let request_settings = request_opts.unwrap_with(&TowerRequestConfig::default());
        let retry_logic = HttpStatusRetryLogic::new(|req: &DatadogEventsResponse| req.http_status);

        let service = ServiceBuilder::new()
            .settings(request_settings, retry_logic)
            .service(service);

        let sink = DatadogEventsSink {
            service,
            acker: cx.acker(),
        };

        Ok(VectorSink::from_event_streamsink(sink))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "datadog_events")]
impl SinkConfig for DatadogEventsConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let client = self.build_client(cx.proxy())?;
        let healthcheck = self.build_healthcheck(client.clone())?;
        let sink = self.build_sink(client, cx)?;

        Ok((sink, healthcheck))
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
