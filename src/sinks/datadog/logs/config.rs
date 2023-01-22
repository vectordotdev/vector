use std::{convert::TryFrom, sync::Arc};

use futures::FutureExt;
use indoc::indoc;
use tower::ServiceBuilder;
use value::Kind;
use vector_common::sensitive_string::SensitiveString;
use vector_config::configurable_component;
use vector_core::config::proxy::ProxyConfig;

use super::{service::LogApiRetry, sink::LogSinkBuilder};
use crate::{
    codecs::Transformer,
    config::{AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext},
    http::HttpClient,
    schema,
    sinks::{
        datadog::{get_api_validate_endpoint, healthcheck, logs::service::LogApiService, Region},
        util::{
            http::RequestConfig, service::ServiceBuilderExt, BatchConfig, Compression,
            SinkBatchSettings,
        },
        Healthcheck, VectorSink,
    },
    tls::{MaybeTlsSettings, TlsEnableableConfig},
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
pub const BATCH_DEFAULT_TIMEOUT_SECS: f64 = 5.0;

#[derive(Clone, Copy, Debug, Default)]
pub struct DatadogLogsDefaultBatchSettings;

impl SinkBatchSettings for DatadogLogsDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(BATCH_MAX_EVENTS);
    const MAX_BYTES: Option<usize> = Some(BATCH_GOAL_BYTES);
    const TIMEOUT_SECS: f64 = BATCH_DEFAULT_TIMEOUT_SECS;
}

/// Configuration for the `datadog_logs` sink.
#[configurable_component(sink("datadog_logs"))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct DatadogLogsConfig {
    /// The endpoint to send logs to.
    pub(crate) endpoint: Option<String>,

    /// The Datadog region to send logs to.
    ///
    /// This option is deprecated, and the `site` field should be used instead.
    #[configurable(deprecated)]
    pub region: Option<Region>,

    /// The Datadog [site][dd_site] to send logs to.
    ///
    /// [dd_site]: https://docs.datadoghq.com/getting_started/site
    pub site: Option<String>,

    /// The default Datadog [API key][api_key] to send logs with.
    ///
    /// If a log has a Datadog [API key][api_key] set explicitly in its metadata, it will take
    /// precedence over the default.
    ///
    /// [api_key]: https://docs.datadoghq.com/api/?lang=bash#authentication
    #[serde(alias = "api_key")]
    pub default_api_key: SensitiveString,

    #[configurable(derived)]
    pub tls: Option<TlsEnableableConfig>,

    #[configurable(derived)]
    #[serde(default)]
    pub compression: Option<Compression>,

    #[configurable(derived)]
    #[serde(
        default,
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub encoding: Transformer,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<DatadogLogsDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: RequestConfig,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
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

    fn get_protocol(&self) -> String {
        self.get_uri().scheme_str().unwrap_or("http").to_string()
    }

    pub fn build_processor(&self, client: HttpClient) -> crate::Result<VectorSink> {
        let default_api_key: Arc<str> = Arc::from(self.default_api_key.inner());
        let request_limits = self.request.tower.unwrap_with(&Default::default());

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
                self.request.headers.clone(),
            )?);

        let encoding = self.encoding.clone();
        let protocol = self.get_protocol();
        let sink = LogSinkBuilder::new(encoding, service, default_api_key, batch, protocol)
            .compression(self.compression.unwrap_or_default())
            .build();

        Ok(VectorSink::from_event_streamsink(sink))
    }

    pub fn build_healthcheck(&self, client: HttpClient) -> crate::Result<Healthcheck> {
        let validate_endpoint =
            get_api_validate_endpoint(self.endpoint.as_ref(), self.site.as_ref(), self.region)?;
        Ok(healthcheck(
            client,
            validate_endpoint,
            self.default_api_key.inner().to_owned(),
        )
        .boxed())
    }

    pub fn create_client(&self, proxy: &ProxyConfig) -> crate::Result<HttpClient> {
        let tls_settings = MaybeTlsSettings::from_config(
            &Some(
                self.tls
                    .clone()
                    .unwrap_or_else(TlsEnableableConfig::enabled),
            ),
            false,
        )?;
        Ok(HttpClient::new(tls_settings, proxy)?)
    }
}

#[async_trait::async_trait]
impl SinkConfig for DatadogLogsConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let client = self.create_client(&cx.proxy)?;
        let healthcheck = self.build_healthcheck(client.clone())?;
        let sink = self.build_processor(client)?;
        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        let requirement = schema::Requirement::empty()
            .required_meaning("message", Kind::bytes())
            .required_meaning("timestamp", Kind::timestamp())
            .optional_meaning("host", Kind::bytes())
            .optional_meaning("source", Kind::bytes())
            .optional_meaning("severity", Kind::bytes())
            .optional_meaning("service", Kind::bytes())
            .optional_meaning("trace_id", Kind::bytes());

        Input::log().with_schema_requirement(requirement)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
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
