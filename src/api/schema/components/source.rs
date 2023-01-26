use std::cmp;

use async_graphql::{Enum, InputObject, Object};

use super::{sink, state, transform, Component};
use crate::{
    api::schema::{
        filter,
        metrics::{self, outputs_by_component_key, IntoSourceMetrics, Output},
        sort,
    },
    config::{ComponentKey, DataType, OutputId},
    filter_check,
};

#[derive(Debug, Enum, Eq, PartialEq, Copy, Clone, Ord, PartialOrd)]
pub enum SourceOutputType {
    Log,
    Metric,
    Trace,
}

#[derive(Debug, Clone)]
pub struct Data {
    pub component_key: ComponentKey,
    pub component_type: String,
    pub output_type: DataType,
    pub outputs: Vec<String>,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum SourcesSortFieldName {
    ComponentKey,
    ComponentType,
    OutputType,
}

#[derive(Debug, Clone)]
pub struct Source(pub Data);

impl Source {
    #[allow(clippy::missing_const_for_fn)] // const cannot run destructor
    pub fn get_component_key(&self) -> &ComponentKey {
        &self.0.component_key
    }
    pub fn get_component_type(&self) -> &str {
        self.0.component_type.as_str()
    }
    pub fn get_output_types(&self) -> Vec<SourceOutputType> {
        [
            SourceOutputType::Log,
            SourceOutputType::Metric,
            SourceOutputType::Trace,
        ]
        .iter()
        .copied()
        .filter(|s| self.0.output_type.contains(s.into()))
        .collect()
    }

    pub fn get_outputs(&self) -> &[String] {
        self.0.outputs.as_ref()
    }
}

impl From<&SourceOutputType> for DataType {
    fn from(s: &SourceOutputType) -> Self {
        match s {
            SourceOutputType::Log => DataType::Log,
            SourceOutputType::Metric => DataType::Metric,
            SourceOutputType::Trace => DataType::Trace,
        }
    }
}

impl sort::SortableByField<SourcesSortFieldName> for Source {
    fn sort(&self, rhs: &Self, field: &SourcesSortFieldName) -> cmp::Ordering {
        match field {
            SourcesSortFieldName::ComponentKey => {
                Ord::cmp(self.get_component_key(), rhs.get_component_key())
            }
            SourcesSortFieldName::ComponentType => {
                Ord::cmp(self.get_component_type(), rhs.get_component_type())
            }
            SourcesSortFieldName::OutputType => {
                Ord::cmp(&u8::from(self.0.output_type), &u8::from(rhs.0.output_type))
            }
        }
    }
}

#[Object]
impl Source {
    /// Source component_id
    pub async fn component_id(&self) -> &str {
        self.0.component_key.id()
    }

    /// Source type
    pub async fn component_type(&self) -> &str {
        self.get_component_type()
    }

    /// Source output type
    pub async fn output_types(&self) -> Vec<SourceOutputType> {
        self.get_output_types()
    }

    /// Source output streams
    pub async fn outputs(&self) -> Vec<Output> {
        outputs_by_component_key(self.get_component_key(), self.get_outputs())
    }

    /// Transform outputs
    pub async fn transforms(&self) -> Vec<transform::Transform> {
        state::filter_components(|(_component_key, components)| match components {
            Component::Transform(t)
                if t.0.inputs.contains(&OutputId::from(&self.0.component_key)) =>
            {
                Some(t.clone())
            }
            _ => None,
        })
    }

    /// Sink outputs
    pub async fn sinks(&self) -> Vec<sink::Sink> {
        state::filter_components(|(_component_key, components)| match components {
            Component::Sink(s) if s.0.inputs.contains(&OutputId::from(&self.0.component_key)) => {
                Some(s.clone())
            }
            _ => None,
        })
    }

