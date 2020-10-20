use crate::event::{Metric, MetricValue};
use async_graphql::Object;
use chrono::{DateTime, Utc};

pub struct EventsProcessed(Metric);

impl EventsProcessed {
    pub fn new(m: Metric) -> Self {
        Self(m)
    }
}

#[Object]
impl EventsProcessed {
    /// Metric timestamp
    pub async fn timestamp(&self) -> Option<DateTime<Utc>> {
        self.0.timestamp
    }

    /// Number of events processed
    pub async fn events_processed(&self) -> f64 {
        match self.0.value {
            MetricValue::Counter { value } => value,
            _ => 0.00,
        }
    }
}

impl From<Metric> for EventsProcessed {
    fn from(m: Metric) -> Self {
        Self(m)
    }
}
