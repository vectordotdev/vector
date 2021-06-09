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

pub use self::checkpointer::{Checkpointer, CheckpointsView};
pub use self::file_server::{FileServer, Line, Shutdown as FileServerShutdown};
pub use self::fingerprinter::{FileFingerprint, FingerprintStrategy, Fingerprinter};
pub use self::internal_events::FileSourceInternalEvents;

pub type FilePosition = u64;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ReadFrom {
    Beginning,
    End,
    Checkpoint(FilePosition),
}

impl Default for ReadFrom {
    fn default() -> Self {
        ReadFrom::Beginning
    }
}
