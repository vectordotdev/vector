#![deny(warnings)]
#![deny(clippy::all)]

#[macro_use]
extern crate scan_fmt;

pub mod buffer;
mod checkpointer;
mod ifile_server;
mod ifile_watcher;
mod fingerprinter;
mod internal_events;
mod metadata_ext;
pub mod paths_provider;

pub use self::{
    checkpointer::{Checkpointer, CheckpointsView, CHECKPOINT_FILE_NAME},
    ifile_server::{calculate_ignore_before, IFileServer, Line, Shutdown as IFileServerShutdown},
    ifile_watcher::{IFileWatcher, WatcherState},
    fingerprinter::{IFileFingerprint, FingerprintStrategy, Fingerprinter},
    internal_events::IFileSourceInternalEvents,
    paths_provider::{boxed::BoxedPathsProvider, notify::NotifyPathsProvider},
};
use vector_config::configurable_component;

pub type IFilePosition = u64;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum ReadFrom {
    #[default]
    Beginning,
    End,
    Checkpoint(IFilePosition),
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
