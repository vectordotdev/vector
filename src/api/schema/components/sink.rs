use super::{source, state, transform, Component};
use crate::{
    api::schema::{
        filter,
        metrics::{self, IntoSinkMetrics},
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
pub struct Sink(pub Data);

impl Sink {
    pub fn get_name(&self) -> &str {
        self.0.name.as_str()
    }
    pub fn get_component_type(&self) -> &str {
        self.0.component_type.as_str()
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
                .map(|f| f.iter().all(|f| f.filter_value(sink.get_name()))),
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
    Name,
    ComponentType,
}

impl sort::SortableByField<SinksSortFieldName> for Sink {
    fn sort(&self, rhs: &Self, field: &SinksSortFieldName) -> cmp::Ordering {
        match field {
            SinksSortFieldName::Name => Ord::cmp(self.get_name(), rhs.get_name()),
            SinksSortFieldName::ComponentType => {
                Ord::cmp(self.get_component_type(), rhs.get_component_type())
            }
        }
    }
}

#[Object]
impl Sink {
    /// Sink name
    pub async fn name(&self) -> &str {
        &self.get_name()
    }

    /// Sink type
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
        metrics::by_component_name(self.get_name()).into_sink_metrics(self.get_component_type())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sink_fixtures() -> Vec<Sink> {
        vec![
            Sink(Data {
                name: "webserver".to_string(),
                component_type: "http".to_string(),
                inputs: vec![],
            }),
            Sink(Data {
                name: "db".to_string(),
                component_type: "clickhouse".to_string(),
                inputs: vec![],
            }),
            Sink(Data {
                name: "zip_drive".to_string(),
                component_type: "file".to_string(),
                inputs: vec![],
            }),
        ]
    }

    #[test]
    fn sort_name_asc() {
        let mut sinks = sink_fixtures();
        let fields = vec![sort::SortField::<SinksSortFieldName> {
            field: SinksSortFieldName::Name,
            direction: sort::Direction::Asc,
        }];
        sort::by_fields(&mut sinks, &fields);

        for (i, name) in ["db", "webserver", "zip_drive"].iter().enumerate() {
            assert_eq!(sinks[i].get_name(), *name);
        }
    }

    #[test]
    fn sort_name_desc() {
        let mut sinks = sink_fixtures();
        let fields = vec![sort::SortField::<SinksSortFieldName> {
            field: SinksSortFieldName::Name,
            direction: sort::Direction::Desc,
        }];
        sort::by_fields(&mut sinks, &fields);

        for (i, name) in ["zip_drive", "webserver", "db"].iter().enumerate() {
            assert_eq!(sinks[i].get_name(), *name);
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

        for (i, name) in ["db", "zip_drive", "webserver"].iter().enumerate() {
            assert_eq!(sinks[i].get_name(), *name);
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

        for (i, name) in ["webserver", "zip_drive", "db"].iter().enumerate() {
            assert_eq!(sinks[i].get_name(), *name);
        }
    }
}
