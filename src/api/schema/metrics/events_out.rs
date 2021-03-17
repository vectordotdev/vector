use crate::event::{Metric, MetricValue};
use async_graphql::Object;
use chrono::{DateTime, Utc};

pub struct EventsOutTotal(Metric);

impl EventsOutTotal {
    pub fn new(m: Metric) -> Self {
        Self(m)
    }

    pub fn get_timestamp(&self) -> Option<DateTime<Utc>> {
        self.0.data.timestamp
    }

    pub fn get_events_out_total(&self) -> f64 {
        match self.0.data.value {
            MetricValue::Counter { value } => value,
            _ => 0.00,
        }
    }
}

#[Object]
impl EventsOutTotal {
    /// Metric timestamp
    pub async fn timestamp(&self) -> Option<DateTime<Utc>> {
        self.get_timestamp()
    }

    /// Total number of events outputted
    pub async fn events_out_total(&self) -> f64 {
        self.get_events_out_total()
    }
}

impl From<Metric> for EventsOutTotal {
    fn from(m: Metric) -> Self {
        Self(m)
    }
}

pub struct ComponentEventsOutTotal {
    name: String,
    metric: Metric,
}

impl ComponentEventsOutTotal {
    /// Returns a new `ComponentEventsOutTotal` struct, which is a GraphQL type. The
    /// component name is hoisted for clear field resolution in the resulting payload
    pub fn new(metric: Metric) -> Self {
        let name = metric.tag_value("component_name").expect(
            "Returned a metric without a `component_name`, which shouldn't happen. Please report.",
        );

        Self { name, metric }
    }
}

#[Object]
impl ComponentEventsOutTotal {
    /// Component name
    async fn name(&self) -> &str {
        &self.name
    }

    /// Events outputted total metric
    async fn metric(&self) -> EventsOutTotal {
        EventsOutTotal::new(self.metric.clone())
    }
}

pub struct ComponentEventsOutThroughput {
    name: String,
    throughput: i64,
}

impl ComponentEventsOutThroughput {
    /// Returns a new `ComponentEventsOutThroughput`, set to the provided name/throughput values
    pub fn new(name: String, throughput: i64) -> Self {
        Self { name, throughput }
    }
}

#[Object]
impl ComponentEventsOutThroughput {
    /// Component name
    async fn name(&self) -> &str {
        &self.name
    }

    /// Events processed throughput
    async fn throughput(&self) -> i64 {
        self.throughput
    }
}
