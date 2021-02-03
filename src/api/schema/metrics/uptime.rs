use crate::event::{Metric, MetricValue};
use async_graphql::Object;
use chrono::{DateTime, Utc};

pub struct Uptime(Metric);

impl Uptime {
    pub fn new(m: Metric) -> Self {
        Self(m)
    }
}

#[Object]
impl Uptime {
    /// Metric timestamp
    pub async fn timestamp(&self) -> Option<DateTime<Utc>> {
        self.0.data.timestamp
    }

    /// Number of seconds the Vector instance has been alive
    pub async fn seconds(&self) -> f64 {
        match self.0.data.value {
            MetricValue::Gauge { value } => value,
            _ => 0.00,
        }
    }
}

impl From<Metric> for Uptime {
    fn from(m: Metric) -> Self {
        Self(m)
    }
}
