use super::{sink, state, transform, Component};
use crate::{
    api::schema::{
        filter,
        metrics::{self, IntoSourceMetrics},
    },
    config::DataType,
    filter_check,
};
use async_graphql::{Enum, InputObject, Object};

#[derive(Debug, Enum, Eq, PartialEq, Copy, Clone)]
pub enum SourceOutputType {
    Any,
    Log,
    Metric,
}

impl From<DataType> for SourceOutputType {
    fn from(data_type: DataType) -> Self {
        match data_type {
            DataType::Metric => SourceOutputType::Metric,
            DataType::Log => SourceOutputType::Log,
            DataType::Any => SourceOutputType::Any,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Data {
    pub name: String,
    pub component_type: String,
    pub output_type: DataType,
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
pub struct Source(pub Data);

#[Object]
impl Source {
    /// Source name
    pub async fn name(&self) -> &str {
        &*self.0.name
    }

    /// Source type
    pub async fn component_type(&self) -> &str {
        &*self.0.component_type
    }

    /// Source output type
    pub async fn output_type(&self) -> SourceOutputType {
        self.0.output_type.into()
    }

    /// Transform outputs
    pub async fn transforms(&self) -> Vec<transform::Transform> {
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

    /// Source metrics
    pub async fn metrics(&self) -> metrics::SourceMetrics {
        metrics::by_component_name(&self.0.name).to_source_metrics(&self.0.component_type)
    }
}

#[derive(Default, InputObject)]
pub struct SourcesFilter {
    name: Option<Vec<filter::StringFilter>>,
    component_type: Option<Vec<filter::StringFilter>>,
    output_type: Option<Vec<filter::EqualityFilter<SourceOutputType>>>,
    or: Option<Vec<Self>>,
}

impl filter::CustomFilter<Source> for SourcesFilter {
    fn matches(&self, source: &Source) -> bool {
        filter_check!(
            self.name
                .as_ref()
                .map(|f| f.iter().all(|f| f.filter_value(source.0.get_name()))),
            self.component_type.as_ref().map(|f| f
                .iter()
                .all(|f| f.filter_value(source.0.get_component_type()))),
            self.output_type.as_ref().map(|f| f
                .iter()
                .all(|f| f.filter_value(source.0.output_type.into())))
        );
        true
    }

    fn or(&self) -> Option<&Vec<Self>> {
        self.or.as_ref()
    }
}
