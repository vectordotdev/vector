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
    pub fn new(metric: Metric) -> Self {
        Self {
            name: metric.tag_value("component_name").unwrap().clone(),
            metric,
        }
    }
}

#[Object]
impl ComponentEventsProcessedTotal {
    /// Component name
    async fn name(&self) -> String {
        self.name.clone()
    }

    /// Events processed total metric
    async fn metric(&self) -> EventsProcessedTotal {
        EventsProcessedTotal::new(self.metric.clone())
    }
}
