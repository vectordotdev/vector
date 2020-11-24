use super::metrics;
use crate::config::{Config, DataType};
use async_graphql::{Enum, Interface, Object, Subscription};
use lazy_static::lazy_static;
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, RwLock},
};
use tokio::stream::{Stream, StreamExt};

pub const INVARIANT: &str = "Couldn't acquire lock on Vector components. Please report this.";

lazy_static! {
    pub static ref COMPONENTS: Arc<RwLock<HashMap<String, Component>>> =
        Arc::new(RwLock::new(HashMap::new()));
}

#[derive(Debug, Clone, Interface)]
#[graphql(
    field(name = "name", type = "String"),
    field(name = "component_type", type = "String"),
    field(
        name = "processed_events_total",
        type = "Option<metrics::ProcessedEventsTotal>"
    ),
    field(
        name = "processed_bytes_total",
        type = "Option<metrics::ProcessedBytesTotal>"
    )
)]
pub enum Component {
    Source(Source),
    Transform(Transform),
    Sink(Sink),
}

#[derive(Enum, Eq, PartialEq, Copy, Clone)]
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
pub struct SourceData {
    name: String,
    component_type: String,
    output_type: DataType,
}

#[derive(Debug, Clone)]
pub struct Source(SourceData);

#[Object]
impl Source {
    /// Source name
    async fn name(&self) -> &str {
        &*self.0.name
    }

    /// Source type
    async fn component_type(&self) -> &str {
        &*self.0.component_type
    }

    /// Source output type
    async fn output_type(&self) -> SourceOutputType {
        self.0.output_type.into()
    }

    /// Transform outputs
    async fn transforms(&self) -> Vec<Transform> {
        filter_components(|(_name, components)| match components {
            Component::Transform(t) if t.0.inputs.contains(&self.0.name) => Some(t.clone()),
            _ => None,
        })
    }

    /// Sink outputs
    async fn sinks(&self) -> Vec<Sink> {
        filter_components(|(_name, components)| match components {
            Component::Sink(s) if s.0.inputs.contains(&self.0.name) => Some(s.clone()),
            _ => None,
        })
    }

    /// Metric indicating events processed for the current source
    async fn processed_events_total(&self) -> Option<metrics::ProcessedEventsTotal> {
        metrics::component_processed_events_total(&self.0.name)
    }

    /// Metric indicating bytes processed for the current source
    async fn processed_bytes_total(&self) -> Option<metrics::ProcessedBytesTotal> {
        metrics::component_processed_bytes_total(&self.0.name)
    }
}

#[derive(Debug, Clone)]
pub struct TransformData {
    name: String,
    component_type: String,
    inputs: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Transform(TransformData);

#[Object]
impl Transform {
    /// Transform name
    async fn name(&self) -> &str {
        &self.0.name
    }

    /// Transform type
    async fn component_type(&self) -> &str {
        &*self.0.component_type
    }

