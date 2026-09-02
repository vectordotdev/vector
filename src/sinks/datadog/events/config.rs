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
            HttpEndpoint, ServiceBuilderExt, TowerRequestConfig, TowerRequestSettings,
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

    #[serde(default)]
    pub request: TowerRequestConfig,

    #[serde(default)]
    pub retry_strategy: RetryStrategy,
}

impl GenerateConfig for DatadogEventsConfig {
    fn generate_config() -> serde_json::Value {
        serde_yaml::from_str(indoc! {r#"
            default_api_key: ${DATADOG_API_KEY_ENV_VAR}
        "#})
        .unwrap()
    }
}

impl DatadogEventsConfig {
    /// Resolve the events API endpoint URI from the given endpoint/site.
    fn events_endpoint(endpoint: Option<&str>, site: &str) -> crate::Result<Uri> {
        let base = datadog::get_api_base_endpoint(endpoint, site);
        Ok(HttpEndpoint::parse(&base)?
            .append_path("/api/v1/events")?
            .into_uri())
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
            .unwrap_or_else(|| datadog::DD_US_SITE.to_owned());
        let uri = Self::events_endpoint(self.dd_common.endpoint.as_deref(), &site)?;
        if !matches!(uri.scheme_str(), Some("http" | "https")) || uri.authority().is_none() {
            return Err("Datadog Events endpoint must be an absolute http(s) URL".into());
        }
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
                datadog::DD_US_SITE,
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

    #[test]
    fn validate_accepts_endpoint_without_scheme() {
        let config = DatadogEventsConfig {
            dd_common: LocalDatadogCommonConfig::new(
                Some("localhost:8080".to_string()),
                None,
                None,
            ),
            ..Default::default()
        };
        config.validate().expect("validation should succeed");
        // A missing scheme is defaulted to https, matching the shared Datadog
        // endpoint contract used by the healthcheck and other Datadog sinks.
        assert_eq!(
            DatadogEventsConfig::events_endpoint(Some("localhost:8080"), datadog::DD_US_SITE)
                .expect("endpoint should parse")
                .to_string(),
            "https://localhost:8080/api/v1/events"
        );
    }
}
