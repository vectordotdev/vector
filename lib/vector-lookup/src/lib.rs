#![deny(warnings)]

pub use vrl::{
    event_path, metadata_path, owned_value_path, path,
    path::{OwnedTargetPath, OwnedValuePath, PathPrefix},
};

pub mod lookup_v2;
