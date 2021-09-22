use crate::config::ComponentKey;
use crate::event::{Metric, MetricValue};
use async_graphql::Object;
use chrono::{DateTime, Utc};

pub struct SentEventsTotal(Metric);

impl SentEventsTotal {
    pub const fn new(m: Metric) -> Self {
        Self(m)
    }

    pub fn get_timestamp(&self) -> Option<DateTime<Utc>> {
        self.0.timestamp()
    }

    pub fn get_sent_events_total(&self) -> f64 {
        match self.0.value() {
            MetricValue::Counter { value } => *value,
            _ => 0.00,
        }
    }
}

#[Object]
impl SentEventsTotal {
    /// Metric timestamp
    pub async fn timestamp(&self) -> Option<DateTime<Utc>> {
        self.get_timestamp()
    }

    /// Total outgoing events
    pub async fn events_out_total(&self) -> f64 {
        self.get_sent_events_total()
    }
}

impl From<Metric> for SentEventsTotal {
    fn from(m: Metric) -> Self {
        Self(m)
    }
}

pub struct ComponentSentEventsTotal {
    component_key: ComponentKey,
    metric: Metric,
}

impl ComponentSentEventsTotal {
    /// Returns a new `ComponentSentEventsTotal` struct, which is a GraphQL type. The
    /// component id is hoisted for clear field resolution in the resulting payload.
    pub fn new(metric: Metric) -> Self {
        let component_key = metric.tag_value("component_id").expect(
            "Returned a metric without a `component_id`, which shouldn't happen. Please report.",
        );
        let component_key = ComponentKey::from((metric.tag_value("pipeline_id"), component_key));

        Self {
            component_key,
            metric,
        }
    }
}

#[Object]
impl ComponentSentEventsTotal {
    /// Component id
    async fn component_id(&self) -> &str {
        self.component_key.id()
    }

    /// Pipeline id
    async fn pipeline_id(&self) -> Option<&str> {
        self.component_key.pipeline_str()
    }

    /// Total outgoing events metric
    async fn metric(&self) -> SentEventsTotal {
        SentEventsTotal::new(self.metric.clone())
    }
}

pub struct ComponentSentEventsThroughput {
    component_key: ComponentKey,
    throughput: i64,
}

impl ComponentSentEventsThroughput {
    /// Returns a new `ComponentSentEventsThroughput`, set to the provided id/throughput values
    pub const fn new(component_key: ComponentKey, throughput: i64) -> Self {
        Self {
            component_key,
            throughput,
        }
    }
}

#[Object]
impl ComponentSentEventsThroughput {
    /// Component id
    async fn component_id(&self) -> &str {
        self.component_key.id()
    }

    /// Pipeline id
    async fn pipeline_id(&self) -> Option<&str> {
        self.component_key.pipeline_str()
    }

    /// Events processed throughput
    async fn throughput(&self) -> i64 {
        self.throughput
    }
}
