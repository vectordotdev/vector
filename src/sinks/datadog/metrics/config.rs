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
    config::{AcknowledgementsConfig, Input, SinkConfig, SinkContext, ValidatedSink},
    http::HttpClient,
    sinks::{
        Healthcheck, VectorSink,
        datadog::{DatadogCommonConfig, LocalDatadogCommonConfig},
        util::{
            HttpEndpoint, ServiceBuilderExt, SinkBatchSettings, TowerRequestConfig,
            batch::BatchConfig,
        },
    },
    tls::{MaybeTlsSettings, TlsEnableableConfig},
};
#[derive(Clone, Copy, Debug, Default)]
pub struct DatadogMetricsDefaultBatchSettings;

impl SinkBatchSettings for DatadogMetricsDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(100_000);
    // No default byte cap here; the appropriate limit (series: 5 MiB, sketches: 60 MiB) is
    // applied during validation based on the endpoint.
    const MAX_BYTES: Option<usize> = None;
    const TIMEOUT_SECS: f64 = 2.0;
}

pub(super) const SERIES_V2_PATH: &str = "/api/v2/series";
pub(super) const SERIES_V3_PATH: &str = "/api/intake/metrics/v3/series";
pub(super) const SKETCHES_PATH: &str = "/api/beta/sketches";

/// The API version to use when submitting series metrics to Datadog.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SeriesApiVersion {
    /// Use the v2 series endpoint (`/api/v2/series`).
    V2,

    /// Use the v3 series endpoint (`/api/intake/metrics/v3/series`).
    ///
    /// Columnar protobuf format with dictionary-based string deduplication and delta
    /// encoding. More efficient than v2 for workloads with many metrics that share
    /// common tags or names.
    ///
    /// This is the recommended and default endpoint.
    #[default]
    V3,
}

impl SeriesApiVersion {
    pub const fn get_path(self) -> &'static str {
        match self {
            Self::V2 => SERIES_V2_PATH,
            Self::V3 => SERIES_V3_PATH,
        }
    }

    /// Returns true if this version uses the V3 columnar encoding format.
    pub const fn is_v3_format(self) -> bool {
        matches!(self, Self::V3)
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
    pub(super) const fn payload_limits(self) -> DatadogMetricsPayloadLimits {
        // from https://docs.datadoghq.com/api/latest/metrics/#submit-metrics
        let (uncompressed, compressed) = match self {
            DatadogMetricsEndpoint::Sketches => (
                62_914_560, // 60 MiB
                3_200_000,  // 3.2 MB
            ),
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V2 | SeriesApiVersion::V3) => (
                5_242_880, // 5 MiB
                512_000,   // 512 KB
            ),
        };

        DatadogMetricsPayloadLimits {
            uncompressed,
            compressed,
        }
    }
}

/// Maps Datadog metric endpoints to their actual URI.
pub struct DatadogMetricsEndpointConfiguration {
    series_endpoint: HttpEndpoint,
    sketches_endpoint: HttpEndpoint,
}

impl DatadogMetricsEndpointConfiguration {
    /// Creates a new `DatadogMEtricsEndpointConfiguration`.
    pub const fn new(series_endpoint: HttpEndpoint, sketches_endpoint: HttpEndpoint) -> Self {
        Self {
            series_endpoint,
            sketches_endpoint,
        }
    }

    /// Gets the URI for the given Datadog metrics endpoint.
    pub fn get_uri_for_endpoint(&self, endpoint: DatadogMetricsEndpoint) -> HttpEndpoint {
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
    /// Defaults to `v3` (`/api/intake/metrics/v3/series`). Set to `v2` (`/api/v2/series`)
    /// if you need to use the v2 endpoint.
    #[serde(default)]
    pub series_api_version: SeriesApiVersion,

    #[serde(default)]
    pub batch: BatchConfig<DatadogMetricsDefaultBatchSettings>,

    #[serde(default)]
    pub request: TowerRequestConfig,
}

impl_generate_config_from_default!(DatadogMetricsConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "datadog_metrics")]
impl SinkConfig for DatadogMetricsConfig {
    fn input(&self) -> Input {
        Input::metric()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.local_dd_common.acknowledgements
    }
}

#[derive(Clone, Debug)]
pub struct ValidatedMetrics {
    batcher_settings: BatcherSettings,
    sketches_batcher_settings: BatcherSettings,
}

#[async_trait::async_trait]
impl ValidatedSink for DatadogMetricsConfig {
    type Validated = ValidatedMetrics;

    fn validate(&self) -> crate::Result<ValidatedMetrics> {
        let (batcher_settings, sketches_batcher_settings) =
            resolve_endpoint_batch_settings(self.batch, self.series_api_version)?;

        let site = self
            .local_dd_common
            .site
            .clone()
            .unwrap_or_else(|| datadog::DD_US_SITE.to_owned());
        let base = Self::metrics_base_endpoint(self.local_dd_common.endpoint.as_deref(), &site);
        HttpEndpoint::parse(&base)?;

        Ok(ValidatedMetrics {
            batcher_settings,
            sketches_batcher_settings,
        })
    }

