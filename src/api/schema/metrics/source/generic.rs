use crate::{
    api::schema::metrics::{self, MetricsFilter},
    event::Metric,
};
use async_graphql::Object;

#[derive(Debug, Clone)]
pub struct GenericSource(Metric);

#[Object]
impl GenericSource {
    /// Metric indicating events processed for the current source
    pub async fn processed_events_total(&self) -> Option<metrics::ProcessedEventsTotal> {
        metrics::by_component_name(&self.0.name).processed_events_total()
    }

    /// Metric indicating bytes processed for the current source
    pub async fn processed_bytes_total(&self) -> Option<metrics::ProcessedBytesTotal> {
        metrics::by_component_name(&self.0.name).processed_bytes_total()
    }
}
