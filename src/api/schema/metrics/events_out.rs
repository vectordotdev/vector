use async_graphql::Object;
use chrono::{DateTime, Utc};

use crate::{
    config::ComponentKey,
    event::{Metric, MetricValue},
};

pub struct EventsOutTotal(Metric);

impl EventsOutTotal {
    pub const fn new(m: Metric) -> Self {
        Self(m)
    }

    pub fn get_timestamp(&self) -> Option<DateTime<Utc>> {
        self.0.timestamp()
    }

    pub fn get_events_out_total(&self) -> f64 {
        match self.0.value() {
            MetricValue::Counter { value } => *value,
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

    /// Total outgoing events
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
    component_key: ComponentKey,
    metric: Metric,
}

impl ComponentEventsOutTotal {
    /// Returns a new `ComponentEventsOutTotal` struct, which is a GraphQL type. The
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
impl ComponentEventsOutTotal {
    /// Component id
    async fn component_id(&self) -> &str {
        self.component_key.id()
    }

    /// Total outgoing events metric
    async fn metric(&self) -> EventsOutTotal {
        EventsOutTotal::new(self.metric.clone())
    }
}

pub struct ComponentEventsOutThroughput {
    component_key: ComponentKey,
    throughput: i64,
}

impl ComponentEventsOutThroughput {
    /// Returns a new `ComponentEventsOutThroughput`, set to the provided id/throughput values
    pub const fn new(component_key: ComponentKey, throughput: i64) -> Self {
        Self {
            component_key,
            throughput,
        }
    }
}

#[Object]
impl ComponentEventsOutThroughput {
    /// Component id
    async fn component_id(&self) -> &str {
        self.component_key.id()
    }

    /// Events processed throughput
    async fn throughput(&self) -> i64 {
        self.throughput
    }
}
