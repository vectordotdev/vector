use crate::event::{Metric, MetricValue};
use async_graphql::Object;
use chrono::{DateTime, Utc};

pub struct ProcessedBytesTotal(Metric);

impl ProcessedBytesTotal {
    pub fn new(m: Metric) -> Self {
        Self(m)
    }
}

#[Object]
impl ProcessedBytesTotal {
    /// Metric timestamp
    pub async fn timestamp(&self) -> Option<DateTime<Utc>> {
        self.0.data.timestamp
    }

    /// Total number of bytes processed
    pub async fn processed_bytes_total(&self) -> f64 {
        match self.0.data.value {
            MetricValue::Counter { value } => value,
            _ => 0.00,
        }
    }
}

impl From<Metric> for ProcessedBytesTotal {
    fn from(m: Metric) -> Self {
        Self(m)
    }
}

pub struct ComponentProcessedBytesTotal {
    name: String,
    metric: Metric,
}

impl ComponentProcessedBytesTotal {
    /// Returns a new `ComponentProcessedBytesTotal` struct, which is a GraphQL type. The
    /// component name is hoisted for clear field resolution in the resulting payload
    pub fn new(metric: Metric) -> Self {
        let name = metric.tag_value("component_name").expect(
            "Returned a metric without a `component_name`, which shouldn't happen. Please report.",
        );

        Self { name, metric }
    }
}

#[Object]
impl ComponentProcessedBytesTotal {
    /// Component name
    async fn name(&self) -> &str {
        &self.name
    }

    /// Bytes processed total metric
    async fn metric(&self) -> ProcessedBytesTotal {
        ProcessedBytesTotal::new(self.metric.clone())
    }
}

pub struct ComponentProcessedBytesThroughput {
    name: String,
    throughput: i64,
}

impl ComponentProcessedBytesThroughput {
    /// Returns a new `ComponentProcessedBytesThroughput`, set to the provided name/throughput values
    pub fn new(name: String, throughput: i64) -> Self {
        Self { name, throughput }
    }
}

#[Object]
impl ComponentProcessedBytesThroughput {
    /// Component name
    async fn name(&self) -> &str {
        &self.name
    }

    /// Bytes processed throughput
    async fn throughput(&self) -> i64 {
        self.throughput
    }
}
