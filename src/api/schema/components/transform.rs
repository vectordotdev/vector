use std::cmp;

use async_graphql::{Enum, InputObject, Object};

use super::{sink, source, state, Component};
use crate::{
    api::schema::{
        filter,
        metrics::{self, outputs_by_component_key, IntoTransformMetrics, Output},
        sort,
    },
    config::{ComponentKey, OutputId},
    filter_check,
};

#[derive(Debug, Clone)]
pub struct Data {
    pub component_key: ComponentKey,
    pub component_type: String,
    pub inputs: Vec<OutputId>,
    pub outputs: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Transform(pub Data);

impl Transform {
    pub const fn get_component_key(&self) -> &ComponentKey {
        &self.0.component_key
    }
    pub fn get_component_type(&self) -> &str {
        self.0.component_type.as_str()
    }
    pub fn get_outputs(&self) -> &[String] {
        self.0.outputs.as_ref()
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum TransformsSortFieldName {
    ComponentKey,
    ComponentType,
}

impl sort::SortableByField<TransformsSortFieldName> for Transform {
    fn sort(&self, rhs: &Self, field: &TransformsSortFieldName) -> cmp::Ordering {
        match field {
            TransformsSortFieldName::ComponentKey => {
                Ord::cmp(self.get_component_key(), rhs.get_component_key())
            }
            TransformsSortFieldName::ComponentType => {
                Ord::cmp(self.get_component_type(), rhs.get_component_type())
            }
        }
    }
}

#[Object]
impl Transform {
    /// Transform component_id
    pub async fn component_id(&self) -> &str {
        self.0.component_key.id()
    }

    /// Transform type
    pub async fn component_type(&self) -> &str {
        &*self.get_component_type()
    }

    /// Transform output streams
    pub async fn outputs(&self) -> Vec<Output> {
        outputs_by_component_key(self.get_component_key(), self.get_outputs())
    }

    /// Source inputs
    pub async fn sources(&self) -> Vec<source::Source> {
        self.0
            .inputs
            .iter()
            .filter_map(|output_id| match state::component_by_output_id(output_id) {
                Some(Component::Source(s)) => Some(s),
                _ => None,
            })
            .collect()
    }

    /// Transform outputs
    pub async fn transforms(&self) -> Vec<Transform> {
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

    /// Transform metrics
    pub async fn metrics(&self) -> metrics::TransformMetrics {
        metrics::by_component_key(&self.0.component_key)
            .into_transform_metrics(self.get_component_type())
    }
}

#[derive(Default, InputObject)]
pub struct TransformsFilter {
    component_id: Option<Vec<filter::StringFilter>>,
    component_type: Option<Vec<filter::StringFilter>>,
    or: Option<Vec<Self>>,
}

impl filter::CustomFilter<Transform> for TransformsFilter {
    fn matches(&self, transform: &Transform) -> bool {
        filter_check!(
            self.component_id.as_ref().map(|f| f
                .iter()
                .all(|f| f.filter_value(&transform.get_component_key().to_string()))),
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
                component_key: ComponentKey::from("parse_json"),
                component_type: "json".to_string(),
                inputs: vec![],
                outputs: vec![],
            }),
            Transform(Data {
                component_key: ComponentKey::from("field_adder"),
                component_type: "add_fields".to_string(),
                inputs: vec![],
                outputs: vec![],
            }),
            Transform(Data {
                component_key: ComponentKey::from("append"),
                component_type: "concat".to_string(),
                inputs: vec![],
                outputs: vec![],
            }),
        ]
    }

    #[test]
    fn sort_component_id_asc() {
        let mut transforms = transform_fixtures();
        let fields = vec![sort::SortField::<TransformsSortFieldName> {
            field: TransformsSortFieldName::ComponentKey,
            direction: sort::Direction::Asc,
        }];
        sort::by_fields(&mut transforms, &fields);

        for (i, component_id) in ["append", "field_adder", "parse_json"].iter().enumerate() {
            assert_eq!(transforms[i].get_component_key().to_string(), *component_id);
        }
    }

    #[test]
    fn sort_component_id_desc() {
        let mut transforms = transform_fixtures();
        let fields = vec![sort::SortField::<TransformsSortFieldName> {
            field: TransformsSortFieldName::ComponentKey,
            direction: sort::Direction::Desc,
        }];
        sort::by_fields(&mut transforms, &fields);

        for (i, component_id) in ["parse_json", "field_adder", "append"].iter().enumerate() {
            assert_eq!(transforms[i].get_component_key().to_string(), *component_id);
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

        for (i, component_id) in ["field_adder", "append", "parse_json"].iter().enumerate() {
            assert_eq!(transforms[i].get_component_key().to_string(), *component_id);
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

        for (i, component_id) in ["parse_json", "append", "field_adder"].iter().enumerate() {
            assert_eq!(transforms[i].get_component_key().to_string(), *component_id);
        }
    }
}
