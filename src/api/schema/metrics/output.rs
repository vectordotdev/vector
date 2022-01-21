use async_graphql::Object;

use crate::event::{Metric, MetricValue};

#[derive(Debug, Clone)]
pub struct Output {
    output_id: String,
    sent_events_total: Option<Metric>,
}

impl Output {
    pub const fn new(output_id: String, sent_events_total: Option<Metric>) -> Self {
        Self {
            output_id,
            sent_events_total,
        }
    }
}

#[Object]
impl Output {
    /// Id of the output stream
    pub async fn output_id(&self) -> &str {
        self.output_id.as_ref()
    }

    /// Total sent events for the current output stream
    pub async fn sent_events_total(&self) -> f64 {
        if let Some(metric) = &self.sent_events_total {
            match metric.value() {
                MetricValue::Counter { value } => *value,
                _ => 0.00,
            }
        } else {
            0.00
        }
    }
}
