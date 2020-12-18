use crate::{
    api::schema::metrics::{filter, ProcessedBytesTotal, ProcessedEventsTotal},
    event::Metric,
};
use async_graphql::Object;

#[derive(Debug, Clone)]
pub struct GenericSource(Metric);

#[Object]
impl GenericSource {
    /// Metric indicating events processed for the current source
    pub async fn processed_events_total(&self) -> Option<ProcessedEventsTotal> {
        filter::component_processed_events_total(&self.0.name)
    }

    /// Metric indicating bytes processed for the current source
    pub async fn processed_bytes_total(&self) -> Option<ProcessedBytesTotal> {
        filter::component_processed_bytes_total(&self.0.name)
    }
}
