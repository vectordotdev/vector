use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use super::{ComponentKey, OutputId};

/// Metadata about the topology connections and component types
/// Used by internal_metrics source to expose topology as metrics
#[derive(Clone, Debug, Default)]
pub struct TopologyMetadata {
    /// Map of component to its inputs
    pub inputs: HashMap<ComponentKey, Vec<OutputId>>,
    /// Map of component to its (type, kind) tuple
    pub component_types: HashMap<ComponentKey, (String, String)>,
}

impl TopologyMetadata {
    /// Create a new TopologyMetadata instance
    pub fn new() -> Self {
        Self::default()
    }

    /// Clear all metadata
    pub fn clear(&mut self) {
        self.inputs.clear();
        self.component_types.clear();
    }
}

/// Thread-safe reference to topology metadata
pub type SharedTopologyMetadata = Arc<RwLock<TopologyMetadata>>;
