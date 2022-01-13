use std::num::NonZeroU64;

use futures::FutureExt;
use http::{uri::InvalidUri, Uri};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use tower::ServiceBuilder;
use vector_core::config::proxy::ProxyConfig;

use super::{
    request_builder::DatadogMetricsRequestBuilder,
    service::{DatadogMetricsRetryLogic, DatadogMetricsService},
    sink::DatadogMetricsSink,
};
use crate::{
    config::{DataType, SinkConfig, SinkContext},
    http::HttpClient,
    sinks::{
        datadog::{get_api_validate_endpoint, get_base_domain, healthcheck, Region},
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
    const TIMEOUT_SECS: NonZeroU64 = unsafe { NonZeroU64::new_unchecked(2) };
}

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Invalid host {:?}: {:?}", host, source))]
    InvalidHost { host: String, source: InvalidUri },
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

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct DatadogMetricsConfig {
    pub default_namespace: Option<String>,
    pub endpoint: Option<String>,
    // Deprecated, replaced by the site option
    pub region: Option<Region>,
    pub site: Option<String>,
    // Deprecated name
    #[serde(alias = "api_key")]
    default_api_key: String,
    #[serde(default)]
    pub batch: BatchConfig<DatadogMetricsDefaultBatchSettings>,
    #[serde(default)]
    pub request: TowerRequestConfig,
}

impl_generate_config_from_default!(DatadogMetricsConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "datadog_metrics")]
impl SinkConfig for DatadogMetricsConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let client = self.build_client(&cx.proxy)?;
        let healthcheck = self.build_healthcheck(client.clone())?;
        let sink = self.build_sink(client, cx)?;

        Ok((sink, healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Metric
    }

    fn sink_type(&self) -> &'static str {
        "datadog_metrics"
    }
}

impl DatadogMetricsConfig {
    /// Creates a default [`DatadogMetricsConfig`] with the given API key.
    pub fn from_api_key<T: Into<String>>(api_key: T) -> Self {
        Self {
            default_api_key: api_key.into(),
            ..Self::default()
        }
    }

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
        let client = HttpClient::new(None, proxy)?;
        Ok(client)
    }

    fn build_healthcheck(&self, client: HttpClient) -> crate::Result<Healthcheck> {
        let validate_endpoint =
            get_api_validate_endpoint(self.endpoint.as_ref(), self.site.as_ref(), self.region)?;
        Ok(healthcheck(client, validate_endpoint, self.default_api_key.clone()).boxed())
    }

    fn build_sink(&self, client: HttpClient, cx: SinkContext) -> crate::Result<VectorSink> {
        let batcher_settings = self.batch.into_batcher_settings()?;

        let request_limits = self.request.unwrap_with(&DEFAULT_REQUEST_LIMITS);
        let endpoint_configuration = self.generate_metrics_endpoint_configuration()?;
        let service = ServiceBuilder::new()
            .settings(request_limits, DatadogMetricsRetryLogic)
            .service(DatadogMetricsService::new(
                client,
                self.default_api_key.as_str(),
            ));

        let request_builder = DatadogMetricsRequestBuilder::new(
            endpoint_configuration,
            self.default_namespace.clone(),
        )?;

        let sink = DatadogMetricsSink::new(cx, service, request_builder, batcher_settings);

        Ok(VectorSink::from_event_streamsink(sink))
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
