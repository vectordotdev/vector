#![allow(missing_docs)]
//! Topology contains all topology based types.
//!
//! Topology is broken up into two main sections. The first
//! section contains all the main topology types include `Topology`
//! and the ability to start, stop and reload a config. The second
//! part contains config related items including config traits for
//! each type of component.

pub(super) use vector_lib::fanout;
pub mod schema;

pub mod builder;
mod controller;
mod ready_arrays;
mod running;
mod task;

#[cfg(test)]
mod test;

use std::{
    collections::{HashMap, HashSet},
    panic::AssertUnwindSafe,
    sync::{Arc, Mutex},
};

use futures::{Future, FutureExt};
use tokio::sync::{mpsc, watch};
use vector_lib::buffers::topology::channel::{BufferReceiverStream, BufferSender};

pub use self::builder::TopologyPieces;
pub use self::controller::{ReloadOutcome, SharedTopologyController, TopologyController};
pub use self::running::{RunningTopology, ShutdownErrorReceiver};

use self::task::{Task, TaskError, TaskResult};
use crate::{
    config::{ComponentKey, Config, ConfigDiff, Inputs, OutputId},
    event::EventArray,
    signal::ShutdownError,
};

type TaskHandle = tokio::task::JoinHandle<TaskResult>;

type BuiltBuffer = (
    BufferSender<EventArray>,
    Arc<Mutex<Option<BufferReceiverStream<EventArray>>>>,
);

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
type WatchTx = watch::Sender<TapResource>;
pub type WatchRx = watch::Receiver<TapResource>;

pub(super) fn take_healthchecks(
    diff: &ConfigDiff,
    pieces: &mut TopologyPieces,
) -> Vec<(ComponentKey, Task)> {
    (&diff.sinks.to_change | &diff.sinks.to_add)
        .into_iter()
        .filter_map(|id| pieces.healthchecks.remove(&id).map(move |task| (id, task)))
        .collect()
}

async fn handle_errors(
    task: impl Future<Output = TaskResult>,
    abort_tx: mpsc::UnboundedSender<ShutdownError>,
    error: impl FnOnce(String) -> ShutdownError,
) -> TaskResult {
    AssertUnwindSafe(task)
        .catch_unwind()
        .await
        .map_err(|_| TaskError::Panicked)
        .and_then(|res| res)
        .map_err(|e| {
            error!("An error occurred that Vector couldn't handle: {}.", e);
            _ = abort_tx.send(error(e.to_string()));
            e
        })
}

/// If the closure returns false, then the element is removed
fn retain<T>(vec: &mut Vec<T>, mut retain_filter: impl FnMut(&mut T) -> bool) {
    let mut i = 0;
    while let Some(data) = vec.get_mut(i) {
        if retain_filter(data) {
            i += 1;
        } else {
            _ = vec.remove(i);
        }
    }
}
