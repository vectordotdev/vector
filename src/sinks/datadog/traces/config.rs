use std::sync::{Arc, Mutex};

use futures::FutureExt;
use http::Uri;
use indoc::indoc;
use snafu::ResultExt;
use tokio::sync::oneshot::{channel, Sender};
use tower::ServiceBuilder;
use vector_common::sensitive_string::SensitiveString;
use vector_config::configurable_component;
use vector_core::config::{proxy::ProxyConfig, AcknowledgementsConfig};

use super::{
    apm_stats::{flush_apm_stats_thread, Aggregator},
    service::TraceApiRetry,
};
use crate::{
    config::{GenerateConfig, Input, SinkConfig, SinkContext},
    http::HttpClient,
    sinks::{
        datadog::{
            default_site, get_api_validate_endpoint, healthcheck,
            traces::{
                request_builder::DatadogTracesRequestBuilder, service::TraceApiService,
                sink::TracesSink,
            },
        },
        util::{
            service::ServiceBuilderExt, BatchConfig, Compression, SinkBatchSettings,
            TowerRequestConfig,
        },
        Healthcheck, UriParseSnafu, VectorSink,
    },
    tls::{MaybeTlsSettings, TlsEnableableConfig},
};

// The Datadog API has a hard limit of 3.2MB for uncompressed payloads.
// Beyond this limit the payload will be ignored, enforcing a slight lower
// limit as a safety margin.
pub const BATCH_GOAL_BYTES: usize = 3_000_000;
pub const BATCH_MAX_EVENTS: usize = 1_000;
pub const BATCH_DEFAULT_TIMEOUT_SECS: f64 = 10.0;

pub const PAYLOAD_LIMIT: usize = 3_200_000;

const DEFAULT_REQUEST_RETRY_ATTEMPTS: usize = 5;
const DEFAULT_REQUEST_RETRY_MAX_DURATION_SECS: u64 = 300;

#[derive(Clone, Copy, Debug, Default)]
pub struct DatadogTracesDefaultBatchSettings;

impl SinkBatchSettings for DatadogTracesDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(BATCH_MAX_EVENTS);
    const MAX_BYTES: Option<usize> = Some(BATCH_GOAL_BYTES);
    const TIMEOUT_SECS: f64 = BATCH_DEFAULT_TIMEOUT_SECS;
}

/// Configuration for the `datadog_traces` sink.
#[configurable_component(sink("datadog_traces"))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct DatadogTracesConfig {
    /// The endpoint to send traces to.
    ///
    /// The endpoint must contain an HTTP scheme, and may specify a
    /// hostname or IP address and port.
    ///
    /// If set, overrides the `site` option.
    #[configurable(metadata(docs::advanced))]
    #[configurable(metadata(docs::examples = "http://127.0.0.1:8080"))]
    #[configurable(metadata(docs::examples = "http://example.com:12345"))]
    #[serde(default)]
    pub(crate) endpoint: Option<String>,

    /// The Datadog [site][dd_site] to send traces to.
    ///
    /// [dd_site]: https://docs.datadoghq.com/getting_started/site
    #[configurable(metadata(docs::examples = "us3.datadoghq.com"))]
    #[configurable(metadata(docs::examples = "datadoghq.eu"))]
    #[serde(default = "default_site")]
    pub site: String,

    /// The default Datadog [API key][api_key] to send traces with.
    ///
    /// If a trace has a Datadog [API key][api_key] set explicitly in its metadata, it will take
    /// precedence over this setting.
    ///
    /// [api_key]: https://docs.datadoghq.com/api/?lang=bash#authentication
    #[configurable(metadata(docs::examples = "${DATADOG_API_KEY_ENV_VAR}"))]
    #[configurable(metadata(docs::examples = "ef8d5de700e7989468166c40fc8a0ccd"))]
    pub default_api_key: SensitiveString,

    #[configurable(derived)]
    #[serde(default)]
    pub tls: Option<TlsEnableableConfig>,

    #[configurable(derived)]
    #[serde(default)]
    pub compression: Option<Compression>,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<DatadogTracesDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl GenerateConfig for DatadogTracesConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(indoc! {r#"
            default_api_key = "${DATADOG_API_KEY_ENV_VAR}"
        "#})
        .unwrap()
    }
}

/// Datadog traces API has two routes: one for traces and another one for stats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DatadogTracesEndpoint {
    Traces,
    #[allow(dead_code)] // This will be used when APM stats will be generated
    APMStats,
}

/// Store traces & APM stats endpoints actual URIs.
#[derive(Clone)]
pub struct DatadogTracesEndpointConfiguration {
    traces_endpoint: Uri,
    stats_endpoint: Uri,
}

