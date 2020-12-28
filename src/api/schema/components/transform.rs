use super::{sink, source, state, Component};
use crate::api::schema::metrics::{self, IntoTransformMetrics};
use async_graphql::Object;

#[derive(Debug, Clone)]
pub struct Data {
    pub name: String,
    pub component_type: String,
    pub inputs: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Transform(pub Data);

#[Object]
impl Transform {
    /// Transform name
    pub async fn name(&self) -> &str {
        &self.0.name
    }

    /// Transform type
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

    /// Transform outputs
    pub async fn transforms(&self) -> Vec<Transform> {
        state::filter_components(|(_name, components)| match components {
            Component::Transform(t) if t.0.inputs.contains(&self.0.name) => Some(t.clone()),
            _ => None,
        })
    }

    /// Sink outputs
    pub async fn sinks(&self) -> Vec<sink::Sink> {
        state::filter_components(|(_name, components)| match components {
            Component::Sink(s) if s.0.inputs.contains(&self.0.name) => Some(s.clone()),
            _ => None,
        })
    }

    /// Transform metrics
    pub async fn metrics(&self) -> metrics::TransformMetrics {
        metrics::by_component_name(&self.0.name).to_transform_metrics(&self.0.component_type)
    }
}
