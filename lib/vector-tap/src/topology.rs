use std::collections::{HashMap, HashSet};
use tokio::sync::watch;
use vector_common::config::ComponentKey;
use vector_common::id::Inputs;
use vector_core::config::OutputId;
use vector_core::fanout;

/// A tappable output consisting of an output ID and associated metadata
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct TapOutput {
    pub output_id: OutputId,
    pub component_kind: &'static str,
    pub component_type: String,
}

/// Resources used by the `tap` API to monitor component inputs and outputs,
/// updated alongside the topology
#[derive(Debug, Default, Clone)]
pub struct TapResource {
    // Outputs and their corresponding Fanout control
    pub outputs: HashMap<TapOutput, fanout::ControlChannel>,
    // Components (transforms, sinks) and their corresponding inputs
    pub inputs: HashMap<ComponentKey, Inputs<OutputId>>,
    // Source component keys used to warn against invalid pattern matches
    pub source_keys: Vec<String>,
    // Sink component keys used to warn against invalid pattern matches
    pub sink_keys: Vec<String>,
    // Components removed on a reload (used to drop TapSinks)
    pub removals: HashSet<ComponentKey>,
}

// Watcher types for topology changes.
pub type WatchTx = watch::Sender<TapResource>;
pub type WatchRx = watch::Receiver<TapResource>;
