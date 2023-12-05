use indoc::indoc;
use tower::ServiceBuilder;
use vector_lib::config::proxy::ProxyConfig;
use vector_lib::configurable::configurable_component;
use vector_lib::schema;
use vrl::value::Kind;

use crate::{
    common::datadog,
    config::{AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext},
    http::HttpClient,
    sinks::{
        datadog::{
            events::{
                service::{DatadogEventsResponse, DatadogEventsService},
                sink::DatadogEventsSink,
            },
            get_api_base_endpoint, DatadogCommonConfig, LocalDatadogCommonConfig,
        },
        util::{http::HttpStatusRetryLogic, ServiceBuilderExt, TowerRequestConfig},
        Healthcheck, VectorSink,
    },
    tls::MaybeTlsSettings,
};

/// Configuration for the `datadog_events` sink.
#[configurable_component(sink(
    "datadog_events",
    "Publish observability events to the Datadog Events API."
))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct DatadogEventsConfig {
    #[serde(flatten)]
    pub dd_common: LocalDatadogCommonConfig,

    #[configurable(derived)]
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
    fn get_api_events_endpoint(&self, dd_common: &DatadogCommonConfig) -> http::Uri {
        let api_base_endpoint =
            get_api_base_endpoint(dd_common.endpoint.as_ref(), dd_common.site.as_str());

        // We know this URI will be valid since we have just built it up ourselves.
        http::Uri::try_from(format!("{}/api/v1/events", api_base_endpoint)).expect("URI not valid")
    }

    fn build_client(&self, proxy: &ProxyConfig) -> crate::Result<HttpClient> {
        let tls = MaybeTlsSettings::from_config(&self.dd_common.tls, false)?;
        let client = HttpClient::new(tls, proxy)?;
        Ok(client)
    }

    fn build_sink(
        &self,
        dd_common: &DatadogCommonConfig,
        client: HttpClient,
    ) -> crate::Result<VectorSink> {
        let service = DatadogEventsService::new(
            self.get_api_events_endpoint(dd_common),
            dd_common.default_api_key.clone(),
            client,
        );

        let request_opts = self.request;
        let request_settings = request_opts.into_settings();
        let retry_logic = HttpStatusRetryLogic::new(|req: &DatadogEventsResponse| req.http_status);

        let service = ServiceBuilder::new()
            .settings(request_settings, retry_logic)
            .service(service);

        let sink = DatadogEventsSink { service };

        Ok(VectorSink::from_event_streamsink(sink))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "datadog_events")]
impl SinkConfig for DatadogEventsConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let client = self.build_client(cx.proxy())?;
        let global = cx.extra_context.get_or_default::<datadog::Options>();
        let dd_common = self.dd_common.with_globals(global)?;
        let healthcheck = dd_common.build_healthcheck(client.clone())?;
        let sink = self.build_sink(&dd_common, client)?;

        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        let requirement = schema::Requirement::empty()
            .required_meaning("message", Kind::bytes())
            .optional_meaning("host", Kind::bytes())
            .optional_meaning("timestamp", Kind::timestamp());

        Input::log().with_schema_requirement(requirement)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.dd_common.acknowledgements
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
