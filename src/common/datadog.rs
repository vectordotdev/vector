//! Functionality shared between Datadog sources and sinks.
// Allow unused imports here, since use of these functions will differ depending on the
// Datadog component type, whether it's used in integration tests, etc.
#![allow(dead_code)]
#![allow(unreachable_pub)]
use serde::{Deserialize, Serialize};
use vector_lib::{event::DatadogMetricOriginMetadata, sensitive_string::SensitiveString};

pub(crate) const DD_US_SITE: &str = "datadoghq.com";
pub(crate) const DD_EU_SITE: &str = "datadoghq.eu";

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct DatadogSeriesMetric {
    pub(crate) metric: String,
    pub(crate) r#type: DatadogMetricType,
    pub(crate) interval: Option<u32>,
    pub(crate) points: Vec<DatadogPoint<f64>>,
    pub(crate) tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) source_type_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) device: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) metadata: Option<DatadogSeriesMetricMetadata>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct DatadogSeriesMetricMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) origin: Option<DatadogMetricOriginMetadata>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DatadogMetricType {
    Gauge,
    Count,
    Rate,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct DatadogPoint<T>(pub(crate) i64, pub(crate) T);

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
