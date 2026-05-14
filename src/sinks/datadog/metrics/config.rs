use http::Uri;
use snafu::ResultExt;
use tower::ServiceBuilder;
use vector_lib::{
    config::proxy::ProxyConfig, configurable::configurable_component, stream::BatcherSettings,
};

use super::{
    request_builder::DatadogMetricsRequestBuilder,
    service::{DatadogMetricsRetryLogic, DatadogMetricsService},
    sink::DatadogMetricsSink,
};
use crate::{
    common::datadog,
    config::{AcknowledgementsConfig, Input, SinkConfig, SinkContext},
    http::HttpClient,
    sinks::{
        Healthcheck, UriParseSnafu, VectorSink,
        datadog::{DatadogCommonConfig, LocalDatadogCommonConfig},
        util::{ServiceBuilderExt, SinkBatchSettings, TowerRequestConfig, batch::BatchConfig},
    },
    tls::{MaybeTlsSettings, TlsEnableableConfig},
};
#[derive(Clone, Copy, Debug, Default)]
pub struct DatadogMetricsDefaultBatchSettings;

impl SinkBatchSettings for DatadogMetricsDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(100_000);
    // No default byte cap here; the appropriate limit (v1: 60 MiB, v2: 5 MiB) is applied at
    // sink build time based on the active series API version.
    const MAX_BYTES: Option<usize> = None;
    const TIMEOUT_SECS: f64 = 2.0;
}

pub(super) const SERIES_V1_PATH: &str = "/api/v1/series";
pub(super) const SERIES_V2_PATH: &str = "/api/v2/series";
pub(super) const SKETCHES_PATH: &str = "/api/beta/sketches";

/// The API version to use when submitting series metrics to Datadog.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SeriesApiVersion {
    /// Use the v1 series endpoint (`/api/v1/series`).
    ///
    /// This is a legacy endpoint. Prefer `v2` unless you have a specific reason to use v1.
    #[configurable(deprecated)]
    V1,

    /// Use the v2 series endpoint (`/api/v2/series`).
    ///
    /// This is the recommended and default endpoint.
    #[default]
    V2,
}

impl SeriesApiVersion {
    pub const fn get_path(self) -> &'static str {
        match self {
            Self::V1 => SERIES_V1_PATH,
            Self::V2 => SERIES_V2_PATH,
        }
    }
}

/// Various metric type-specific API types.
///
/// Each of these corresponds to a specific request path when making a request to the agent API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DatadogMetricsEndpoint {
    Series(SeriesApiVersion),
    Sketches,
}

/// Payload limits for metrics are endpoint-dependent.
pub(super) struct DatadogMetricsPayloadLimits {
    pub(super) uncompressed: usize,
    pub(super) compressed: usize,
}

impl DatadogMetricsEndpoint {
    /// Gets the content type associated with the specific encoder for a given metric endpoint.
    pub const fn content_type(self) -> &'static str {
        match self {
            Self::Series(SeriesApiVersion::V1) => "application/json",
            Self::Sketches | Self::Series(SeriesApiVersion::V2) => "application/x-protobuf",
        }
    }

    pub(super) const fn payload_limits(self) -> DatadogMetricsPayloadLimits {
        // from https://docs.datadoghq.com/api/latest/metrics/#submit-metrics
        let (uncompressed, compressed) = match self {
            // Sketches use the same payload size limits as v1 series
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V1)
            | DatadogMetricsEndpoint::Sketches => (
                62_914_560, // 60 MiB
                3_200_000,  // 3.2 MB
            ),
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V2) => (
                5_242_880, // 5 MiB
                512_000,   // 512 KB
            ),
        };

        DatadogMetricsPayloadLimits {
            uncompressed,
            compressed,
        }
    }

    /// Returns the compression scheme used for this endpoint.
    pub(super) const fn compression(self) -> DatadogMetricsCompression {
        match self {
            Self::Series(SeriesApiVersion::V1) => DatadogMetricsCompression::Zlib,
            _ => DatadogMetricsCompression::Zstd,
        }
    }
}

