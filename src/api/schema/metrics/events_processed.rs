use crate::event::{Metric, MetricValue};
use async_graphql::Object;
use chrono::{DateTime, Utc};

pub struct EventsProcessedTotal(Metric);

impl EventsProcessedTotal {
    pub fn new(m: Metric) -> Self {
        Self(m)
    }
}

#[Object]
impl EventsProcessedTotal {
    /// Metric timestamp
    pub async fn timestamp(&self) -> Option<DateTime<Utc>> {
        self.0.timestamp
    }

    /// Total number of events processed
    pub async fn events_processed_total(&self) -> f64 {
        match self.0.value {
            MetricValue::Counter { value } => value,
            _ => 0.00,
        }
    }
}

impl From<Metric> for EventsProcessedTotal {
    fn from(m: Metric) -> Self {
        Self(m)
    }
}

pub struct ComponentEventsProcessedTotal {
    name: String,
    metric: Metric,
}

impl ComponentEventsProcessedTotal {
    /// Returns a new `ComponentEventsProcessedTotal` struct, which is a GraphQL type. The
    /// component name is hoisted for clear field resolution in the resulting payload
    pub fn new(metric: Metric) -> Self {
        let name = metric.tag_value("component_name").expect(
            "Returned a metric without a `component_name`, which shouldn't happen. Please report.",
        );

        Self { name, metric }
    }
}

#[Object]
impl ComponentEventsProcessedTotal {
    /// Component name
    async fn name(&self) -> &str {
        &self.name
    }

    /// Events processed total metric
    async fn metric(&self) -> EventsProcessedTotal {
        EventsProcessedTotal::new(self.metric.clone())
    }
}

pub struct ComponentEventsProcessedThroughput {
    name: String,
    throughput: i64,
}

impl ComponentEventsProcessedThroughput {
    /// Returns a new `ComponentEventsProcessedThroughput`, set to the provided name/throughput values
    pub fn new(name: String, throughput: i64) -> Self {
        Self { name, throughput }
    }
}

#[Object]
impl ComponentEventsProcessedThroughput {
    /// Component name
    async fn name(&self) -> &str {
        &self.name
    }

    /// Events processed throughput
    async fn throughput(&self) -> i64 {
        self.throughput
    }
}
