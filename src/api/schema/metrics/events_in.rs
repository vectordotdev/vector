use async_graphql::Object;
use chrono::{DateTime, Utc};

use crate::{
    config::ComponentKey,
    event::{Metric, MetricValue},
};

pub struct EventsInTotal(Metric);

impl EventsInTotal {
    pub const fn new(m: Metric) -> Self {
        Self(m)
    }

    pub fn get_timestamp(&self) -> Option<DateTime<Utc>> {
        self.0.timestamp()
    }

    pub fn get_events_in_total(&self) -> f64 {
        match self.0.value() {
            MetricValue::Counter { value } => *value,
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

    /// Total incoming events
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
    component_key: ComponentKey,
    metric: Metric,
}

impl ComponentEventsInTotal {
    /// Returns a new `ComponentEventsInTotal` struct, which is a GraphQL type. The
    /// component id is hoisted for clear field resolution in the resulting payload.
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
impl ComponentEventsInTotal {
    /// Component id
    async fn component_id(&self) -> &str {
        self.component_key.id()
    }

    /// Total incoming events metric
    async fn metric(&self) -> EventsInTotal {
        EventsInTotal::new(self.metric.clone())
    }
}

pub struct ComponentEventsInThroughput {
    component_key: ComponentKey,
    throughput: i64,
}

impl ComponentEventsInThroughput {
    /// Returns a new `ComponentEventsInThroughput`, set to the provided id/throughput values.
    pub const fn new(component_key: ComponentKey, throughput: i64) -> Self {
        Self {
            component_key,
            throughput,
        }
    }
}

#[Object]
impl ComponentEventsInThroughput {
    /// Component id
    async fn component_id(&self) -> &str {
        self.component_key.id()
    }

    /// Events processed throughput
    async fn throughput(&self) -> i64 {
        self.throughput
    }
}