/// Selects the compressor for a given Datadog metrics endpoint.
#[derive(Clone, Copy, Debug)]
pub(super) enum DatadogMetricsCompression {
    /// zlib (deflate) — used by Series v1.
    Zlib,
    /// zstd — used by Series v2 and Sketches.
    Zstd,
}

impl DatadogMetricsCompression {
    pub(super) const fn content_encoding(self) -> &'static str {
        match self {
            Self::Zstd => "zstd",
            Self::Zlib => "deflate",
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
            DatadogMetricsEndpoint::Series { .. } => self.series_endpoint.clone(),
            DatadogMetricsEndpoint::Sketches => self.sketches_endpoint.clone(),
        }
    }
}

/// Configuration for the `datadog_metrics` sink.
#[configurable_component(sink("datadog_metrics", "Publish metric events to Datadog."))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct DatadogMetricsConfig {
    #[serde(flatten)]
    pub local_dd_common: LocalDatadogCommonConfig,

    /// Sets the default namespace for any metrics sent.
    ///
    /// This namespace is only used if a metric has no existing namespace. When a namespace is
    /// present, it is used as a prefix to the metric name, and separated with a period (`.`).
    #[configurable(metadata(docs::examples = "myservice"))]
    #[serde(default)]
    pub default_namespace: Option<String>,

    /// Controls which Datadog series API endpoint is used to submit metrics.
    ///
    /// Defaults to `v2` (`/api/v2/series`). Set to `v1` (`/api/v1/series`) only if you need to
    /// fall back to the legacy endpoint.
    #[serde(default)]
    pub series_api_version: SeriesApiVersion,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<DatadogMetricsDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,
}

impl_generate_config_from_default!(DatadogMetricsConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "datadog_metrics")]
impl SinkConfig for DatadogMetricsConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let client = self.build_client(&cx.proxy)?;
        let global = cx.extra_context.get_or_default::<datadog::Options>();
        let dd_common = self.local_dd_common.with_globals(global)?;
        let healthcheck = dd_common.build_healthcheck(client.clone())?;
        let sink = self.build_sink(&dd_common, client)?;

        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        Input::metric()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.local_dd_common.acknowledgements
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
    fn get_base_agent_endpoint(&self, dd_common: &DatadogCommonConfig) -> String {
        dd_common.endpoint.clone().unwrap_or_else(|| {
            let version = str::replace(crate::built_info::PKG_VERSION, ".", "-");
            format!(
                "https://{}-vector.agent.{}",
                version,
                dd_common.site.as_str()
            )
        })
    }

    /// Generates the `DatadogMetricsEndpointConfiguration`, used for mapping endpoints to their URI.
    fn generate_metrics_endpoint_configuration(
        &self,
        dd_common: &DatadogCommonConfig,
    ) -> crate::Result<DatadogMetricsEndpointConfiguration> {
        let base_uri = self.get_base_agent_endpoint(dd_common);

        let series_endpoint = build_uri(&base_uri, self.series_api_version.get_path())?;
        let sketches_endpoint = build_uri(&base_uri, SKETCHES_PATH)?;

        Ok(DatadogMetricsEndpointConfiguration::new(
            series_endpoint,
            sketches_endpoint,
        ))
    }

    fn build_client(&self, proxy: &ProxyConfig) -> crate::Result<HttpClient> {
        let default_tls_config;

        let tls_settings = MaybeTlsSettings::from_config(
            Some(match self.local_dd_common.tls.as_ref() {
                Some(config) => config,
                None => {
                    default_tls_config = TlsEnableableConfig::enabled();
                    &default_tls_config
                }
            }),
            false,
        )?;
        let client = HttpClient::new(tls_settings, proxy)?;
        Ok(client)
    }

    fn build_sink(
        &self,
        dd_common: &DatadogCommonConfig,
        client: HttpClient,
    ) -> crate::Result<VectorSink> {
        let (batcher_settings, sketches_batcher_settings) =
            resolve_endpoint_batch_settings(self.batch, self.series_api_version)?;

        // TODO: revisit our concurrency and batching defaults
        let request_limits = self.request.into_settings();

        let endpoint_configuration = self.generate_metrics_endpoint_configuration(dd_common)?;
        let service = ServiceBuilder::new()
            .settings(request_limits, DatadogMetricsRetryLogic)
            .service(DatadogMetricsService::new(
                client,
                dd_common.default_api_key.inner(),
            ));

        let request_builder = DatadogMetricsRequestBuilder::new(
            endpoint_configuration,
            self.default_namespace.clone(),
            self.series_api_version,
        );

        let protocol = self.get_protocol(dd_common);
        let sink = DatadogMetricsSink::new(
            service,
            request_builder,
            batcher_settings,
            sketches_batcher_settings,
            protocol,
            self.series_api_version,
        );

        Ok(VectorSink::from_event_streamsink(sink))
    }

    fn get_protocol(&self, dd_common: &DatadogCommonConfig) -> String {
        self.get_base_agent_endpoint(dd_common)
            .parse::<Uri>()
            .unwrap()
            .scheme_str()
            .unwrap_or("http")
            .to_string()
    }
}

