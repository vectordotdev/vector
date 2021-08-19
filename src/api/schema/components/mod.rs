pub mod sink;
pub mod source;
pub mod state;
pub mod transform;

use crate::{
    api::schema::{
        components::state::component_by_component_id,
        filter::{self, filter_items},
        relay, sort,
    },
    config::ComponentId,
    config::Config,
    filter_check,
};
use async_graphql::{Enum, InputObject, Interface, Object, Subscription};
use lazy_static::lazy_static;
use std::{
    cmp,
    collections::{HashMap, HashSet},
};
use tokio_stream::{wrappers::BroadcastStream, Stream, StreamExt};

#[derive(Debug, Clone, Interface)]
#[graphql(
    field(name = "component_id", type = "String"),
    field(name = "component_type", type = "String")
)]
pub enum Component {
    Source(source::Source),
    Transform(transform::Transform),
    Sink(sink::Sink),
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum ComponentKind {
    Source,
    Transform,
    Sink,
}

impl Component {
    fn get_component_id(&self) -> &ComponentId {
        match self {
            Component::Source(c) => &c.0.component_id,
            Component::Transform(c) => &c.0.component_id,
            Component::Sink(c) => &c.0.component_id,
        }
    }

    fn get_component_kind(&self) -> ComponentKind {
        match self {
            Component::Source(_) => ComponentKind::Source,
            Component::Transform(_) => ComponentKind::Transform,
            Component::Sink(_) => ComponentKind::Sink,
        }
    }
}

#[derive(Default, InputObject)]
pub struct ComponentsFilter {
    component_id: Option<Vec<filter::StringFilter>>,
    component_kind: Option<Vec<filter::EqualityFilter<ComponentKind>>>,
    or: Option<Vec<Self>>,
}

impl filter::CustomFilter<Component> for ComponentsFilter {
    fn matches(&self, component: &Component) -> bool {
        filter_check!(
            self.component_id.as_ref().map(|f| f
                .iter()
                .all(|f| f.filter_value(component.get_component_id().as_str()))),
            self.component_kind.as_ref().map(|f| f
                .iter()
                .all(|f| f.filter_value(component.get_component_kind())))
        );
        true
    }