    /// Source metrics
    pub async fn metrics(&self) -> metrics::SourceMetrics {
        metrics::by_component_key(&self.0.component_key)
            .into_source_metrics(self.get_component_type())
    }
}

#[derive(Default, InputObject)]
pub(super) struct SourcesFilter {
    component_id: Option<Vec<filter::StringFilter>>,
    component_type: Option<Vec<filter::StringFilter>>,
    output_type: Option<Vec<filter::ListFilter<SourceOutputType>>>,
    or: Option<Vec<Self>>,
}

impl filter::CustomFilter<Source> for SourcesFilter {
    fn matches(&self, source: &Source) -> bool {
        filter_check!(
            self.component_id.as_ref().map(|f| f
                .iter()
                .all(|f| f.filter_value(&source.get_component_key().to_string()))),
            self.component_type.as_ref().map(|f| f
                .iter()
                .all(|f| f.filter_value(source.get_component_type()))),
            self.output_type
                .as_ref()
                .map(|f| f.iter().all(|f| f.filter_value(source.get_output_types())))
        );
        true
    }

    fn or(&self) -> Option<&Vec<Self>> {
        self.or.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DataType;

    /// Generate component fixes for use with tests
    fn source_fixtures() -> Vec<Source> {
        vec![
            Source(Data {
                component_key: ComponentKey::from("gen1"),
                component_type: "demo_logs".to_string(),
                output_type: DataType::Log | DataType::Metric,
                outputs: vec![],
            }),
            Source(Data {
                component_key: ComponentKey::from("gen2"),
                component_type: "demo_logs".to_string(),
                output_type: DataType::Log,
                outputs: vec![],
            }),
            Source(Data {
                component_key: ComponentKey::from("gen3"),
                component_type: "demo_logs".to_string(),
                output_type: DataType::Metric,
                outputs: vec![],
            }),
        ]
    }

    #[test]
    fn filter_output_type() {
        struct Test {
            component_id: &'static str,
            output_types: Vec<SourceOutputType>,
        }

        let tests = vec![
            Test {
                component_id: "gen1",
                output_types: vec![SourceOutputType::Log, SourceOutputType::Metric],
            },
            Test {
                component_id: "gen2",
                output_types: vec![SourceOutputType::Log],
            },
            Test {
                component_id: "gen3",
                output_types: vec![SourceOutputType::Metric],
            },
        ];

        for t in tests {
            let filter = SourcesFilter {
                component_id: Some(vec![filter::StringFilter {
                    equals: Some(t.component_id.to_string()),
                    ..Default::default()
                }]),
                output_type: Some(vec![filter::ListFilter::<SourceOutputType> {
                    equals: Some(t.output_types),
                    not_equals: None,
                    contains: None,
                    not_contains: None,
                }]),
                ..Default::default()
            };

            let sources = filter::filter_items(source_fixtures().into_iter(), &filter);
            assert_eq!(sources.len(), 1);
        }
    }

    #[test]
    fn sort_component_id_desc() {
        let mut sources = source_fixtures();
        let fields = vec![sort::SortField::<SourcesSortFieldName> {
            field: SourcesSortFieldName::ComponentKey,
            direction: sort::Direction::Desc,
        }];
        sort::by_fields(&mut sources, &fields);

        for (i, component_id) in ["gen3", "gen2", "gen1"].iter().enumerate() {
            assert_eq!(sources[i].get_component_key().to_string(), *component_id);
        }
    }

    #[test]
    fn sort_component_type_asc() {
        let mut sources = vec![
            Source(Data {
                component_key: ComponentKey::from("gen2"),
                component_type: "file".to_string(),
                output_type: DataType::Log | DataType::Metric,
                outputs: vec![],
            }),
            Source(Data {
                component_key: ComponentKey::from("gen3"),
                component_type: "demo_logs".to_string(),
                output_type: DataType::Log,
                outputs: vec![],
            }),
            Source(Data {
                component_key: ComponentKey::from("gen1"),
                component_type: "docker_logs".to_string(),
                output_type: DataType::Metric,
                outputs: vec![],
            }),
        ];

        let fields = vec![sort::SortField::<SourcesSortFieldName> {
            field: SourcesSortFieldName::ComponentType,
            direction: sort::Direction::Asc,
        }];
        sort::by_fields(&mut sources, &fields);

        for (i, component_id) in ["gen3", "gen1", "gen2"].iter().enumerate() {
            assert_eq!(sources[i].get_component_key().to_string(), *component_id);
        }
    }

    #[test]
    fn sort_component_type_desc() {
        let mut sources = vec![
            Source(Data {
                component_key: ComponentKey::from("gen3"),
                component_type: "file".to_string(),
                output_type: DataType::Log | DataType::Metric,
                outputs: vec![],
            }),
            Source(Data {
                component_key: ComponentKey::from("gen2"),
                component_type: "demo_logs".to_string(),
                output_type: DataType::Log,
                outputs: vec![],
            }),
            Source(Data {
                component_key: ComponentKey::from("gen1"),
                component_type: "docker_logs".to_string(),
                output_type: DataType::Metric,
                outputs: vec![],
            }),
        ];

        let fields = vec![sort::SortField::<SourcesSortFieldName> {
            field: SourcesSortFieldName::ComponentType,
            direction: sort::Direction::Desc,
        }];
        sort::by_fields(&mut sources, &fields);

        for (i, component_id) in ["gen3", "gen1", "gen2"].iter().enumerate() {
            assert_eq!(sources[i].get_component_key().to_string(), *component_id);
        }
    }

    #[test]
    fn sort_output_type_asc() {
        let mut sources = vec![
            Source(Data {
                component_key: ComponentKey::from("gen4"),
                component_type: "demo_trace".to_string(),
                output_type: DataType::Trace,
                outputs: vec![],
            }),
            Source(Data {
                component_key: ComponentKey::from("gen1"),
                component_type: "demo_logs".to_string(),
                output_type: DataType::Metric,
                outputs: vec![],
            }),
            Source(Data {
                component_key: ComponentKey::from("gen2"),
                component_type: "file".to_string(),
                output_type: DataType::Log,
                outputs: vec![],
            }),
            Source(Data {
                component_key: ComponentKey::from("gen3"),
                component_type: "multiple_type".to_string(),
                output_type: DataType::Log | DataType::Metric | DataType::Trace,
                outputs: vec![],
            }),
        ];

        let fields = vec![sort::SortField::<SourcesSortFieldName> {
            field: SourcesSortFieldName::OutputType,
            direction: sort::Direction::Asc,
        }];
        sort::by_fields(&mut sources, &fields);

        for (i, component_id) in ["gen2", "gen1", "gen4", "gen3"].iter().enumerate() {
            assert_eq!(sources[i].get_component_key().to_string(), *component_id);
        }
    }

    #[test]
    fn sort_output_type_desc() {
        let mut sources = vec![
            Source(Data {
                component_key: ComponentKey::from("gen4"),
                component_type: "demo_trace".to_string(),
                output_type: DataType::Trace,
                outputs: vec![],
            }),
            Source(Data {
                component_key: ComponentKey::from("gen1"),
                component_type: "demo_logs".to_string(),
                output_type: DataType::Metric,
                outputs: vec![],
            }),
            Source(Data {
                component_key: ComponentKey::from("gen2"),
                component_type: "file".to_string(),
                output_type: DataType::Log,
                outputs: vec![],
            }),
            Source(Data {
                component_key: ComponentKey::from("gen3"),
                component_type: "multiple_type".to_string(),
                output_type: DataType::Log | DataType::Metric | DataType::Trace,
                outputs: vec![],
            }),
        ];

        let fields = vec![sort::SortField::<SourcesSortFieldName> {
            field: SourcesSortFieldName::OutputType,
            direction: sort::Direction::Desc,
        }];
        sort::by_fields(&mut sources, &fields);
        for (i, component_id) in ["gen3", "gen4", "gen1", "gen2"].iter().enumerate() {
            assert_eq!(sources[i].get_component_key().to_string(), *component_id);
        }
    }
}
