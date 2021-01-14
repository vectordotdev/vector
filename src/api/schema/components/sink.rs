use super::{source, state, transform, Component};
use crate::{
    api::schema::{
        filter,
        metrics::{self, IntoSinkMetrics},
    },
    filter_check,
};
use async_graphql::{InputObject, Object};

#[derive(Debug, Clone)]
pub struct Data {
    pub name: String,
    pub component_type: String,
    pub inputs: Vec<String>,
}

impl Data {
    pub fn get_name(&self) -> &str {
        self.name.as_str()
    }
    pub fn get_component_type(&self) -> &str {
        self.component_type.as_str()
    }
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

#[derive(Default, InputObject)]
pub struct SinksFilter {
    name: Option<Vec<filter::StringFilter>>,
    component_type: Option<Vec<filter::StringFilter>>,
    or: Option<Vec<Self>>,
}

impl filter::CustomFilter<Sink> for SinksFilter {
    fn matches(&self, sink: &Sink) -> bool {
        filter_check!(
            self.name
                .as_ref()
                .map(|f| f.iter().all(|f| f.filter_value(sink.0.get_name()))),
            self.component_type.as_ref().map(|f| f
                .iter()
                .all(|f| f.filter_value(sink.0.get_component_type())))
        );
        true
    }

    fn or(&self) -> Option<&Vec<Self>> {
        self.or.as_ref()
    }
}
