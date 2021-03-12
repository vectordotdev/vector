#![deny(clippy::all)]

#[macro_use]
extern crate scan_fmt;
#[macro_use]
extern crate tracing;

mod checkpointer;
mod file_server;
mod file_watcher;
mod fingerprinter;
mod internal_events;
mod metadata_ext;
#[cfg(test)]
mod model_tests;
pub mod paths_provider;

pub use self::file_server::{FileServer, Shutdown as FileServerShutdown};
pub use self::fingerprinter::{FingerprintStrategy, Fingerprinter};
pub use self::internal_events::FileSourceInternalEvents;

type FilePosition = u64;

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
