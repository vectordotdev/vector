use crate::{
    api::schema::metrics::{self, MetricsFilter},
    event::Metric,
};
use async_graphql::Object;

#[derive(Debug, Clone)]
pub struct GenericTransformMetrics(Vec<Metric>);

impl GenericTransformMetrics {
    pub fn new(metrics: Vec<Metric>) -> Self {
        Self(metrics)
    }
}

#[Object]
impl GenericTransformMetrics {
    /// Events processed for the current transform
    pub async fn processed_events_total(&self) -> Option<metrics::ProcessedEventsTotal> {
        self.0.processed_events_total()
    }

    /// Bytes processed for the current transform
    pub async fn processed_bytes_total(&self) -> Option<metrics::ProcessedBytesTotal> {
        self.0.processed_bytes_total()
    }
}
