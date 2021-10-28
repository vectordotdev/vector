use crate::config::{DataType, GenerateConfig, SinkConfig, SinkContext};
use crate::sinks::{Healthcheck, VectorSink};

use crate::http::HttpClient;
use crate::sinks::datadog::events::service::{DatadogEventsResponse, DatadogEventsService};
use crate::sinks::datadog::events::sink::DatadogEventsSink;
use crate::sinks::datadog::healthcheck;
use crate::sinks::util::http::HttpStatusRetryLogic;
use crate::sinks::util::ServiceBuilderExt;
use crate::sinks::util::TowerRequestConfig;
use crate::tls::{MaybeTlsSettings, TlsConfig};
use futures::FutureExt;
use indoc::indoc;
use serde::{Deserialize, Serialize};
use tower::ServiceBuilder;

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
        let tls_settings = MaybeTlsSettings::from_config(
            &Some(self.tls.clone().unwrap_or_else(TlsConfig::enabled)),
            false,
        )?;

        let http_client = HttpClient::new(tls_settings, cx.proxy())?;

        let service =
            DatadogEventsService::new(&self.get_uri(), &self.default_api_key, http_client.clone());

        let request_opts = self.request;
        let request_settings = request_opts.unwrap_with(&TowerRequestConfig::default());

        let healthcheck = healthcheck(
            self.get_api_endpoint(),
            self.default_api_key.clone(),
            http_client,
        )
        .boxed();

        let retry_logic = HttpStatusRetryLogic::new(|req: &DatadogEventsResponse| req.http_status);

        let service = ServiceBuilder::new()
            .settings(request_settings, retry_logic)
            .service(service);

        let sink = DatadogEventsSink {
            service,
            acker: cx.acker(),
        };

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
