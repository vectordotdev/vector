// Allow unused imports here, since use of these functions will differ depending on the
// Datadog component type, whether it's used in integration tests, etc.
#![allow(dead_code)]
#![allow(unreachable_pub)]
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct DatadogSeriesMetric {
    pub(crate) metric: String,
    pub(crate) r#type: DatadogMetricType,
    pub(crate) interval: Option<i64>,
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

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Region {
    Us,
    Eu,
}

/// Gets the base domain to use for any calls to Datadog.
///
/// If `site` is not specified, we fallback to `region`, and if that is not specified, we
/// fallback to the Datadog US domain.
pub(crate) fn get_base_domain(site: Option<&String>, region: Option<Region>) -> &str {
    site.map(|s| s.as_str()).unwrap_or_else(|| match region {
        Some(Region::Eu) => "datadoghq.eu",
        None | Some(Region::Us) => "datadoghq.com",
    })
}

/// Gets the base API endpoint to use for any calls to Datadog.
///
/// If `site` is not specified, we fallback to `region`, and if that is not specified, we fallback
/// to the Datadog US domain.
pub(crate) fn get_api_base_endpoint(
    endpoint: Option<&String>,
    site: Option<&String>,
    region: Option<Region>,
) -> String {
    endpoint.cloned().unwrap_or_else(|| {
        let base = get_base_domain(site, region);
        format!("https://api.{}", base)
    })
}
