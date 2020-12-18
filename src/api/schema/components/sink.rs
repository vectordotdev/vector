use super::{source, state, transform, Component};
use crate::api::schema::metrics;
use async_graphql::Object;

#[derive(Debug, Clone)]
pub struct Data {
    pub name: String,
    pub component_type: String,
    pub inputs: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Sink(pub Data);

#[Object]
impl Sink {
    /// Sink name
    pub async fn name(&self) -> &str {
        &self.0.name
    }

    /// Sink type
    pub async fn component_type(&self) -> &str {
        &*self.0.component_type
    }

    /// Source inputs
    pub async fn sources(&self) -> Vec<source::Source> {
        self.0
            .inputs
            .iter()
            .filter_map(|name| match state::component_by_name(name) {
                Component::Source(s) => Some(s),
                _ => None,
            })
            .collect()
    }

    /// Transform inputs
    pub async fn transforms(&self) -> Vec<transform::Transform> {
        self.0
            .inputs
            .iter()
            .filter_map(|name| match state::component_by_name(name) {
                Component::Transform(t) => Some(t),
                _ => None,
            })
            .collect()
    }

    /// Metric indicating events processed for the current sink
    pub async fn processed_events_total(&self) -> Option<metrics::ProcessedEventsTotal> {
        metrics::component_processed_events_total(&self.0.name)
    }

    /// Metric indicating bytes processed for the current sink
    pub async fn processed_bytes_total(&self) -> Option<metrics::ProcessedBytesTotal> {
        metrics::component_processed_bytes_total(&self.0.name)
    }
}
