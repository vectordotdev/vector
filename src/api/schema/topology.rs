use crate::config::{Config, DataType};
use async_graphql::{Enum, Interface, Object};
use lazy_static::lazy_static;
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

const INVARIANT: &str =
    "It is an invariant for the API to be active but not have a TOPOLOGY. Please report this.";

#[Enum]
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

#[derive(Clone)]
pub struct SourceData {
    name: String,
    output_type: DataType,
}

#[derive(Clone)]
pub struct Source(SourceData);

#[Object]
impl Source {
    /// Source name
    async fn name(&self) -> String {
        self.0.name.clone()
    }

    /// Source output type
    async fn output_type(&self) -> SourceOutputType {
        self.0.output_type.into()
    }

    /// Transform outputs
    async fn transforms(&self) -> Vec<Transform> {
        filter_topology(|(_name, topology)| match topology {
            Topology::Transform(t) if t.0.inputs.contains(&self.0.name) => Some(t.clone()),
            _ => None,
        })
    }

    /// Sink outputs
    async fn sinks(&self) -> Vec<Sink> {
        filter_topology(|(_name, topology)| match topology {
            Topology::Sink(s) if s.0.inputs.contains(&self.0.name) => Some(s.clone()),
            _ => None,
        })
    }
}

#[derive(Clone)]
pub struct InputsData {
    name: String,
    inputs: Vec<String>,
}

#[derive(Clone)]
pub struct Transform(InputsData);

#[Object]
impl Transform {
    /// Transform name
    async fn name(&self) -> String {
        self.0.name.clone()
    }

    /// Source inputs
    async fn sources(&self) -> Vec<Source> {
        self.0
            .inputs
            .iter()
            .filter_map(|name| match TOPOLOGY.read().expect(INVARIANT).get(name) {
                Some(t) => match t {
                    Topology::Source(s) => Some(s.clone()),
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }

    /// Sink outputs
    async fn sinks(&self) -> Vec<Sink> {
        filter_topology(|(_name, topology)| match topology {
            Topology::Sink(s) if s.0.inputs.contains(&self.0.name) => Some(s.clone()),
            _ => None,
        })
    }
}

#[derive(Clone)]
pub struct Sink(InputsData);

#[Object]
impl Sink {
    /// Sink name
    async fn name(&self) -> String {
        self.0.name.clone()
    }

    /// Source inputs
    async fn sources(&self) -> Vec<Source> {
        self.0
            .inputs
            .iter()
            .filter_map(|name| match TOPOLOGY.read().expect(INVARIANT).get(name) {
                Some(topology) => match topology {
                    Topology::Source(s) => Some(s.clone()),
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
            .filter_map(|name| match TOPOLOGY.read().expect(INVARIANT).get(name) {
                Some(topology) => match topology {
                    Topology::Transform(t) => Some(t.clone()),
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }
}

#[Interface(field(name = "name", type = "String"))]
#[derive(Clone)]
pub enum Topology {
    Source(Source),
    Transform(Transform),
    Sink(Sink),
}

lazy_static! {
    static ref TOPOLOGY: Arc<RwLock<HashMap<String, Topology>>> =
        Arc::new(RwLock::new(HashMap::new()));
}

#[derive(Default)]
pub struct TopologyQuery;

#[Object]
impl TopologyQuery {
    /// Configured Topology (source/transform/sink)
    async fn topology(&self) -> Vec<Topology> {
        filter_topology(|(_name, topology)| Some(topology.clone()))
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

fn filter_topology<T>(map_func: impl Fn((&String, &Topology)) -> Option<T>) -> Vec<T> {
    TOPOLOGY
        .read()
        .expect(INVARIANT)
        .iter()
        .filter_map(map_func)
        .collect()
}

fn get_sources() -> Vec<Source> {
    filter_topology(|(_, topology)| match topology {
        Topology::Source(s) => Some(s.clone()),
        _ => None,
    })
}

fn get_transforms() -> Vec<Transform> {
    filter_topology(|(_, topology)| match topology {
        Topology::Transform(t) => Some(t.clone()),
        _ => None,
    })
}

fn get_sinks() -> Vec<Sink> {
    filter_topology(|(_, topology)| match topology {
        Topology::Sink(s) => Some(s.clone()),
        _ => None,
    })
}

/// Update the 'global' configuration that will be consumed by topology queries
pub fn update_config(config: &Config) {
    let mut new_topology = HashMap::new();

    // Sources
    for (name, source) in config.sources.iter() {
        new_topology.insert(
            name.to_owned(),
            Topology::Source(Source(SourceData {
                name: name.to_owned(),
                output_type: source.output_type(),
            })),
        );
    }

    // Transforms
    for (name, transform) in config.transforms.iter() {
        new_topology.insert(
            name.to_string(),
            Topology::Transform(Transform(InputsData {
                name: name.to_owned(),
                inputs: transform.inputs.clone(),
            })),
        );
    }

    // Sinks
    for (name, sink) in config.sinks.iter() {
        new_topology.insert(
            name.to_string(),
            Topology::Sink(Sink(InputsData {
                name: name.to_owned(),
                inputs: sink.inputs.clone(),
            })),
        );
    }

    // override the old hashmap
    *TOPOLOGY.write().expect(INVARIANT) = new_topology
}
