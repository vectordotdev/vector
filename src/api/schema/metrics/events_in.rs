use async_graphql::Object;
use chrono::{DateTime, Utc};

use crate::event::{Metric, MetricValue};

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
