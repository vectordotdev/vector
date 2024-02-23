//! Functionality shared between Datadog sources and sinks.
// Allow unused imports here, since use of these functions will differ depending on the
// Datadog component type, whether it's used in integration tests, etc.
#![allow(dead_code)]
#![allow(unreachable_pub)]
use serde::{Deserialize, Serialize};
use vector_lib::{event::DatadogMetricOriginMetadata, sensitive_string::SensitiveString};

pub(crate) const DD_US_SITE: &str = "datadoghq.com";
pub(crate) const DD_EU_SITE: &str = "datadoghq.eu";

/// DatadogSeriesMetric
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct DatadogSeriesMetric {
    /// metric
    pub metric: String,
    /// metric type
    pub r#type: DatadogMetricType,
    /// interval
    pub interval: Option<u32>,
    /// points
    pub points: Vec<DatadogPoint<f64>>,
    /// tags
    pub tags: Option<Vec<String>>,
    /// host
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// source_type_name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_type_name: Option<String>,
    /// device
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    /// metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<DatadogSeriesMetricMetadata>,
}

/// Datadog series metric metadata
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct DatadogSeriesMetricMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) origin: Option<DatadogMetricOriginMetadata>,
}

/// Datadog Metric Type
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DatadogMetricType {
    /// Gauge
    Gauge,
    /// Count
    Count,
    /// Rate
    Rate,
}

/// Datadog Point
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct DatadogPoint<T>(pub i64, pub T);

/// Gets the base API endpoint to use for any calls to Datadog.
///
/// If `endpoint` is not specified, we fallback to `site`.
pub(crate) fn get_api_base_endpoint(endpoint: Option<&String>, site: &str) -> String {
    endpoint
        .cloned()
        .unwrap_or_else(|| format!("https://api.{}", site))
}

/// Default settings to use for Datadog components.
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
pub struct Options {
    /// Default Datadog API key to use for Datadog components.
    ///
    /// This can also be specified with the `DD_API_KEY` environment variable.
    #[derivative(Default(value = "default_api_key()"))]
    pub api_key: Option<SensitiveString>,

    /// Default site to use for Datadog components.
    ///
    /// This can also be specified with the `DD_SITE` environment variable.
    #[derivative(Default(value = "default_site()"))]
    pub site: String,
}

fn default_api_key() -> Option<SensitiveString> {
    std::env::var("DD_API_KEY").ok().map(Into::into)
}

pub(crate) fn default_site() -> String {
    std::env::var("DD_SITE").unwrap_or(DD_US_SITE.to_string())
}
