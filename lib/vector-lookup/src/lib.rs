#![deny(warnings)]

pub use vrl::path::{OwnedTargetPath, OwnedValuePath, PathPrefix};

pub use vrl::{event_path, metadata_path, owned_value_path, path};

pub mod lookup_v2;
