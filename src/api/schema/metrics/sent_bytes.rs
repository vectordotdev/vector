use async_graphql::Object;
use chrono::{DateTime, Utc};

use crate::{
    config::ComponentKey,
    event::{Metric, MetricValue},
};

pub struct SentBytesTotal(Metric);

impl SentBytesTotal {
    pub const fn new(m: Metric) -> Self {
        Self(m)
    }

    pub fn get_timestamp(&self) -> Option<DateTime<Utc>> {
        self.0.timestamp()
    }

    pub fn get_sent_bytes_total(&self) -> f64 {
        match self.0.value() {
            MetricValue::Counter { value } => *value,
            _ => 0.00,
        }
    }
}

#[Object]
impl SentBytesTotal {
    /// Metric timestamp.
    pub async fn timestamp(&self) -> Option<DateTime<Utc>> {
        self.get_timestamp()
    }

    /// Total number of bytes sent.
    pub async fn sent_bytes_total(&self) -> f64 {
        self.get_sent_bytes_total()
    }
}

impl From<Metric> for SentBytesTotal {
    fn from(m: Metric) -> Self {
        Self(m)
    }
}

pub struct ComponentSentBytesTotal {
    component_key: ComponentKey,
    metric: Metric,
}

impl ComponentSentBytesTotal {
    /// Returns a new `ComponentSentBytesTotal` for the given metric.
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
impl ComponentSentBytesTotal {
    /// Component ID.
    async fn component_id(&self) -> &str {
        self.component_key.id()
    }

    /// Metric for total bytes sent.
    async fn metric(&self) -> SentBytesTotal {
        SentBytesTotal::new(self.metric.clone())
    }
}

pub struct ComponentSentBytesThroughput {
    component_key: ComponentKey,
    throughput: i64,
}

impl ComponentSentBytesThroughput {
    /// Returns a new `ComponentSentBytesThroughput` for the given component.
    pub const fn new(component_key: ComponentKey, throughput: i64) -> Self {
        Self {
            component_key,
            throughput,
        }
    }
}

#[Object]
impl ComponentSentBytesThroughput {
    /// Component ID.
    async fn component_id(&self) -> &str {
        self.component_key.id()
    }

    /// Throughput of bytes sent.
    async fn throughput(&self) -> i64 {
        self.throughput
    }
}
