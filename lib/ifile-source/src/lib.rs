#![deny(warnings)]
#![deny(clippy::all)]

mod file_server;
mod file_watcher;
pub mod paths_provider;

#[cfg(any(test, feature = "test"))]
pub use self::file_server::TestEvent;
pub use self::{
    file_server::{calculate_ignore_before, FileServer, Line, Shutdown as FileServerShutdown},
    file_watcher::{FileWatcher, WatcherState},
    paths_provider::{boxed::BoxedPathsProvider, notify::NotifyPathsProvider},
};
pub use file_source_common::{
    buffer,
    checkpointer::{Checkpointer, CheckpointsView, CHECKPOINT_FILE_NAME},
    internal_events::FileSourceExtendedInternalEvents as FileSourceInternalEvents,
    FileFingerprint, FingerprintStrategy, Fingerprinter, PortableFileExt,
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
