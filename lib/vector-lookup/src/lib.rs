#![deny(warnings)]

pub use lookup_v2::PathPrefix;

pub use lookup::{
    event_path, metadata_path, owned_value_path, path, Field, FieldBuf, Look, LookSegment, Lookup,
    LookupBuf, LookupError, OwnedTargetPath, OwnedValuePath, Segment, SegmentBuf,
};

pub mod lookup_v2;
