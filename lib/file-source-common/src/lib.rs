#![deny(warnings)]
#![deny(clippy::all)]

#[macro_use]
extern crate scan_fmt;

pub mod buffer;
pub mod checkpointer;
mod fingerprinter;
pub mod internal_events;
mod metadata_ext;

use std::collections::HashMap;

use tokio::task::{Id, JoinError, JoinSet};
use vector_config::configurable_component;

pub use self::{
    checkpointer::{CHECKPOINT_FILE_NAME, Checkpointer, CheckpointsView},
    fingerprinter::{FileFingerprint, FingerprintStrategy, Fingerprinter},
    internal_events::FileSourceInternalEvents,
    metadata_ext::{AsyncFileInfo, PortableFileExt},
};

pub type FilePosition = u64;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum ReadFrom {
    #[default]
    Beginning,
    End,
    Checkpoint(FilePosition),
}

/// File position to use when reading a new file.
#[configurable_component]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReadFromConfig {
    /// Read from the beginning of the file.
    Beginning,

    /// Start reading from the current end of the file.
    End,
}

impl From<ReadFromConfig> for ReadFrom {
    fn from(rfc: ReadFromConfig) -> Self {
        match rfc {
            ReadFromConfig::Beginning => ReadFrom::Beginning,
            ReadFromConfig::End => ReadFrom::End,
        }
    }
}

pub struct TaskSet<K, T> {
    ids: HashMap<Id, K>,
    set: JoinSet<(K, T)>,
}

impl<K: Clone + Send + Sync + 'static, T: 'static> TaskSet<K, T> {
    pub fn new() -> TaskSet<K, T> {
        TaskSet {
            ids: HashMap::new(),
            set: JoinSet::new(),
        }
    }

    #[track_caller]
    pub fn spawn<F>(&mut self, key: K, task: F)
    where
        F: Future<Output = T> + Send + 'static,
        T: Send,
    {
        let key_ = key.clone();
        let abort_handle = self.set.spawn(async move { (key_, task.await) });
        self.ids.insert(abort_handle.id(), key);
    }

    pub async fn join_next(&mut self) -> Option<(K, Result<T, JoinError>)> {
        Some(match self.set.join_next().await? {
            Ok((key, result)) => (key, Ok(result)),
            Err(join_err) => {
                let key = self
                    .ids
                    .remove(&join_err.id())
                    .expect("panicked/cancelled task id not in task id pool");
                (key, Err(join_err))
            }
        })
    }
}
