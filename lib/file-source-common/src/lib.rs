#![deny(warnings)]
#![deny(clippy::all)]

#[macro_use]
extern crate scan_fmt;

pub mod buffer;
pub mod checkpointer;
mod fingerprinter;
mod metadata_ext;
pub mod internal_events;

pub use self::{
    checkpointer::{Checkpointer, CheckpointsView, CHECKPOINT_FILE_NAME},
    fingerprinter::{FileFingerprint, FingerprintStrategy, Fingerprinter},
    metadata_ext::PortableFileExt,
};
