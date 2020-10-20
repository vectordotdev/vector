use crate::event::{Metric, MetricValue};
use async_graphql::Object;
use chrono::{DateTime, Utc};

pub struct BytesProcessed(Metric);

impl BytesProcessed {
    pub fn new(m: Metric) -> Self {
        Self(m)
    }
}

#[Object]
impl BytesProcessed {
    /// Metric timestamp
    pub async fn timestamp(&self) -> Option<DateTime<Utc>> {
        self.0.timestamp
    }

    /// Number of bytes processed
    pub async fn bytes_processed(&self) -> f64 {
        match self.0.value {
            MetricValue::Counter { value } => value,
            _ => 0.00,
        }
    }
}

impl From<Metric> for BytesProcessed {
    fn from(m: Metric) -> Self {
        Self(m)
    }
}