impl DatadogTracesEndpointConfiguration {
    pub fn get_uri_for_endpoint(&self, endpoint: DatadogTracesEndpoint) -> Uri {
        match endpoint {
            DatadogTracesEndpoint::Traces => self.traces_endpoint.clone(),
            DatadogTracesEndpoint::APMStats => self.stats_endpoint.clone(),
        }
    }
}

impl DatadogTracesConfig {
    fn get_base_uri(&self) -> String {
        self.endpoint
            .clone()
            .unwrap_or_else(|| format!("https://trace.agent.{}", self.site))
    }

    fn generate_traces_endpoint_configuration(
        &self,
    ) -> crate::Result<DatadogTracesEndpointConfiguration> {
        let base_uri = self.get_base_uri();
        let traces_endpoint = build_uri(&base_uri, "/api/v0.2/traces")?;
        let stats_endpoint = build_uri(&base_uri, "/api/v0.2/stats")?;

        Ok(DatadogTracesEndpointConfiguration {
            traces_endpoint,
            stats_endpoint,
        })
    }

    pub fn build_sink(&self, client: HttpClient) -> crate::Result<VectorSink> {
        let default_api_key: Arc<str> = Arc::from(self.default_api_key.inner());
        let request_limits = self.request.unwrap_with(
            &TowerRequestConfig::default()
                .retry_attempts(DEFAULT_REQUEST_RETRY_ATTEMPTS)
                .retry_max_duration_secs(DEFAULT_REQUEST_RETRY_MAX_DURATION_SECS),
        );
        let endpoints = self.generate_traces_endpoint_configuration()?;

        let batcher_settings = self
            .batch
            .validate()?
            .limit_max_bytes(BATCH_GOAL_BYTES)?
            .limit_max_events(BATCH_MAX_EVENTS)?
            .into_batcher_settings()?;

        let service = ServiceBuilder::new()
            .settings(request_limits, TraceApiRetry)
            .service(TraceApiService::new(client.clone()));

        // Object responsible for caching/processing APM stats from incoming trace events.
        let apm_stats_aggregator =
            Arc::new(Mutex::new(Aggregator::new(Arc::clone(&default_api_key))));

        let compression = self.compression.unwrap_or_else(Compression::gzip_default);

        let request_builder = DatadogTracesRequestBuilder::new(
            Arc::clone(&default_api_key),
            endpoints.clone(),
            compression,
            PAYLOAD_LIMIT,
            Arc::clone(&apm_stats_aggregator),
        )?;

        // shutdown= Sender that the sink signals when input stream is exhausted.
        // tripwire= Receiver that APM stats flush thread listens for exit signal on.
        let (shutdown, tripwire) = channel::<Sender<()>>();

        let sink = TracesSink::new(
            service,
            request_builder,
            batcher_settings,
            shutdown,
            self.get_protocol(),
        );

        // Send the APM stats payloads independently of the sink framework.
        // This is necessary to comply with what the APM stats backend of Datadog expects with
        // respect to receiving stats payloads.
        tokio::spawn(flush_apm_stats_thread(
            tripwire,
            client,
            compression,
            endpoints,
            Arc::clone(&apm_stats_aggregator),
        ));

        Ok(VectorSink::from_event_streamsink(sink))
    }

    pub fn build_healthcheck(&self, client: HttpClient) -> crate::Result<Healthcheck> {
        let validate_endpoint =
            get_api_validate_endpoint(self.endpoint.as_ref(), self.site.as_ref())?;
        Ok(healthcheck(
            client,
            validate_endpoint,
            self.default_api_key.inner().to_string(),
        )
        .boxed())
    }

    pub fn build_client(&self, proxy: &ProxyConfig) -> crate::Result<HttpClient> {
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

    fn get_protocol(&self) -> String {
        build_uri(&self.get_base_uri(), "")
            .unwrap()
            .scheme_str()
            .unwrap_or("http")
            .to_string()
    }
}

#[async_trait::async_trait]
impl SinkConfig for DatadogTracesConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let client = self.build_client(&cx.proxy)?;
        let healthcheck = self.build_healthcheck(client.clone())?;
        let sink = self.build_sink(client)?;
        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        Input::trace()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

fn build_uri(host: &str, endpoint: &str) -> crate::Result<Uri> {
    let result = format!("{}{}", host, endpoint)
        .parse::<Uri>()
        .context(UriParseSnafu)?;
    Ok(result)
}

#[cfg(test)]
mod test {
    use super::DatadogTracesConfig;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<DatadogTracesConfig>();
    }
}
