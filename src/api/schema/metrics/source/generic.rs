use async_graphql::Object;

use crate::{
    api::schema::metrics::{self, MetricsFilter},
    event::Metric,
};

#[derive(Debug, Clone)]
pub struct GenericSourceMetrics(Vec<Metric>);

impl GenericSourceMetrics {
    pub fn new(metrics: Vec<Metric>) -> Self {
        Self(metrics)
    }
}

#[Object]
impl GenericSourceMetrics {
    /// Total received bytes for the current source
    pub async fn received_bytes_total(&self) -> Option<metrics::ReceivedBytesTotal> {
        self.0.received_bytes_total()
    }

    /// Total received events for the current source
    pub async fn received_events_total(&self) -> Option<metrics::ReceivedEventsTotal> {
        self.0.received_events_total()
    }

    /// Total sent events for the current source
    pub async fn sent_events_total(&self) -> Option<metrics::SentEventsTotal> {
        self.0.sent_events_total()
    }
}
