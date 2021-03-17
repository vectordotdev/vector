use crate::event::{Metric, MetricValue};
use async_graphql::Object;
use chrono::{DateTime, Utc};

pub struct EventsInTotal(Metric);

impl EventsInTotal {
    pub fn new(m: Metric) -> Self {
        Self(m)
    }

    pub fn get_timestamp(&self) -> Option<DateTime<Utc>> {
        self.0.data.timestamp
    }

    pub fn get_events_in_total(&self) -> f64 {
        match self.0.data.value {
            MetricValue::Counter { value } => value,
            _ => 0.00,
        }
    }
}

#[Object]
impl EventsInTotal {
    /// Metric timestamp
    pub async fn timestamp(&self) -> Option<DateTime<Utc>> {
        self.get_timestamp()
    }

    /// Total number of events inputted
    pub async fn events_in_total(&self) -> f64 {
        self.get_events_in_total()
    }
}

impl From<Metric> for EventsInTotal {
    fn from(m: Metric) -> Self {
        Self(m)
    }
}

pub struct ComponentEventsInTotal {
    name: String,
    metric: Metric,
}

impl ComponentEventsInTotal {
    /// Returns a new `ComponentEventsInTotal` struct, which is a GraphQL type. The
    /// component name is hoisted for clear field resolution in the resulting payload
    pub fn new(metric: Metric) -> Self {
        let name = metric.tag_value("component_name").expect(
            "Returned a metric without a `component_name`, which shouldn't happen. Please report.",
        );

        Self { name, metric }
    }
}

#[Object]
impl ComponentEventsInTotal {
    /// Component name
    async fn name(&self) -> &str {
        &self.name
    }

    /// Events inputted total metric
    async fn metric(&self) -> EventsInTotal {
        EventsInTotal::new(self.metric.clone())
    }
}

pub struct ComponentEventsInThroughput {
    name: String,
    throughput: i64,
}

impl ComponentEventsInThroughput {
    /// Returns a new `ComponentEventsInThroughput`, set to the provided name/throughput values
    pub fn new(name: String, throughput: i64) -> Self {
        Self { name, throughput }
    }
}

#[Object]
impl ComponentEventsInThroughput {
    /// Component name
    async fn name(&self) -> &str {
        &self.name
    }

    /// Events processed throughput
    async fn throughput(&self) -> i64 {
        self.throughput
    }
}
