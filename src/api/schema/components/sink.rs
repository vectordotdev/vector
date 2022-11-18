use std::cmp;

use async_graphql::{Enum, InputObject, Object};

use super::{source, state, transform, Component};
use crate::{
    api::schema::{
        filter,
        metrics::{self, IntoSinkMetrics},
        sort,
    },
    config::{ComponentKey, Inputs, OutputId},
    filter_check,
};

#[derive(Debug, Clone)]
pub struct Data {
    pub component_key: ComponentKey,
    pub component_type: String,
    pub inputs: Inputs<OutputId>,
}

#[derive(Debug, Clone)]
pub struct Sink(pub Data);

impl Sink {
    pub const fn get_component_key(&self) -> &ComponentKey {
        &self.0.component_key
    }

    pub fn get_component_type(&self) -> &str {
        self.0.component_type.as_str()
    }
}

#[derive(Default, InputObject)]
pub struct SinksFilter {
    component_id: Option<Vec<filter::StringFilter>>,
    component_type: Option<Vec<filter::StringFilter>>,
    or: Option<Vec<Self>>,
}

impl filter::CustomFilter<Sink> for SinksFilter {
    fn matches(&self, sink: &Sink) -> bool {
        filter_check!(
            self.component_id.as_ref().map(|f| f
                .iter()
                .all(|f| f.filter_value(&sink.get_component_key().to_string()))),
            self.component_type
                .as_ref()
                .map(|f| f.iter().all(|f| f.filter_value(sink.get_component_type())))
        );
        true
    }

    fn or(&self) -> Option<&Vec<Self>> {
        self.or.as_ref()
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum SinksSortFieldName {
    ComponentKey,
    ComponentType,
}

impl sort::SortableByField<SinksSortFieldName> for Sink {
    fn sort(&self, rhs: &Self, field: &SinksSortFieldName) -> cmp::Ordering {
        match field {
            SinksSortFieldName::ComponentKey => {
                Ord::cmp(self.get_component_key(), rhs.get_component_key())
            }
            SinksSortFieldName::ComponentType => {
                Ord::cmp(self.get_component_type(), rhs.get_component_type())
            }
        }
    }
}

#[Object]
impl Sink {
    /// Sink component_id
    pub async fn component_id(&self) -> &str {
        self.get_component_key().id()
    }

    /// Sink type
    pub async fn component_type(&self) -> &str {
        self.get_component_type()
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

    /// Transform inputs
    pub async fn transforms(&self) -> Vec<transform::Transform> {
        self.0
            .inputs
            .iter()
            .filter_map(|output_id| match state::component_by_output_id(output_id) {
                Some(Component::Transform(t)) => Some(t),
                _ => None,
            })
            .collect()
    }

    /// Sink metrics
    pub async fn metrics(&self) -> metrics::SinkMetrics {
        metrics::by_component_key(self.get_component_key())
            .into_sink_metrics(self.get_component_type())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sink_fixtures() -> Vec<Sink> {
        vec![
            Sink(Data {
                component_key: ComponentKey::from("webserver"),
                component_type: "http".to_string(),
                inputs: Inputs::default(),
            }),
            Sink(Data {
                component_key: ComponentKey::from("db"),
                component_type: "clickhouse".to_string(),
                inputs: Inputs::default(),
            }),
            Sink(Data {
                component_key: ComponentKey::from("zip_drive"),
                component_type: "file".to_string(),
                inputs: Inputs::default(),
            }),
        ]
    }

    #[test]
    fn sort_component_id_asc() {
        let mut sinks = sink_fixtures();
        let fields = vec![sort::SortField::<SinksSortFieldName> {
            field: SinksSortFieldName::ComponentKey,
            direction: sort::Direction::Asc,
        }];
        sort::by_fields(&mut sinks, &fields);

        for (i, component_id) in ["db", "webserver", "zip_drive"].iter().enumerate() {
            assert_eq!(sinks[i].get_component_key().to_string(), *component_id);
        }
    }

    #[test]
    fn sort_component_id_desc() {
        let mut sinks = sink_fixtures();
        let fields = vec![sort::SortField::<SinksSortFieldName> {
            field: SinksSortFieldName::ComponentKey,
            direction: sort::Direction::Desc,
        }];
        sort::by_fields(&mut sinks, &fields);

        for (i, component_id) in ["zip_drive", "webserver", "db"].iter().enumerate() {
            assert_eq!(sinks[i].get_component_key().to_string(), *component_id);
        }
    }

    #[test]
    fn sort_component_type_asc() {
        let mut sinks = sink_fixtures();
        let fields = vec![sort::SortField::<SinksSortFieldName> {
            field: SinksSortFieldName::ComponentType,
            direction: sort::Direction::Asc,
        }];
        sort::by_fields(&mut sinks, &fields);

        for (i, component_id) in ["db", "zip_drive", "webserver"].iter().enumerate() {
            assert_eq!(sinks[i].get_component_key().to_string(), *component_id);
        }
    }

    #[test]
    fn sort_component_type_desc() {
        let mut sinks = sink_fixtures();
        let fields = vec![sort::SortField::<SinksSortFieldName> {
            field: SinksSortFieldName::ComponentType,
            direction: sort::Direction::Desc,
        }];
        sort::by_fields(&mut sinks, &fields);

        for (i, component_id) in ["webserver", "zip_drive", "db"].iter().enumerate() {
            assert_eq!(sinks[i].get_component_key().to_string(), *component_id);
        }
    }
}
