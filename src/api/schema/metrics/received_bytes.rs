use async_graphql::Object;
use chrono::{DateTime, Utc};

use crate::{
    config::ComponentKey,
    event::{Metric, MetricValue},
};

pub struct ReceivedBytesTotal(Metric);

impl ReceivedBytesTotal {
    pub const fn new(m: Metric) -> Self {
        Self(m)
    }

    pub fn get_timestamp(&self) -> Option<DateTime<Utc>> {
        self.0.timestamp()
    }

    pub fn get_received_bytes_total(&self) -> f64 {
        match self.0.value() {
            MetricValue::Counter { value } => *value,
            _ => 0.00,
        }
    }
}

#[Object]
impl ReceivedBytesTotal {
    /// Metric timestamp.
    pub async fn timestamp(&self) -> Option<DateTime<Utc>> {
        self.get_timestamp()
    }

    /// Total number of bytes received.
    pub async fn received_bytes_total(&self) -> f64 {
        self.get_received_bytes_total()
    }
}

impl From<Metric> for ReceivedBytesTotal {
    fn from(m: Metric) -> Self {
        Self(m)
    }
}

pub struct ComponentReceivedBytesTotal {
    component_key: ComponentKey,
    metric: Metric,
}

impl ComponentReceivedBytesTotal {
    /// Returns a new `ComponentReceivedBytesTotal`.
    ///
    /// Expects that the metric contains a tag for the component ID the metric is referenced to.
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
impl ComponentReceivedBytesTotal {
    /// Component ID.
    async fn component_id(&self) -> &str {
        self.component_key.id()
    }

    /// Metric for total bytes received.
    async fn metric(&self) -> ReceivedBytesTotal {
        ReceivedBytesTotal::new(self.metric.clone())
    }
}

pub struct ComponentReceivedBytesThroughput {
    component_key: ComponentKey,
    throughput: i64,
}

impl ComponentReceivedBytesThroughput {
    /// Returns a new `ComponentReceivedBytesThroughput` for the given component.
    pub const fn new(component_key: ComponentKey, throughput: i64) -> Self {
        Self {
            component_key,
            throughput,
        }
    }
}

#[Object]
impl ComponentReceivedBytesThroughput {
    /// Component ID.
    async fn component_id(&self) -> &str {
        self.component_key.id()
    }

    /// Throughput of bytes sent.
    async fn throughput(&self) -> i64 {
        self.throughput
    }
}