/// Returns `(series_settings, sketches_settings)`.
///
/// When the user has not set an explicit `max_bytes`, each endpoint is capped to its own
/// uncompressed payload limit (5 MiB for Series v2, 60 MiB for Sketches). When an explicit
/// limit is configured, both endpoints share it.
fn resolve_endpoint_batch_settings(
    batch: BatchConfig<DatadogMetricsDefaultBatchSettings>,
    series_version: SeriesApiVersion,
) -> crate::Result<(BatcherSettings, BatcherSettings)> {
    let mut series = batch.into_batcher_settings()?;
    let mut sketches = series;
    if series.size_limit == usize::MAX {
        series.size_limit = DatadogMetricsEndpoint::Series(series_version)
            .payload_limits()
            .uncompressed;
        sketches.size_limit = DatadogMetricsEndpoint::Sketches
            .payload_limits()
            .uncompressed;
    }
    Ok((series, sketches))
}

fn build_uri(host: &str, endpoint: &str) -> crate::Result<Uri> {
    let result = format!("{host}{endpoint}")
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

    // When max_bytes is unset, each endpoint gets its own API payload limit.
    #[test]
    fn default_batch_config_uses_endpoint_specific_size_limits() {
        let (series, sketches) =
            resolve_endpoint_batch_settings(BatchConfig::default(), SeriesApiVersion::V2).unwrap();

        assert_eq!(series.size_limit, 5_242_880); // 5 MiB — Series v2 limit
        assert_eq!(sketches.size_limit, 62_914_560); // 60 MiB — Sketches limit
    }

    #[test]
    fn v1_batch_config_uses_v1_size_limit() {
        let (series, sketches) =
            resolve_endpoint_batch_settings(BatchConfig::default(), SeriesApiVersion::V1).unwrap();

        assert_eq!(series.size_limit, 62_914_560); // 60 MiB — Series v1 limit
        assert_eq!(sketches.size_limit, 62_914_560); // 60 MiB — Sketches limit
    }

    // When the user sets max_bytes, both endpoints share that limit unchanged.
    #[test]
    fn explicit_max_bytes_applies_to_both_endpoints() {
        let mut config = BatchConfig::<DatadogMetricsDefaultBatchSettings>::default();
        config.max_bytes = Some(1_000_000);

        let (series, sketches) =
            resolve_endpoint_batch_settings(config, SeriesApiVersion::V2).unwrap();

        assert_eq!(series.size_limit, 1_000_000);
        assert_eq!(sketches.size_limit, 1_000_000);
    }
}
