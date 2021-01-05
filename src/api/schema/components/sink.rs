use super::{source, state, transform, Component};
use crate::api::schema::metrics::{self, IntoSinkMetrics};
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
                Some(Component::Source(s)) => Some(s),
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
                Some(Component::Transform(t)) => Some(t),
                _ => None,
            })
            .collect()
    }

    /// Sink metrics
    pub async fn metrics(&self) -> metrics::SinkMetrics {
        metrics::by_component_name(&self.0.name).to_sink_metrics(&self.0.component_type)
    }
}
