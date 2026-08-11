use http::Uri;
use indoc::indoc;
use tower::ServiceBuilder;
use vector_lib::{config::proxy::ProxyConfig, configurable::configurable_component, schema};
use vrl::value::Kind;

use super::{
    service::{DatadogEventsResponse, DatadogEventsService},
    sink::DatadogEventsSink,
};
use crate::{
    common::datadog,
    config::{
        AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext, ValidatedSink,
    },
    http::HttpClient,
    sinks::{
        Healthcheck, VectorSink,
        datadog::{DatadogCommonConfig, LocalDatadogCommonConfig},
        util::{
            ServiceBuilderExt, TowerRequestConfig, TowerRequestSettings,
            http::{HttpStatusRetryLogic, RetryStrategy},
        },
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

    #[configurable(derived)]
    #[serde(default)]
    pub retry_strategy: RetryStrategy,
}

impl GenerateConfig for DatadogEventsConfig {
    fn generate_config() -> serde_json::Value {
        toml::from_str(indoc! {r#"
            default_api_key = "${DATADOG_API_KEY_ENV_VAR}"
        "#})
        .unwrap()
    }
}

impl DatadogEventsConfig {
    /// Resolve the events API endpoint URI from the given endpoint/site.
    fn events_endpoint(endpoint: Option<&str>, site: &str) -> crate::Result<Uri> {
        let base = datadog::get_api_base_endpoint(endpoint, site);
        [&base, "/api/v1/events"]
            .join("")
            .parse()
            .map_err(Into::into)
    }

    fn build_client(&self, proxy: &ProxyConfig) -> crate::Result<HttpClient> {
        let tls = MaybeTlsSettings::from_config(self.dd_common.tls.as_ref(), false)?;
        let client = HttpClient::new(tls, proxy)?;
        Ok(client)
    }

    fn build_sink(
        &self,
        dd_common: &DatadogCommonConfig,
        client: HttpClient,
        validated: &ValidatedEvents,
        endpoint: Uri,
    ) -> crate::Result<VectorSink> {
        let service =
            DatadogEventsService::new(endpoint, dd_common.default_api_key.clone(), client);

        let request_settings = validated.request_settings.clone();
        let retry_logic = HttpStatusRetryLogic::new(
            |req: &DatadogEventsResponse| req.http_status,
            self.retry_strategy.clone(),
        );

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

#[derive(Clone, Debug)]
pub struct ValidatedEvents {
    request_settings: TowerRequestSettings,
}

#[async_trait::async_trait]
impl ValidatedSink for DatadogEventsConfig {
    type Validated = ValidatedEvents;

    fn validate(&self) -> crate::Result<ValidatedEvents> {
        let site = self
            .dd_common
            .site
            .clone()
            .unwrap_or_else(datadog::default_site);
        Self::events_endpoint(self.dd_common.endpoint.as_deref(), &site)?;
        let request_settings = self.request.into_settings();
        Ok(ValidatedEvents { request_settings })
    }

    async fn build(
        &self,
        validated: &ValidatedEvents,
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let client = self.build_client(cx.proxy())?;
        let global = cx.extra_context.get_or_default::<datadog::Options>();
        let dd_common = self.dd_common.with_globals(global)?;
        let healthcheck = dd_common.build_healthcheck(client.clone())?;
        let endpoint = Self::events_endpoint(dd_common.endpoint.as_deref(), &dd_common.site)?;
        let sink = self.build_sink(&dd_common, client, validated, endpoint)?;

        Ok((sink, healthcheck))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<DatadogEventsConfig>();
    }

    #[test]
    fn validate_produces_usable_state() {
        let config = DatadogEventsConfig {
            dd_common: LocalDatadogCommonConfig::new(
                Some("http://127.0.0.1:8080".to_string()),
                None,
                None,
            ),
            ..Default::default()
        };
        config.validate().expect("validation should succeed");
        // The delivery endpoint is derived at build time after globals; the
        // pure endpoint resolution still produces the expected events URI.
        assert_eq!(
            DatadogEventsConfig::events_endpoint(
                Some("http://127.0.0.1:8080"),
                &datadog::default_site(),
            )
            .expect("endpoint should parse")
            .to_string(),
            "http://127.0.0.1:8080/api/v1/events"
        );
    }

    #[test]
    fn validate_rejects_malformed_endpoint() {
        let config = DatadogEventsConfig {
            dd_common: LocalDatadogCommonConfig::new(Some("not a uri".to_string()), None, None),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }
}
