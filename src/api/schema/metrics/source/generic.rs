use crate::{
    api::schema::metrics::{self, MetricsFilter},
    event::Metric,
};
use async_graphql::Object;

#[derive(Debug, Clone)]
pub struct GenericSourceMetrics(Vec<Metric>);

impl GenericSourceMetrics {
    pub fn new(metrics: Vec<Metric>) -> Self {
        Self(metrics)
    }
}

#[Object]
impl GenericSourceMetrics {
    /// Metric indicating events processed for the current source
    pub async fn processed_events_total(&self) -> Option<metrics::ProcessedEventsTotal> {
        self.0.processed_events_total()
    }

    /// Metric indicating bytes processed for the current source
    pub async fn processed_bytes_total(&self) -> Option<metrics::ProcessedBytesTotal> {
        self.0.processed_bytes_total()
    }
}
