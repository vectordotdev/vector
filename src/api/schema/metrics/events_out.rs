use async_graphql::Object;
use chrono::{DateTime, Utc};

use crate::event::{Metric, MetricValue};

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
