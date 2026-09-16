//! Functionality shared between Datadog sources and sinks.
// Allow unused imports here, since use of these functions will differ depending on the
// Datadog component type, whether it's used in integration tests, etc.
#![allow(dead_code)]
#![allow(unreachable_pub)]

use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use vector_lib::{
    event::DatadogMetricOriginMetadata, schema::meaning, sensitive_string::SensitiveString,
};

use crate::event::Value;

pub(crate) const DD_US_SITE: &str = "datadoghq.com";
pub(crate) const DD_EU_SITE: &str = "datadoghq.eu";

/// The datadog tags event path.
pub const DDTAGS: &str = "ddtags";
/// The datadog message event path.
pub const MESSAGE: &str = "message";
pub(crate) const DATADOG_METRIC_RESOURCE_TAG_PREFIX: &str = "resource.";

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

/// Returns true if the parameter `attr` is one of the reserved Datadog log attributes
pub fn is_reserved_attribute(attr: &str) -> bool {
    DD_RESERVED_SEMANTIC_ATTRS
        .iter()
        .any(|(_, attr_str)| &attr == attr_str)
}

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
    endpoint.map_or_else(|| format!("https://api.{site}"), compute_api_endpoint)
}

/// Computes the Datadog API endpoint from a given endpoint string.
///
/// This scans the given endpoint for the common Datadog domain names; and, if found, rewrites the
/// endpoint string using the standard API URI. If not found, the endpoint is used as-is.
fn compute_api_endpoint(endpoint: &str) -> String {
    // This mechanism is derived from the forwarder health check in the Datadog Agent:
    // https://github.com/DataDog/datadog-agent/blob/cdcf0fc809b9ac1cd6e08057b4971c7dbb8dbe30/comp/forwarder/defaultforwarder/forwarder_health.go#L45-L47
    // https://github.com/DataDog/datadog-agent/blob/cdcf0fc809b9ac1cd6e08057b4971c7dbb8dbe30/comp/forwarder/defaultforwarder/forwarder_health.go#L188-L190
    static DOMAIN_REGEX: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"((?:[a-z]{2}\d\.)?(?:datadoghq\.[a-z]+|ddog-gov\.com))/*$")
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

/// Encode a Datadog 64-bit identifier as 16 lowercase hexadecimal characters.
///
/// Vector's integer `Value` is `i64`, so casting a Datadog `uint64` ID would wrap any value
/// above `i64::MAX`. A fixed-width hex string preserves the full unsigned range.
pub(crate) fn encode_u64_id_hex(id: u64) -> String {
    format!("{id:016x}")
}

/// Decode a Datadog 64-bit identifier from an event value.
///
/// Hex strings are the source encoding. Integers are still accepted so a sink can reverse
/// the historical `u64 as i64` wrap for in-flight events.
pub(crate) fn decode_u64_id(value: &Value) -> u64 {
    match value {
        Value::Integer(v) => *v as u64,
        other => u64::from_str_radix(&other.to_string_lossy(), 16).unwrap_or(0),
    }
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
    fn preserves_site_prefix_in_api_endpoint() {
        for (prefix, tld) in [
            ("us3", "com"),
            ("us5", "com"),
            ("ap1", "com"),
            ("eu1", "eu"),
        ] {
            assert_eq!(
                compute_api_endpoint(&format!(
                    "https://http-intake.logs.{prefix}.datadoghq.{tld}"
                )),
                format!("https://api.{prefix}.datadoghq.{tld}")
            );
        }
        assert_eq!(
            compute_api_endpoint("https://1-2-3-observability-pipelines.agent.us3.datadoghq.com"),
            "https://api.us3.datadoghq.com"
        );
    }

    #[test]
    fn encode_u64_id_hex_is_fixed_width_lowercase() {
        assert_eq!(encode_u64_id_hex(0), "0000000000000000");
        assert_eq!(encode_u64_id_hex(999), "00000000000003e7");
        assert_eq!(encode_u64_id_hex(1u64 << 63), "8000000000000000");
        assert_eq!(encode_u64_id_hex(u64::MAX), "ffffffffffffffff");
    }

    #[test]
    fn decode_u64_id_preserves_unsigned_range() {
        assert_eq!(decode_u64_id(&Value::from("ffffffffffffffff")), u64::MAX);
        assert_eq!(decode_u64_id(&Value::from("8000000000000000")), 1u64 << 63);
        assert_eq!(decode_u64_id(&Value::from("00000000000003e7")), 999);
        assert_eq!(decode_u64_id(&Value::from(999i64)), 999);
        // Historical wrap: `u64 as i64` for the high bit, then back.
        assert_eq!(decode_u64_id(&Value::from(i64::MIN)), 1u64 << 63);
        assert_eq!(decode_u64_id(&Value::from("not-hex")), 0);
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
        assert_eq!(
            get_api_base_endpoint(
                Some("https://http-intake.logs.us3.datadoghq.com"),
                DD_US_SITE
            ),
            "https://api.us3.datadoghq.com"
        );
    }
}
