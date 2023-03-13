#![deny(warnings)]
#![deny(clippy::all)]

#[macro_use]
extern crate scan_fmt;

pub mod buffer;
mod checkpointer;
mod file_server;
mod file_watcher;
mod fingerprinter;
mod internal_events;
mod metadata_ext;
pub mod paths_provider;

pub use self::{
    checkpointer::{Checkpointer, CheckpointsView, CHECKPOINT_FILE_NAME},
    file_server::{calculate_ignore_before, FileServer, Line, Shutdown as FileServerShutdown},
    fingerprinter::{FileFingerprint, FingerprintStrategy, Fingerprinter},
    internal_events::FileSourceInternalEvents,
};
use vector_config::configurable_component;

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
