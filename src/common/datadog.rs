//! Functionality shared between Datadog sources and sinks.
// Allow unused imports here, since use of these functions will differ depending on the
// Datadog component type, whether it's used in integration tests, etc.
#![allow(dead_code)]
#![allow(unreachable_pub)]

use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use vector_lib::{
    event::DatadogMetricOriginMetadata, schema::meaning, sensitive_string::SensitiveString,
};

pub(crate) const DD_US_SITE: &str = "datadoghq.com";
pub(crate) const DD_EU_SITE: &str = "datadoghq.eu";

/// The datadog tags event path.
pub const DDTAGS: &str = "ddtags";

/// Mapping of the semantic meaning of well known Datadog reserved attributes
/// to the field name that Datadog intake expects.
// https://docs.datadoghq.com/logs/log_configuration/attributes_naming_convention/?s=severity#reserved-attributes
pub const DD_RESERVED_SEMANTIC_ATTRS: [(&str, &str); 6] = [
    (meaning::SEVERITY, "status"), // status is intentionally semantically defined as severity
    (meaning::TIMESTAMP, "timestamp"),
    (meaning::HOST, "hostname"),
    (meaning::SERVICE, "service"),
    (meaning::SOURCE, "ddsource"),
    (meaning::TAGS, DDTAGS),
];

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
pub(crate) fn get_api_base_endpoint(endpoint: Option<&str>, site: &str) -> String {
    endpoint.map_or_else(|| format!("https://api.{}", site), compute_api_endpoint)
}

/// Computes the Datadog API endpoint from a given endpoint string.
///
/// This scans the given endpoint for the common Datadog domain names; and, if found, rewrites the
/// endpoint string using the standard API URI. If not found, the endpoint is used as-is.
fn compute_api_endpoint(endpoint: &str) -> String {
    // This mechanism is derived from the forwarder health check in the Datadog Agent:
    // https://github.com/DataDog/datadog-agent/blob/cdcf0fc809b9ac1cd6e08057b4971c7dbb8dbe30/comp/forwarder/defaultforwarder/forwarder_health.go#L45-L47
    // https://github.com/DataDog/datadog-agent/blob/cdcf0fc809b9ac1cd6e08057b4971c7dbb8dbe30/comp/forwarder/defaultforwarder/forwarder_health.go#L188-L190
    static DOMAIN_REGEX: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?:[a-z]{2}\d\.)?(datadoghq\.[a-z]+|ddog-gov\.com)/*$")
            .expect("Could not build Datadog domain regex")
    });

    if let Some(caps) = DOMAIN_REGEX.captures(endpoint) {
        format!("https://api.{}", &caps[1])
    } else {
        endpoint.into()
    }
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

#[cfg(test)]
mod tests {
    use similar_asserts::assert_eq;

    use super::*;

    #[test]
    fn computes_correct_api_endpoint() {
        assert_eq!(
            compute_api_endpoint("https://http-intake.logs.datadoghq.com"),
            "https://api.datadoghq.com"
        );
        assert_eq!(
            compute_api_endpoint("https://http-intake.logs.datadoghq.com/"),
            "https://api.datadoghq.com"
        );
        assert_eq!(
            compute_api_endpoint("http://http-intake.logs.datadoghq.com/"),
            "https://api.datadoghq.com"
        );
        assert_eq!(
            compute_api_endpoint("https://anythingelse.datadoghq.com/"),
            "https://api.datadoghq.com"
        );
        assert_eq!(
            compute_api_endpoint("https://this.datadoghq.eu/"),
            "https://api.datadoghq.eu"
        );
        assert_eq!(
            compute_api_endpoint("http://datadog.com/"),
            "http://datadog.com/"
        );
    }

    #[test]
    fn gets_correct_api_base_endpoint() {
        assert_eq!(
            get_api_base_endpoint(None, DD_US_SITE),
            "https://api.datadoghq.com"
        );
        assert_eq!(
            get_api_base_endpoint(None, "datadog.net"),
            "https://api.datadog.net"
        );
        assert_eq!(
            get_api_base_endpoint(Some("https://logs.datadoghq.eu"), DD_US_SITE),
            "https://api.datadoghq.eu"
        );
    }
}
