use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use super::{ComponentKey, Config, OutputId, enrichment_table_sinks, enrichment_table_sources};

/// Metadata about the connections between components and their types.
///
/// Used by the `internal_metrics` source to expose the topology as metrics.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TopologyMetadata {
    /// Map of each component to its inputs.
    pub inputs: HashMap<ComponentKey, Vec<OutputId>>,
    /// Map of each component to its `(component_type, component_kind)` tuple.
    pub component_types: HashMap<ComponentKey, (&'static str, &'static str)>,
}

impl TopologyMetadata {
    /// Builds the topology metadata from a compiled configuration.
    ///
    /// This covers the same components and edges as `vector graph`, including enrichment tables
    /// that act as sources or sinks.
    #[must_use]
    pub fn from_config(config: &Config) -> Self {
        let mut metadata = Self::default();

        for (key, source) in config.sources() {
            metadata.add_component(key.clone(), source.inner.get_component_name(), "source");
        }
        for (key, source) in enrichment_table_sources(&config.enrichment_tables) {
            metadata.add_component(key, source.inner.get_component_name(), "source");
        }

        for (key, transform) in config.transforms() {
            metadata.add_component(
                key.clone(),
                transform.inner.get_component_name(),
                "transform",
            );
            metadata.add_inputs(key, transform.inputs.iter());
        }

        for (key, sink) in config.sinks() {
            metadata.add_component(key.clone(), sink.inner.get_component_name(), "sink");
            metadata.add_inputs(key, sink.inputs.iter());
        }
        for (key, sink) in enrichment_table_sinks(&config.enrichment_tables) {
            metadata.add_component(key.clone(), sink.inner.get_component_name(), "sink");
            metadata.add_inputs(&key, sink.inputs.iter());
        }

        metadata
    }

    fn add_component(&mut self, key: ComponentKey, type_name: &'static str, kind: &'static str) {
        self.component_types.insert(key, (type_name, kind));
    }

    fn add_inputs<'a>(&mut self, key: &ComponentKey, inputs: impl Iterator<Item = &'a OutputId>) {
        let entry = self.inputs.entry(key.clone()).or_default();
        entry.extend(inputs.cloned());
        if entry.is_empty() {
            self.inputs.remove(key);
        }
    }
}

/// Thread-safe, shared reference to the topology metadata.
///
/// A single instance is owned by the running topology and handed to every `internal_metrics`
/// source, so all of them observe the same, current topology across configuration reloads.
pub type SharedTopologyMetadata = Arc<RwLock<TopologyMetadata>>;
