use crate::event::{Metric, MetricValue};
use async_graphql::Object;
use chrono::{DateTime, Utc};

pub struct BytesProcessedTotal(Metric);

impl BytesProcessedTotal {
    pub fn new(m: Metric) -> Self {
        Self(m)
    }
}

#[Object]
impl BytesProcessedTotal {
    /// Metric timestamp
    pub async fn timestamp(&self) -> Option<DateTime<Utc>> {
        self.0.timestamp
    }

    /// Total number of bytes processed
    pub async fn bytes_processed_total(&self) -> f64 {
        match self.0.value {
            MetricValue::Counter { value } => value,
            _ => 0.00,
        }
    }
}

impl From<Metric> for BytesProcessedTotal {
    fn from(m: Metric) -> Self {
        Self(m)
    }
}

pub struct ComponentBytesProcessedTotal {
    name: String,
    metric: Metric,
}

impl ComponentBytesProcessedTotal {
    /// Returns a new `ComponentBytesProcessedTotal` struct, which is a GraphQL type. The
    /// component name is hoisted for clear field resolution in the resulting payload
    pub fn new(metric: Metric) -> Self {
        let name = metric.tag_value("component_name").expect(
            "Returned a metric without a `component_name`, which shouldn't happen. Please report.",
        );

        Self { name, metric }
    }
}

#[Object]
impl ComponentBytesProcessedTotal {
    /// Component name
    async fn name(&self) -> &str {
        &self.name
    }

    /// Bytes processed total metric
    async fn metric(&self) -> BytesProcessedTotal {
        BytesProcessedTotal::new(self.metric.clone())
    }
}

pub struct ComponentBytesProcessedThroughput {
    name: String,
    throughput: i64,
}

impl ComponentBytesProcessedThroughput {
    /// Returns a new `ComponentBytesProcessedThroughput`, set to the provided name/throughput values
    pub fn new(name: String, throughput: i64) -> Self {
        Self { name, throughput }
    }
}

#[Object]
impl ComponentBytesProcessedThroughput {
    /// Component name
    async fn name(&self) -> &str {
        &self.name
    }

    /// Bytes processed throughput
    async fn throughput(&self) -> i64 {
        self.throughput
    }
}
