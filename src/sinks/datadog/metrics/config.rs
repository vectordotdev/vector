use futures::FutureExt;
use http::Uri;
use snafu::ResultExt;
use tower::ServiceBuilder;
use vector_common::sensitive_string::SensitiveString;
use vector_config::configurable_component;
use vector_core::config::proxy::ProxyConfig;

use super::{
    request_builder::DatadogMetricsRequestBuilder,
    service::{DatadogMetricsRetryLogic, DatadogMetricsService},
    sink::DatadogMetricsSink,
};
use crate::tls::{MaybeTlsSettings, TlsEnableableConfig};
use crate::{
    common::datadog::get_base_domain,
    config::{AcknowledgementsConfig, Input, SinkConfig, SinkContext},
    http::HttpClient,
    sinks::{
        datadog::{get_api_validate_endpoint, healthcheck, Region},
        util::{
            batch::BatchConfig, Concurrency, ServiceBuilderExt, SinkBatchSettings,
            TowerRequestConfig,
        },
        Healthcheck, UriParseSnafu, VectorSink,
    },
};

// TODO: revisit our concurrency and batching defaults
const DEFAULT_REQUEST_LIMITS: TowerRequestConfig =
    TowerRequestConfig::new(Concurrency::None).retry_attempts(5);

// This default is centered around "series" data, which should be the lion's share of what we
// process.  Given that a single series, when encoded, is in the 150-300 byte range, we can fit a
// lot of these into a single request, something like 150-200K series.  Simply to be a little more
// conservative, though, we use 100K here.  This will also get a little more tricky when it comes to
// distributions and sketches, but we're going to have to implement incremental encoding to handle
// "we've exceeded our maximum payload size, split this batch" scenarios anyways.
pub const MAXIMUM_PAYLOAD_COMPRESSED_SIZE: usize = 3_200_000;
pub const MAXIMUM_PAYLOAD_SIZE: usize = 62_914_560;

#[derive(Clone, Copy, Debug, Default)]
pub struct DatadogMetricsDefaultBatchSettings;

impl SinkBatchSettings for DatadogMetricsDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(100_000);
    const MAX_BYTES: Option<usize> = None;
    const TIMEOUT_SECS: f64 = 2.0;
}

/// Various metric type-specific API types.
///
/// Each of these corresponds to a specific request path when making a request to the agent API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DatadogMetricsEndpoint {
    Series,
    Sketches,
}

impl DatadogMetricsEndpoint {
    /// Gets the content type associated with the specific encoder for a given metric endpoint.
    pub const fn content_type(self) -> &'static str {
        match self {
            DatadogMetricsEndpoint::Series => "application/json",
            DatadogMetricsEndpoint::Sketches => "application/x-protobuf",
        }
    }
}

/// Maps Datadog metric endpoints to their actual URI.
pub struct DatadogMetricsEndpointConfiguration {
    series_endpoint: Uri,
    sketches_endpoint: Uri,
}

impl DatadogMetricsEndpointConfiguration {
    /// Creates a new `DatadogMEtricsEndpointConfiguration`.
    pub const fn new(series_endpoint: Uri, sketches_endpoint: Uri) -> Self {
        Self {
            series_endpoint,
            sketches_endpoint,
        }
    }

    /// Gets the URI for the given Datadog metrics endpoint.
    pub fn get_uri_for_endpoint(&self, endpoint: DatadogMetricsEndpoint) -> Uri {
        match endpoint {
            DatadogMetricsEndpoint::Series => self.series_endpoint.clone(),
            DatadogMetricsEndpoint::Sketches => self.sketches_endpoint.clone(),
        }
    }
}

/// Configuration for the `datadog_metrics` sink.
#[configurable_component(sink("datadog_metrics"))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct DatadogMetricsConfig {
    /// Sets the default namespace for any metrics sent.
    ///
    /// This namespace is only used if a metric has no existing namespace. When a namespace is
    /// present, it is used as a prefix to the metric name, and separated with a period (`.`).
    pub default_namespace: Option<String>,

    /// The endpoint to send metrics to.
    pub(crate) endpoint: Option<String>,

    /// The Datadog region to send metrics to.
    ///
    /// This option is deprecated, and the `site` field should be used instead.
    #[configurable(deprecated)]
    pub region: Option<Region>,

    /// The Datadog [site][dd_site] to send metrics to.
    ///
    /// [dd_site]: https://docs.datadoghq.com/getting_started/site
    pub site: Option<String>,

    /// The default Datadog [API key][api_key] to send metrics with.
    ///
    /// If a metric has a Datadog [API key][api_key] set explicitly in its metadata, it will take
    /// precedence over the default.
    ///
    /// [api_key]: https://docs.datadoghq.com/api/?lang=bash#authentication
    #[serde(alias = "api_key")]
    pub default_api_key: SensitiveString,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<DatadogMetricsDefaultBatchSettings>,

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

    #[configurable(derived)]
    pub tls: Option<TlsEnableableConfig>,
}