    async fn build(
        &self,
        validated: &ValidatedMetrics,
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let client = self.build_client(&cx.proxy)?;
        let global = cx.extra_context.get_or_default::<datadog::Options>();
        let dd_common = self.local_dd_common.with_globals(global)?;
        let healthcheck = dd_common.build_healthcheck(client.clone())?;
        let sink = self.build_sink(
            &dd_common,
            client,
            validated.batcher_settings,
            validated.sketches_batcher_settings,
        )?;

        Ok((sink, healthcheck))
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
    fn metrics_base_endpoint(endpoint: Option<&str>, site: &str) -> String {
        endpoint.map_or_else(
            || {
                let version = str::replace(crate::built_info::PKG_VERSION, ".", "-");
                format!("https://{version}-vector.agent.{site}")
            },
            std::string::ToString::to_string,
        )
    }

    /// Generates the `DatadogMetricsEndpointConfiguration`, used for mapping endpoints to their URI.
    fn generate_metrics_endpoint_configuration(
        &self,
        dd_common: &DatadogCommonConfig,
    ) -> crate::Result<DatadogMetricsEndpointConfiguration> {
        let base_uri = Self::metrics_base_endpoint(dd_common.endpoint.as_deref(), &dd_common.site);

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
            Some(if let Some(config) = self.local_dd_common.tls.as_ref() {
                config
            } else {
                default_tls_config = TlsEnableableConfig::enabled();
                &default_tls_config
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
        batcher_settings: BatcherSettings,
        sketches_batcher_settings: BatcherSettings,
    ) -> crate::Result<VectorSink> {
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

        let protocol = HttpEndpoint::parse(&Self::metrics_base_endpoint(
            dd_common.endpoint.as_deref(),
            &dd_common.site,
        ))?
        .protocol()
        .to_string();
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
}

/// Returns `(series_settings, sketches_settings)`.
///
/// When the user has not set an explicit `max_bytes`, each endpoint is capped to its own
/// uncompressed payload limit (5 MiB for series, 60 MiB for sketches). When an explicit
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

fn build_uri(host: &str, endpoint: &str) -> crate::Result<HttpEndpoint> {
    Ok(HttpEndpoint::parse(host)?.append_path(endpoint)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ValidatedSink;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<DatadogMetricsConfig>();
    }

    #[test]
    fn validate_produces_endpoint_specific_batch_settings() {
        let config = DatadogMetricsConfig::default();
        let validated = config.validate().expect("validation should succeed");
        assert_eq!(validated.batcher_settings.size_limit, 5_242_880); // 5 MiB — Series v3 limit
        assert_eq!(validated.sketches_batcher_settings.size_limit, 62_914_560); // 60 MiB — Sketches limit
    }

    #[test]
    fn validate_rejects_malformed_endpoint() {
        let config = DatadogMetricsConfig {
            local_dd_common: LocalDatadogCommonConfig::new(
                Some("not a uri".to_string()),
                None,
                None,
            ),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_rejects_non_http_scheme() {
        let config = DatadogMetricsConfig {
            local_dd_common: LocalDatadogCommonConfig::new(
                Some("ftp://localhost:8080".to_string()),
                None,
                None,
            ),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    // When max_bytes is unset, each endpoint gets its own API payload limit.
    #[test]
    fn default_batch_config_uses_endpoint_specific_size_limits() {
        let (series, sketches) =
            resolve_endpoint_batch_settings(BatchConfig::default(), SeriesApiVersion::V2).unwrap();

        assert_eq!(series.size_limit, 5_242_880); // 5 MiB — Series v2 limit
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

    #[test]
    fn series_api_version_v2_v3_and_default_are_configurable() {
        for (yaml, expected) in [
            ("default_api_key: unused", SeriesApiVersion::V3),
            (
                "default_api_key: unused\nseries_api_version: v2",
                SeriesApiVersion::V2,
            ),
            (
                "default_api_key: unused\nseries_api_version: v3",
                SeriesApiVersion::V3,
            ),
        ] {
            let config = serde_yaml::from_str::<DatadogMetricsConfig>(yaml)
                .expect("v2, v3, and the unset default must all parse");
            assert_eq!(config.series_api_version, expected);
        }
    }

    #[test]
    fn series_api_version_v1_is_rejected() {
        let error = serde_yaml::from_str::<DatadogMetricsConfig>(
            "default_api_key: unused\nseries_api_version: v1",
        )
        .expect_err("the removed v1 option must fail configuration parsing");

        assert!(error.to_string().contains("unknown variant `v1`"));
        assert!(error.to_string().contains("expected `v2` or `v3`"));
    }

    // Each configurable series version must resolve to its own intake path, and only `v3` uses
    // the columnar wire format.
    #[test]
    fn series_api_version_paths_and_formats() {
        assert_eq!(SeriesApiVersion::V2.get_path(), SERIES_V2_PATH);
        assert_eq!(SeriesApiVersion::V3.get_path(), SERIES_V3_PATH);

        assert!(!SeriesApiVersion::V2.is_v3_format());
        assert!(SeriesApiVersion::V3.is_v3_format());
    }
}
