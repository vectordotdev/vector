use async_graphql::Object;

use crate::{
    api::schema::metrics::{self, MetricsFilter},
    event::Metric,
};

#[derive(Debug, Clone)]
pub struct GenericSinkMetrics(Vec<Metric>);

impl GenericSinkMetrics {
    pub fn new(metrics: Vec<Metric>) -> Self {
        Self(metrics)
    }
}

#[Object]
impl GenericSinkMetrics {
    /// Total received events for the current sink
    pub async fn received_events_total(&self) -> Option<metrics::ReceivedEventsTotal> {
        self.0.received_events_total()
    }

    /// Total sent bytes for the current sink
    pub async fn sent_bytes_total(&self) -> Option<metrics::SentBytesTotal> {
        self.0.sent_bytes_total()
    }

    /// Total sent events for the current sink
    pub async fn sent_events_total(&self) -> Option<metrics::SentEventsTotal> {
        self.0.sent_events_total()
    }
}
