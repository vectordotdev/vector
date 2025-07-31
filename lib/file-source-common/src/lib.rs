#![deny(warnings)]
#![deny(clippy::all)]

#[macro_use]
extern crate scan_fmt;

pub mod buffer;
pub mod checkpointer;
mod fingerprinter;
pub mod internal_events;
mod metadata_ext;

pub use self::{
    checkpointer::{Checkpointer, CheckpointsView, CHECKPOINT_FILE_NAME},
    fingerprinter::{FileFingerprint, FingerprintStrategy, Fingerprinter},
    metadata_ext::PortableFileExt,
};

pub type FilePosition = u64;
