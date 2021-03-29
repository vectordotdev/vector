use super::{sink, state, transform, Component};
use crate::{
    api::schema::{
        filter,
        metrics::{self, IntoSourceMetrics},
        sort,
    },
    config::DataType,
    filter_check,
};
use async_graphql::{Enum, InputObject, Object};
use std::cmp;

#[derive(Debug, Enum, Eq, PartialEq, Copy, Clone, Ord, PartialOrd)]
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

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum SourcesSortFieldName {
    Name,
    ComponentType,
    OutputType,
}

#[derive(Debug, Clone)]
pub struct Source(pub Data);

impl Source {
    pub fn get_name(&self) -> &str {
        self.0.name.as_str()
    }
    pub fn get_component_type(&self) -> &str {
        self.0.component_type.as_str()
    }
    pub fn get_output_type(&self) -> SourceOutputType {
        self.0.output_type.into()
    }
}

impl sort::SortableByField<SourcesSortFieldName> for Source {
    fn sort(&self, rhs: &Self, field: &SourcesSortFieldName) -> cmp::Ordering {
        match field {
            SourcesSortFieldName::Name => Ord::cmp(self.get_name(), rhs.get_name()),
            SourcesSortFieldName::ComponentType => {
                Ord::cmp(self.get_component_type(), rhs.get_component_type())
            }
            SourcesSortFieldName::OutputType => Ord::cmp(
                &SourceOutputType::from(self.0.output_type),
                &SourceOutputType::from(rhs.0.output_type),
            ),
        }
    }
}

#[Object]
impl Source {
    /// Source name
    pub async fn name(&self) -> &str {
        self.get_name()
    }

    /// Source type
    pub async fn component_type(&self) -> &str {
        self.get_component_type()
    }

    /// Source output type
    pub async fn output_type(&self) -> SourceOutputType {
        self.get_output_type()
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
        metrics::by_component_name(&self.get_name()).into_source_metrics(&self.get_component_type())
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
                .map(|f| f.iter().all(|f| f.filter_value(source.get_name()))),
            self.component_type.as_ref().map(|f| f
                .iter()
                .all(|f| f.filter_value(source.get_component_type()))),
            self.output_type
                .as_ref()
                .map(|f| f.iter().all(|f| f.filter_value(source.get_output_type())))
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
                name: "gen1".to_string(),
                component_type: "generator".to_string(),
                output_type: DataType::Any,
            }),
            Source(Data {
                name: "gen2".to_string(),
                component_type: "generator".to_string(),
                output_type: DataType::Log,
            }),
            Source(Data {
                name: "gen3".to_string(),
                component_type: "generator".to_string(),
                output_type: DataType::Metric,
            }),
        ]
    }

    #[test]
    fn filter_output_type() {
        struct Test {
            name: &'static str,
            output_type: SourceOutputType,
        }

        let tests = vec![
            Test {
                name: "gen1",
                output_type: SourceOutputType::Any,
            },
            Test {
                name: "gen2",
                output_type: SourceOutputType::Log,
            },
            Test {
                name: "gen3",
                output_type: SourceOutputType::Metric,
            },
        ];

        for t in tests {
            let filter = SourcesFilter {
                name: Some(vec![filter::StringFilter {
                    equals: Some(t.name.to_string()),
                    ..Default::default()
                }]),
                output_type: Some(vec![filter::EqualityFilter {
                    equals: Some(t.output_type),
                    not_equals: None,
                }]),
                ..Default::default()
            };

            let sources = filter::filter_items(source_fixtures().into_iter(), &filter);
            assert_eq!(sources.len(), 1);
        }
    }

    #[test]
    fn sort_name_desc() {
        let mut sources = source_fixtures();
        let fields = vec![sort::SortField::<SourcesSortFieldName> {
            field: SourcesSortFieldName::Name,
            direction: sort::Direction::Desc,
        }];
        sort::by_fields(&mut sources, &fields);

        for (i, name) in ["gen3", "gen2", "gen1"].iter().enumerate() {
            assert_eq!(sources[i].get_name(), *name);
        }
    }

    #[test]
    fn sort_component_type_asc() {
        let mut sources = vec![
            Source(Data {
                name: "gen1".to_string(),
                component_type: "file".to_string(),
                output_type: DataType::Any,
            }),
            Source(Data {
                name: "gen2".to_string(),
                component_type: "generator".to_string(),
                output_type: DataType::Log,
            }),
            Source(Data {
                name: "gen3".to_string(),
                component_type: "docker_logs".to_string(),
                output_type: DataType::Metric,
            }),
        ];

        let fields = vec![sort::SortField::<SourcesSortFieldName> {
            field: SourcesSortFieldName::ComponentType,
            direction: sort::Direction::Asc,
        }];
        sort::by_fields(&mut sources, &fields);

        for (i, name) in ["gen3", "gen1", "gen2"].iter().enumerate() {
            assert_eq!(sources[i].get_name(), *name);
        }
    }

    #[test]
    fn sort_component_type_desc() {
        let mut sources = vec![
            Source(Data {
                name: "gen1".to_string(),
                component_type: "file".to_string(),
                output_type: DataType::Any,
            }),
            Source(Data {
                name: "gen2".to_string(),
                component_type: "generator".to_string(),
                output_type: DataType::Log,
            }),
            Source(Data {
                name: "gen3".to_string(),
                component_type: "docker_logs".to_string(),
                output_type: DataType::Metric,
            }),
        ];

        let fields = vec![sort::SortField::<SourcesSortFieldName> {
            field: SourcesSortFieldName::ComponentType,
            direction: sort::Direction::Desc,
        }];
        sort::by_fields(&mut sources, &fields);

        for (i, name) in ["gen2", "gen1", "gen3"].iter().enumerate() {
            assert_eq!(sources[i].get_name(), *name);
        }
    }

    #[test]
    fn sort_output_type_asc() {
        let mut sources = vec![
            Source(Data {
                name: "gen1".to_string(),
                component_type: "file".to_string(),
                output_type: DataType::Any,
            }),
            Source(Data {
                name: "gen2".to_string(),
                component_type: "generator".to_string(),
                output_type: DataType::Log,
            }),
            Source(Data {
                name: "gen3".to_string(),
                component_type: "docker_logs".to_string(),
                output_type: DataType::Metric,
            }),
        ];

        let fields = vec![sort::SortField::<SourcesSortFieldName> {
            field: SourcesSortFieldName::OutputType,
            direction: sort::Direction::Asc,
        }];
        sort::by_fields(&mut sources, &fields);

        for (i, name) in ["gen1", "gen2", "gen3"].iter().enumerate() {
            assert_eq!(sources[i].get_name(), *name);
        }
    }

    #[test]
    fn sort_output_type_desc() {
        let mut sources = vec![
            Source(Data {
                name: "gen1".to_string(),
                component_type: "file".to_string(),
                output_type: DataType::Any,
            }),
            Source(Data {
                name: "gen2".to_string(),
                component_type: "generator".to_string(),
                output_type: DataType::Log,
            }),
            Source(Data {
                name: "gen3".to_string(),
                component_type: "docker_logs".to_string(),
                output_type: DataType::Metric,
            }),
        ];

        let fields = vec![sort::SortField::<SourcesSortFieldName> {
            field: SourcesSortFieldName::OutputType,
            direction: sort::Direction::Desc,
        }];
        sort::by_fields(&mut sources, &fields);

        for (i, name) in ["gen3", "gen2", "gen1"].iter().enumerate() {
            assert_eq!(sources[i].get_name(), *name);
        }
    }
}