impl_generate_config_from_default!(DatadogMetricsConfig);

#[async_trait::async_trait]
impl SinkConfig for DatadogMetricsConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let client = self.build_client(&cx.proxy)?;
        let healthcheck = self.build_healthcheck(client.clone())?;
        let sink = self.build_sink(client)?;

        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        Input::metric()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

impl DatadogMetricsConfig {
    /// Gets the base URI of the Datadog agent API.
    ///
    /// Per the Datadog agent convention, we should include a unique identifier as part of the
    /// domain to indicate that these metrics are being submitted by Vector, including the version,
    /// likely useful for detecting if a specific version of the agent (Vector, in this case) is
    /// doing something wrong, for understanding issues from the API side.
    ///
    /// The `endpoint` configuration field will be used here if it is present.
    fn get_base_agent_endpoint(&self) -> String {
        self.endpoint.clone().unwrap_or_else(|| {
            let version = str::replace(crate::built_info::PKG_VERSION, ".", "-");
            format!(
                "https://{}-vector.agent.{}",
                version,
                get_base_domain(self.site.as_ref(), self.region)
            )
        })
    }

    /// Generates the `DatadogMetricsEndpointConfiguration`, used for mapping endpoints to their URI.
    fn generate_metrics_endpoint_configuration(
        &self,
    ) -> crate::Result<DatadogMetricsEndpointConfiguration> {
        let base_uri = self.get_base_agent_endpoint();
        let series_endpoint = build_uri(&base_uri, "/api/v1/series")?;
        let sketches_endpoint = build_uri(&base_uri, "/api/beta/sketches")?;

        Ok(DatadogMetricsEndpointConfiguration::new(
            series_endpoint,
            sketches_endpoint,
        ))
    }

    fn build_client(&self, proxy: &ProxyConfig) -> crate::Result<HttpClient> {
        let tls_settings = MaybeTlsSettings::from_config(
            &Some(
                self.tls
                    .clone()
                    .unwrap_or_else(TlsEnableableConfig::enabled),
            ),
            false,
        )?;
        let client = HttpClient::new(tls_settings, proxy)?;
        Ok(client)
    }

    fn build_healthcheck(&self, client: HttpClient) -> crate::Result<Healthcheck> {
        let validate_endpoint =
            get_api_validate_endpoint(self.endpoint.as_ref(), self.site.as_ref(), self.region)?;
        Ok(healthcheck(
            client,
            validate_endpoint,
            self.default_api_key.inner().to_string(),
        )
        .boxed())
    }

    fn build_sink(&self, client: HttpClient) -> crate::Result<VectorSink> {
        let batcher_settings = self.batch.into_batcher_settings()?;

        let request_limits = self.request.unwrap_with(&DEFAULT_REQUEST_LIMITS);
        let endpoint_configuration = self.generate_metrics_endpoint_configuration()?;
        let service = ServiceBuilder::new()
            .settings(request_limits, DatadogMetricsRetryLogic)
            .service(DatadogMetricsService::new(
                client,
                self.default_api_key.inner(),
            ));

        let request_builder = DatadogMetricsRequestBuilder::new(
            endpoint_configuration,
            self.default_namespace.clone(),
        )?;

        let protocol = self.get_protocol();
        let sink = DatadogMetricsSink::new(service, request_builder, batcher_settings, protocol);

        Ok(VectorSink::from_event_streamsink(sink))
    }

    fn get_protocol(&self) -> String {
        self.get_base_agent_endpoint()
            .parse::<Uri>()
            .unwrap()
            .scheme_str()
            .unwrap_or("http")
            .to_string()
    }
}

fn build_uri(host: &str, endpoint: &str) -> crate::Result<Uri> {
    let result = format!("{}{}", host, endpoint)
        .parse::<Uri>()
        .context(UriParseSnafu)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<DatadogMetricsConfig>();
    }
}
