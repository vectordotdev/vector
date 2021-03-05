use super::{sink, source, state, Component};
use crate::{
    api::schema::{
        filter,
        metrics::{self, IntoTransformMetrics},
        sort,
    },
    filter_check,
};
use async_graphql::{Enum, InputObject, Object};
use std::cmp;

#[derive(Debug, Clone)]
pub struct Data {
    pub name: String,
    pub component_type: String,
    pub inputs: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Transform(pub Data);

impl Transform {
    pub fn get_name(&self) -> &str {
        self.0.name.as_str()
    }
    pub fn get_component_type(&self) -> &str {
        self.0.component_type.as_str()
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum TransformsSortFieldName {
    Name,
    ComponentType,
}

impl sort::SortableByField<TransformsSortFieldName> for Transform {
    fn sort(&self, rhs: &Self, field: &TransformsSortFieldName) -> cmp::Ordering {
        match field {
            TransformsSortFieldName::Name => Ord::cmp(self.get_name(), rhs.get_name()),
            TransformsSortFieldName::ComponentType => {
                Ord::cmp(self.get_component_type(), rhs.get_component_type())
            }
        }
    }
}

#[Object]
impl Transform {
    /// Transform name
    pub async fn name(&self) -> &str {
        &self.get_name()
    }

    /// Transform type
    pub async fn component_type(&self) -> &str {
        &*self.get_component_type()
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
        metrics::by_component_name(&self.0.name).into_transform_metrics(&self.get_component_type())
    }
}

#[derive(Default, InputObject)]
pub struct TransformsFilter {
    name: Option<Vec<filter::StringFilter>>,
    component_type: Option<Vec<filter::StringFilter>>,
    or: Option<Vec<Self>>,
}

impl filter::CustomFilter<Transform> for TransformsFilter {
    fn matches(&self, transform: &Transform) -> bool {
        filter_check!(
            self.name
                .as_ref()
                .map(|f| f.iter().all(|f| f.filter_value(transform.get_name()))),
            self.component_type.as_ref().map(|f| f
                .iter()
                .all(|f| f.filter_value(transform.get_component_type())))
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

    fn transform_fixtures() -> Vec<Transform> {
        vec![
            Transform(Data {
                name: "parse_json".to_string(),
                component_type: "json".to_string(),
                inputs: vec![],
            }),
            Transform(Data {
                name: "field_adder".to_string(),
                component_type: "add_fields".to_string(),
                inputs: vec![],
            }),
            Transform(Data {
                name: "append".to_string(),
                component_type: "concat".to_string(),
                inputs: vec![],
            }),
        ]
    }

    #[test]
    fn sort_name_asc() {
        let mut transforms = transform_fixtures();
        let fields = vec![sort::SortField::<TransformsSortFieldName> {
            field: TransformsSortFieldName::Name,
            direction: sort::Direction::Asc,
        }];
        sort::by_fields(&mut transforms, &fields);

        for (i, name) in ["append", "field_adder", "parse_json"].iter().enumerate() {
            assert_eq!(transforms[i].get_name(), *name);
        }
    }

    #[test]
    fn sort_name_desc() {
        let mut transforms = transform_fixtures();
        let fields = vec![sort::SortField::<TransformsSortFieldName> {
            field: TransformsSortFieldName::Name,
            direction: sort::Direction::Desc,
        }];
        sort::by_fields(&mut transforms, &fields);

        for (i, name) in ["parse_json", "field_adder", "append"].iter().enumerate() {
            assert_eq!(transforms[i].get_name(), *name);
        }
    }

    #[test]
    fn sort_component_type_asc() {
        let mut transforms = transform_fixtures();
        let fields = vec![sort::SortField::<TransformsSortFieldName> {
            field: TransformsSortFieldName::ComponentType,
            direction: sort::Direction::Asc,
        }];
        sort::by_fields(&mut transforms, &fields);

        for (i, name) in ["field_adder", "append", "parse_json"].iter().enumerate() {
            assert_eq!(transforms[i].get_name(), *name);
        }
    }

    #[test]
    fn sort_component_type_desc() {
        let mut transforms = transform_fixtures();
        let fields = vec![sort::SortField::<TransformsSortFieldName> {
            field: TransformsSortFieldName::ComponentType,
            direction: sort::Direction::Desc,
        }];
        sort::by_fields(&mut transforms, &fields);

        for (i, name) in ["parse_json", "append", "field_adder"].iter().enumerate() {
            assert_eq!(transforms[i].get_name(), *name);
        }
    }
}