    /// Source inputs
    async fn sources(&self) -> Vec<Source> {
        self.0
            .inputs
            .iter()
            .filter_map(|name| match COMPONENTS.read().expect(INVARIANT).get(name) {
                Some(t) => match t {
                    Component::Source(s) => Some(s.clone()),
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }

    /// Transform outputs
    async fn transforms(&self) -> Vec<Transform> {
        filter_components(|(_name, components)| match components {
            Component::Transform(t) if t.0.inputs.contains(&self.0.name) => Some(t.clone()),
            _ => None,
        })
    }

    /// Sink outputs
    async fn sinks(&self) -> Vec<Sink> {
        filter_components(|(_name, components)| match components {
            Component::Sink(s) if s.0.inputs.contains(&self.0.name) => Some(s.clone()),
            _ => None,
        })
    }

    /// Metric indicating events processed for the current transform
    async fn processed_events_total(&self) -> Option<metrics::ProcessedEventsTotal> {
        metrics::component_processed_events_total(&self.0.name)
    }

    /// Metric indicating bytes processed for the current transform
    async fn processed_bytes_total(&self) -> Option<metrics::ProcessedBytesTotal> {
        metrics::component_processed_bytes_total(&self.0.name)
    }
}

#[derive(Debug, Clone)]
pub struct SinkData {
    name: String,
    component_type: String,
    inputs: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Sink(SinkData);

#[Object]
impl Sink {
    /// Sink name
    async fn name(&self) -> &str {
        &self.0.name
    }

    /// Sink type
    async fn component_type(&self) -> &str {
        &*self.0.component_type
    }

    /// Source inputs
    async fn sources(&self) -> Vec<Source> {
        self.0
            .inputs
            .iter()
            .filter_map(|name| match COMPONENTS.read().expect(INVARIANT).get(name) {
                Some(components) => match components {
                    Component::Source(s) => Some(s.clone()),
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }

    /// Transform inputs
    async fn transforms(&self) -> Vec<Transform> {
        self.0
            .inputs
            .iter()
            .filter_map(|name| match COMPONENTS.read().expect(INVARIANT).get(name) {
                Some(components) => match components {
                    Component::Transform(t) => Some(t.clone()),
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }

    /// Metric indicating events processed for the current sink
    async fn processed_events_total(&self) -> Option<metrics::ProcessedEventsTotal> {
        metrics::component_processed_events_total(&self.0.name)
    }

    /// Metric indicating bytes processed for the current sink
    async fn processed_bytes_total(&self) -> Option<metrics::ProcessedBytesTotal> {
        metrics::component_processed_bytes_total(&self.0.name)
    }
}
#[derive(Default)]
pub struct ComponentsQuery;

#[Object]
impl ComponentsQuery {
    /// Configured components (sources/transforms/sinks)
    async fn components(&self) -> Vec<Component> {
        filter_components(|(_name, components)| Some(components.clone()))
    }

    /// Configured sources
    async fn sources(&self) -> Vec<Source> {
        get_sources()
    }

    /// Configured transforms
    async fn transforms(&self) -> Vec<Transform> {
        get_transforms()
    }

    /// Configured sinks
    async fn sinks(&self) -> Vec<Sink> {
        get_sinks()
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
        COMPONENT_CHANGED
            .subscribe()
            .into_stream()
            .filter_map(|c| match c {
                Ok(ComponentChanged::Added(c)) => Some(c),
                _ => None,
            })
    }

    /// Subscribes to all removed components
    async fn component_removed(&self) -> impl Stream<Item = Component> {
        COMPONENT_CHANGED
            .subscribe()
            .into_stream()
            .filter_map(|c| match c {
                Ok(ComponentChanged::Removed(c)) => Some(c),
                _ => None,
            })
    }
}

/// Filter components with the provided `map_func`
fn filter_components<T>(map_func: impl Fn((&String, &Component)) -> Option<T>) -> Vec<T> {
    COMPONENTS
        .read()
        .expect(INVARIANT)
        .iter()
        .filter_map(map_func)
        .collect()
}

/// Filters components, and returns a clone of sources
fn get_sources() -> Vec<Source> {
    filter_components(|(_, components)| match components {
        Component::Source(s) => Some(s.clone()),
        _ => None,
    })
}

/// Filters components, and returns a clone of transforms
fn get_transforms() -> Vec<Transform> {
    filter_components(|(_, components)| match components {
        Component::Transform(t) => Some(t.clone()),
        _ => None,
    })
}

/// Filters components, and returns a clone of sinks
fn get_sinks() -> Vec<Sink> {
    filter_components(|(_, components)| match components {
        Component::Sink(s) => Some(s.clone()),
        _ => None,
    })
}

/// Returns the current component names as a HashSet
fn get_component_names() -> HashSet<String> {
    COMPONENTS
        .read()
        .expect(INVARIANT)
        .keys()
        .cloned()
        .collect::<HashSet<String>>()
}

/// Update the 'global' configuration that will be consumed by component queries
pub fn update_config(config: &Config) {
    let mut new_components = HashMap::new();

    // Sources
    for (name, source) in config.sources.iter() {
        new_components.insert(
            name.to_owned(),
            Component::Source(Source(SourceData {
                name: name.to_owned(),
                component_type: source.source_type().to_string(),
                output_type: source.output_type(),
            })),
        );
    }

    // Transforms
    for (name, transform) in config.transforms.iter() {
        new_components.insert(
            name.to_string(),
            Component::Transform(Transform(TransformData {
                name: name.to_owned(),
                component_type: transform.inner.transform_type().to_string(),
                inputs: transform.inputs.clone(),
            })),
        );
    }

    // Sinks
    for (name, sink) in config.sinks.iter() {
        new_components.insert(
            name.to_string(),
            Component::Sink(Sink(SinkData {
                name: name.to_owned(),
                component_type: sink.inner.sink_type().to_string(),
                inputs: sink.inputs.clone(),
            })),
        );
    }

    // Get the names of existing components
    let existing_component_names = get_component_names();
    let new_component_names = new_components
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<HashSet<String>>();

    // Publish all components that have been removed
    existing_component_names
        .difference(&new_component_names)
        .for_each(|name| {
            let _ = COMPONENT_CHANGED.send(ComponentChanged::Removed(
                COMPONENTS
                    .read()
                    .expect(INVARIANT)
                    .get(name)
                    .expect(INVARIANT)
                    .clone(),
            ));
        });

    // Publish all components that have been added
    new_component_names
        .difference(&existing_component_names)
        .for_each(|name| {
            let _ = COMPONENT_CHANGED.send(ComponentChanged::Added(
                new_components.get(name).unwrap().clone(),
            ));
        });

    // Override the old hashmap
    *COMPONENTS.write().expect(INVARIANT) = new_components;
}
