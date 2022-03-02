use async_graphql::Object;
use chrono::{DateTime, Utc};

use crate::{
    config::ComponentKey,
    event::{Metric, MetricValue},
};

pub struct ProcessedBytesTotal(Metric);

impl ProcessedBytesTotal {
    pub const fn new(m: Metric) -> Self {
        Self(m)
    }

    pub fn get_timestamp(&self) -> Option<DateTime<Utc>> {
        self.0.timestamp()
    }

    pub fn get_processed_bytes_total(&self) -> f64 {
        match self.0.value() {
            MetricValue::Counter { value } => *value,
            _ => 0.00,
        }
    }
}

#[Object]
impl ProcessedBytesTotal {
    /// Metric timestamp
    pub async fn timestamp(&self) -> Option<DateTime<Utc>> {
        self.get_timestamp()
    }

    /// Total number of bytes processed
    pub async fn processed_bytes_total(&self) -> f64 {
        self.get_processed_bytes_total()
    }
}

impl From<Metric> for ProcessedBytesTotal {
    fn from(m: Metric) -> Self {
        Self(m)
    }
}

pub struct ComponentProcessedBytesTotal {
    component_key: ComponentKey,
    metric: Metric,
}

impl ComponentProcessedBytesTotal {
    /// Returns a new `ComponentProcessedBytesTotal` struct, which is a GraphQL type. The
    /// component id is hoisted for clear field resolution in the resulting payload
    pub fn new(metric: Metric) -> Self {
        let component_key = metric.tag_value("component_id").expect(
            "Returned a metric without a `component_id`, which shouldn't happen. Please report.",
        );
        let component_key = ComponentKey::from(component_key);

        Self {
            component_key,
            metric,
        }
    }
}

#[Object]
impl ComponentProcessedBytesTotal {
    /// Component id
    async fn component_id(&self) -> &str {
        self.component_key.id()
    }

    /// Bytes processed total metric
    async fn metric(&self) -> ProcessedBytesTotal {
        ProcessedBytesTotal::new(self.metric.clone())
    }
}

pub struct ComponentProcessedBytesThroughput {
    component_key: ComponentKey,
    throughput: i64,
}

impl ComponentProcessedBytesThroughput {
    /// Returns a new `ComponentProcessedBytesThroughput`, set to the provided id/throughput values
    pub const fn new(component_key: ComponentKey, throughput: i64) -> Self {
        Self {
            component_key,
            throughput,
        }
    }
}

#[Object]
impl ComponentProcessedBytesThroughput {
    /// Component id
    async fn component_id(&self) -> &str {
        self.component_key.id()
    }

    /// Bytes processed throughput
    async fn throughput(&self) -> i64 {
        self.throughput
    }
}