    fn or(&self) -> Option<&Vec<Self>> {
        self.or.as_ref()
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum ComponentsSortFieldName {
    ComponentId,
    ComponentKind,
}

impl sort::SortableByField<ComponentsSortFieldName> for Component {
    fn sort(&self, rhs: &Self, field: &ComponentsSortFieldName) -> cmp::Ordering {
        match field {
            ComponentsSortFieldName::ComponentId => {
                Ord::cmp(&self.get_component_id(), &rhs.get_component_id())
            }
            ComponentsSortFieldName::ComponentKind => {
                Ord::cmp(&self.get_component_kind(), &rhs.get_component_kind())
            }
        }
    }
}

#[derive(Default)]
pub struct ComponentsQuery;

#[Object]
impl ComponentsQuery {
    /// Configured components (sources/transforms/sinks)
    async fn components(
        &self,
        after: Option<String>,
        before: Option<String>,
        first: Option<i32>,
        last: Option<i32>,
        filter: Option<ComponentsFilter>,
        sort: Option<Vec<sort::SortField<ComponentsSortFieldName>>>,
    ) -> relay::ConnectionResult<Component> {
        let filter = filter.unwrap_or_else(ComponentsFilter::default);
        let mut components = filter_items(state::get_components().into_iter(), &filter);

        if let Some(sort_fields) = sort {
            sort::by_fields(&mut components, &sort_fields);
        }

        relay::query(
            components.into_iter(),
            relay::Params::new(after, before, first, last),
            10,
        )
        .await
    }

    /// Configured sources
    async fn sources(
        &self,
        after: Option<String>,
        before: Option<String>,
        first: Option<i32>,
        last: Option<i32>,
        filter: Option<source::SourcesFilter>,
        sort: Option<Vec<sort::SortField<source::SourcesSortFieldName>>>,
    ) -> relay::ConnectionResult<source::Source> {
        let filter = filter.unwrap_or_else(source::SourcesFilter::default);
        let mut sources = filter_items(state::get_sources().into_iter(), &filter);

        if let Some(sort_fields) = sort {
            sort::by_fields(&mut sources, &sort_fields);
        }

        relay::query(
            sources.into_iter(),
            relay::Params::new(after, before, first, last),
            10,
        )
        .await
    }

    /// Configured transforms
    async fn transforms(
        &self,
        after: Option<String>,
        before: Option<String>,
        first: Option<i32>,
        last: Option<i32>,
        filter: Option<transform::TransformsFilter>,
        sort: Option<Vec<sort::SortField<transform::TransformsSortFieldName>>>,
    ) -> relay::ConnectionResult<transform::Transform> {
        let filter = filter.unwrap_or_else(transform::TransformsFilter::default);
        let mut transforms = filter_items(state::get_transforms().into_iter(), &filter);

        if let Some(sort_fields) = sort {
            sort::by_fields(&mut transforms, &sort_fields);
        }

        relay::query(
            transforms.into_iter(),
            relay::Params::new(after, before, first, last),
            10,
        )
        .await
    }

    /// Configured sinks
    async fn sinks(
        &self,
        after: Option<String>,
        before: Option<String>,
        first: Option<i32>,
        last: Option<i32>,
        filter: Option<sink::SinksFilter>,
        sort: Option<Vec<sort::SortField<sink::SinksSortFieldName>>>,
    ) -> relay::ConnectionResult<sink::Sink> {
        let filter = filter.unwrap_or_else(sink::SinksFilter::default);
        let mut sinks = filter_items(state::get_sinks().into_iter(), &filter);

        if let Some(sort_fields) = sort {
            sort::by_fields(&mut sinks, &sort_fields);
        }

        relay::query(
            sinks.into_iter(),
            relay::Params::new(after, before, first, last),
            10,
        )
        .await
    }

    /// Gets a configured component by component_id
    async fn component_by_component_id(&self, component_id: String) -> Option<Component> {
        let id = ComponentId::from(component_id);
        component_by_component_id(&id)
    }
}

#[derive(Clone, Debug)]
enum ComponentChanged {
    Added(Component),
    Removed(Component),
}

lazy_static! {
    static ref COMPONENT_CHANGED: tokio::sync::broadcast::Sender<ComponentChanged> = {
        let (tx, _) = tokio::sync::broadcast::channel(10);
        tx
    };
}

#[derive(Debug, Default)]
pub struct ComponentsSubscription;

#[Subscription]
impl ComponentsSubscription {
    /// Subscribes to all newly added components
    async fn component_added(&self) -> impl Stream<Item = Component> {
        BroadcastStream::new(COMPONENT_CHANGED.subscribe()).filter_map(|c| match c {
            Ok(ComponentChanged::Added(c)) => Some(c),
            _ => None,
        })
    }

    /// Subscribes to all removed components
    async fn component_removed(&self) -> impl Stream<Item = Component> {
        BroadcastStream::new(COMPONENT_CHANGED.subscribe()).filter_map(|c| match c {
            Ok(ComponentChanged::Removed(c)) => Some(c),
            _ => None,
        })
    }
}

/// Update the 'global' configuration that will be consumed by component queries
pub fn update_config(config: &Config) {
    let mut new_components = HashMap::new();

    // Sources
    for (component_id, source) in config.sources.iter() {
        new_components.insert(
            component_id.clone(),
            Component::Source(source::Source(source::Data {
                component_id: component_id.clone(),
                component_type: source.inner.source_type().to_string(),
                output_type: source.inner.output_type(),
            })),
        );
    }

    // Transforms
    for (component_id, transform) in config.transforms.iter() {
        new_components.insert(
            component_id.clone(),
            Component::Transform(transform::Transform(transform::Data {
                component_id: component_id.clone(),
                component_type: transform.inner.transform_type().to_string(),
                inputs: transform.inputs.clone(),
            })),
        );
    }

    // Sinks
    for (component_id, sink) in config.sinks.iter() {
        new_components.insert(
            component_id.clone(),
            Component::Sink(sink::Sink(sink::Data {
                component_id: component_id.clone(),
                component_type: sink.inner.sink_type().to_string(),
                inputs: sink.inputs.clone(),
            })),
        );
    }

    // Get the component_ids of existing components
    let existing_component_ids = state::get_component_ids();
    let new_component_ids = new_components
        .iter()
        .map(|(component_id, _)| component_id.clone())
        .collect::<HashSet<ComponentId>>();

    // Publish all components that have been removed
    existing_component_ids
        .difference(&new_component_ids)
        .for_each(|component_id| {
            let _ = COMPONENT_CHANGED.send(ComponentChanged::Removed(
                state::component_by_component_id(component_id)
                    .expect("Couldn't get component by id"),
            ));
        });

    // Publish all components that have been added
    new_component_ids
        .difference(&existing_component_ids)
        .for_each(|component_id| {
            let _ = COMPONENT_CHANGED.send(ComponentChanged::Added(
                new_components.get(component_id).unwrap().clone(),
            ));
        });

    // Override the old component state
    state::update(new_components);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        api::schema::sort,
        config::{ComponentId, DataType},
    };

    /// Generate component fixes for use with tests
    fn component_fixtures() -> Vec<Component> {
        vec![
            Component::Source(source::Source(source::Data {
                component_id: ComponentId::from("gen1"),
                component_type: "generator".to_string(),
                output_type: DataType::Metric,
            })),
            Component::Source(source::Source(source::Data {
                component_id: ComponentId::from("gen2"),
                component_type: "generator".to_string(),
                output_type: DataType::Metric,
            })),
            Component::Source(source::Source(source::Data {
                component_id: ComponentId::from("gen3"),
                component_type: "generator".to_string(),
                output_type: DataType::Metric,
            })),
            Component::Transform(transform::Transform(transform::Data {
                component_id: ComponentId::from("parse_json"),
                component_type: "json".to_string(),
                inputs: vec![ComponentId::from("gen1"), ComponentId::from("gen2")],
            })),
            Component::Sink(sink::Sink(sink::Data {
                component_id: ComponentId::from("devnull"),
                component_type: "blackhole".to_string(),
                inputs: vec![ComponentId::from("gen3"), ComponentId::from("parse_json")],
            })),
        ]
    }

    #[test]
    fn components_filter_contains() {
        let filter = ComponentsFilter {
            component_id: Some(vec![filter::StringFilter {
                contains: Some("gen".to_string()),
                ..Default::default()
            }]),
            ..Default::default()
        };

        let components = filter_items(component_fixtures().into_iter(), &filter);

        assert_eq!(components.len(), 3);
    }

    #[test]
    fn components_filter_equals_or() {
        let filter = ComponentsFilter {
            component_id: Some(vec![filter::StringFilter {
                equals: Some("gen1".to_string()),
                ..Default::default()
            }]),
            or: Some(vec![ComponentsFilter {
                component_id: Some(vec![filter::StringFilter {
                    equals: Some("devnull".to_string()),
                    ..Default::default()
                }]),
                ..Default::default()
            }]),
            ..Default::default()
        };

        let components = filter_items(component_fixtures().into_iter(), &filter);

        assert_eq!(components.len(), 2);
    }

    #[test]
    fn components_filter_and() {
        let filter = ComponentsFilter {
            component_id: Some(vec![filter::StringFilter {
                equals: Some("gen1".to_string()),
                ..Default::default()
            }]),
            component_kind: Some(vec![filter::EqualityFilter {
                equals: Some(ComponentKind::Source),
                not_equals: None,
            }]),
            ..Default::default()
        };

        let components = filter_items(component_fixtures().into_iter(), &filter);

        assert_eq!(components.len(), 1);
    }

    #[test]
    fn components_filter_and_or() {
        let filter = ComponentsFilter {
            component_id: Some(vec![filter::StringFilter {
                equals: Some("gen1".to_string()),
                ..Default::default()
            }]),
            component_kind: Some(vec![filter::EqualityFilter {
                equals: Some(ComponentKind::Source),
                not_equals: None,
            }]),
            or: Some(vec![ComponentsFilter {
                component_kind: Some(vec![filter::EqualityFilter {
                    equals: Some(ComponentKind::Sink),
                    not_equals: None,
                }]),
                ..Default::default()
            }]),
        };

        let components = filter_items(component_fixtures().into_iter(), &filter);

        assert_eq!(components.len(), 2);
    }

    #[test]
    fn components_sort_asc() {
        let mut components = component_fixtures();
        let fields = vec![sort::SortField::<ComponentsSortFieldName> {
            field: ComponentsSortFieldName::ComponentId,
            direction: sort::Direction::Asc,
        }];
        sort::by_fields(&mut components, &fields);

        let expectations = ["devnull", "gen1", "gen2", "gen3", "parse_json"];

        for (i, component_id) in expectations.iter().enumerate() {
            assert_eq!(components[i].get_component_id().as_str(), *component_id);
        }
    }

    #[test]
    fn components_sort_desc() {
        let mut components = component_fixtures();
        let fields = vec![sort::SortField::<ComponentsSortFieldName> {
            field: ComponentsSortFieldName::ComponentId,
            direction: sort::Direction::Desc,
        }];
        sort::by_fields(&mut components, &fields);

        let expectations = ["parse_json", "gen3", "gen2", "gen1", "devnull"];

        for (i, component_id) in expectations.iter().enumerate() {
            assert_eq!(components[i].get_component_id().as_str(), *component_id);
        }
    }

    #[test]
    fn components_sort_multi() {
        let mut components = vec![
            Component::Sink(sink::Sink(sink::Data {
                component_id: ComponentId::from("a"),
                component_type: "blackhole".to_string(),
                inputs: vec![ComponentId::from("gen3"), ComponentId::from("parse_json")],
            })),
            Component::Sink(sink::Sink(sink::Data {
                component_id: ComponentId::from("b"),
                component_type: "blackhole".to_string(),
                inputs: vec![ComponentId::from("gen3"), ComponentId::from("parse_json")],
            })),
            Component::Transform(transform::Transform(transform::Data {
                component_id: ComponentId::from("c"),
                component_type: "json".to_string(),
                inputs: vec![ComponentId::from("gen1"), ComponentId::from("gen2")],
            })),
            Component::Source(source::Source(source::Data {
                component_id: ComponentId::from("e"),
                component_type: "generator".to_string(),
                output_type: DataType::Metric,
            })),
            Component::Source(source::Source(source::Data {
                component_id: ComponentId::from("d"),
                component_type: "generator".to_string(),
                output_type: DataType::Metric,
            })),
            Component::Source(source::Source(source::Data {
                component_id: ComponentId::from("g"),
                component_type: "generator".to_string(),
                output_type: DataType::Metric,
            })),
            Component::Source(source::Source(source::Data {
                component_id: ComponentId::from("f"),
                component_type: "generator".to_string(),
                output_type: DataType::Metric,
            })),
        ];

        let fields = vec![
            sort::SortField::<ComponentsSortFieldName> {
                field: ComponentsSortFieldName::ComponentKind,
                direction: sort::Direction::Asc,
            },
            sort::SortField::<ComponentsSortFieldName> {
                field: ComponentsSortFieldName::ComponentId,
                direction: sort::Direction::Asc,
            },
        ];
        sort::by_fields(&mut components, &fields);

        let expectations = ["d", "e", "f", "g", "c", "a", "b"];
        for (i, component_id) in expectations.iter().enumerate() {
            assert_eq!(components[i].get_component_id().as_str(), *component_id);
        }
    }
}
