// Allow unused imports here, since use of these functions will differ depending on the
// Datadog component type, whether it's used in integration tests, etc.
#![allow(dead_code)]
#![allow(unreachable_pub)]
use serde::{Deserialize, Serialize};
use vector_config::configurable_component;

pub const DD_US_SITE: &str = "datadoghq.com";
pub const DD_EU_SITE: &str = "datadoghq.eu";

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

/// A Datadog region.
#[configurable_component]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Region {
    /// US region.
    Us,

    /// EU region.
    Eu,
}

/// Gets the base domain to use for any calls to Datadog.
///
/// This is a helper function for Datadog component configs using the deprecated `region` field.
///
/// If `region` is not specified, we fallback to `site`.
///
/// TODO: This should be deleted when the deprecated `region` config option is fully removed,
///       and the callers will replace the result of this function call with just `site`.
pub(crate) const fn get_base_domain_region(site: &str, region: Option<Region>) -> &str {
    if let Some(region) = region {
        match region {
            Region::Eu => DD_EU_SITE,
            Region::Us => DD_US_SITE,
        }
    } else {
        site
    }
}

/// Gets the base API endpoint to use for any calls to Datadog.
///
/// If `endpoint` is not specified, we fallback to `site`.
pub(crate) fn get_api_base_endpoint(endpoint: Option<&String>, site: &str) -> String {
    endpoint
        .cloned()
        .unwrap_or_else(|| format!("https://api.{}", site))
}
