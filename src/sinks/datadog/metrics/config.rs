use crate::{
    config::{DataType, SinkConfig, SinkContext},
    http::HttpClient,
    sinks::{
        datadog::{healthcheck, Region},
        util::{
            batch::{BatchConfig, BatchSettings},
            retries::RetryLogic,
            Compression, Concurrency, ServiceBuilderExt, TowerRequestConfig,
        },
        Healthcheck, UriParseError, VectorSink,
    },
};
use futures::FutureExt;
use http::{uri::InvalidUri, Uri};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use tower::ServiceBuilder;
use vector_core::config::proxy::ProxyConfig;

use super::{
    service::{DatadogMetricsRetryLogic, DatadogMetricsService},
    sink::DatadogMetricsSink,
};

// TODO: revisit our concurrency and batching defaults
const DEFAULT_REQUEST_LIMITS: TowerRequestConfig =
    TowerRequestConfig::const_new(Concurrency::None, Concurrency::None).retry_attempts(5);

const DEFAULT_BATCH_SETTINGS: BatchSettings<()> =
    BatchSettings::const_default().events(20).timeout(1);

const MAXIMUM_SERIES_PAYLOAD_COMPRESSED_SIZE: usize = 3_200_000;
const MAXIMUM_SERIES_PAYLOAD_SIZE: usize = 62_914_560;

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
    Distribution,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct DatadogMetricsConfig {
    #[serde(alias = "namespace")]
    pub default_namespace: Option<String>,
    // Deprecated name
    #[serde(alias = "host")]
    pub endpoint: Option<String>,
    // Deprecated, replaced by the site option
    pub region: Option<Region>,
    pub site: Option<String>,
    pub api_key: String,
    #[serde(default)]
    pub batch: BatchConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,
    #[serde(default = "Compression::gzip_default")]
    pub compression: Compression,
}

impl_generate_config_from_default!(DatadogMetricsConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "datadog_metrics")]
impl SinkConfig for DatadogMetricsConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let client = HttpClient::new(None, cx.proxy())?;

        let client = self.build_client(&cx.proxy)?;
        let healthcheck = self.build_healthcheck(client.clone());
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
            api_key: api_key.into(),
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
            format!("https://{}-vector.agent.{}", version, self.get_site())
        })
    }

    /// Generates the full URIs to use for the various type-specific metrics endpoints.
    fn generate_metric_endpoints(&self) -> crate::Result<Vec<(DatadogMetricsEndpoint, Uri)>> {
        let base_uri = self.get_base_agent_endpoint();
        let series_endpoint = build_uri(&base_uri, "/api/v1/series")?;
        let distribution_endpoint = build_uri(&base_uri, "/api/v1/distribution_points")?;

        Ok(vec![
            (DatadogMetricsEndpoint::Series, series_endpoint),
            (DatadogMetricsEndpoint::Distribution, distribution_endpoint),
        ])
    }

    /// Gets the base URI of the Datadog API.
    ///
    /// The `endpoint` configuration field will be used here if it is present.
    fn get_api_endpoint(&self) -> String {
        self.endpoint
            .clone()
            .unwrap_or_else(|| format!("https://api.{}", self.get_site()))
    }

    /// Gets the base domain to use for any calls to Datadog.
    ///
    /// If `site` is not specified, we fallback to `region`, and if that is not specified, we
    /// fallback to the Datadog US domain.
    fn get_site(&self) -> &str {
        self.site.as_deref().unwrap_or_else(|| match self.region {
            Some(Region::Eu) => "datadoghq.eu",
            None | Some(Region::Us) => "datadoghq.com",
        })
    }

    fn build_client(&self, proxy: &ProxyConfig) -> crate::Result<HttpClient> {
        let client = HttpClient::new(None, proxy)?;
        Ok(client)
    }

    fn build_healthcheck(&self, client: HttpClient) -> Healthcheck {
        healthcheck(self.get_api_endpoint(), self.api_key.clone(), client).boxed()
    }

    fn build_sink(&self, client: HttpClient, cx: SinkContext) -> crate::Result<VectorSink> {
        let batch = DEFAULT_BATCH_SETTINGS.parse_config(self.batch)?;

        let request_limits = self.request.unwrap_with(&DEFAULT_REQUEST_LIMITS);
        let metric_endpoints = self.generate_metric_endpoints()?;
        let service = ServiceBuilder::new()
            .settings(request_limits, DatadogMetricsRetryLogic)
            .service(DatadogMetricsService::new(client));

        let sink = DatadogMetricsSink::new(cx, service, metric_endpoints, self.compression);

        Ok(VectorSink::Stream(Box::new(sink)))
    }
}

fn build_uri(host: &str, endpoint: &str) -> crate::Result<Uri> {
    let result = format!("{}{}", host, endpoint)
        .parse::<Uri>()
        .context(UriParseError)?;
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
