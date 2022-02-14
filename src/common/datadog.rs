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
