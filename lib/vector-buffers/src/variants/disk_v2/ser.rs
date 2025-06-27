use std::fmt;

use bytecheck::CheckBytes;
use rkyv::{
    rancor::{Failure, Source, Trace},
    validation::{validators::DefaultValidator, CheckArchiveError},
    Archive,
};

/// Error that occurred during serialization.
#[derive(Debug)]
pub enum SerializeError<T> {
    /// The type failed to be serialized correctly.
    FailedToSerialize(String),

    /// The backing store was not big enough to fit the serialized version of the value.
    ///
    /// The original value that was given is returned, along with the minimum size that the backing
    /// store must be sized to hold the serialized value.  Providing a backing store that is larger
    /// than the given value is acceptable, but not necessary.
    BackingStoreTooSmall(T, usize),
}

/// Error that occurred during deserialization.
#[derive(Debug)]
pub enum DeserializeError {
    /// The data in the backing store does not represent the archive type as whole.
    ///
    /// This error is primarily indicative of not having enough data present, which is often a
    /// signal that the type represented by the bytes in the backing store and the incoming archive
    /// type are either entirely different, or that the structure of the type has changed: addition
    /// or removal of fields, reordering of fields, etc.
    ///
    /// The backing store that was given is returned, along with an error string that briefly
    /// describes the error in a more verbose fashion, suitable for debugging.
    InvalidStructure(String),

    /// Some of the data in the backing store cannot represent a particular field in the archive type.
    ///
    /// This would typically occur if the data read for a particular field could not specifically
    /// represent that type.  For example, a boolean is encoded as a single byte with a 0 or 1 as
    /// the value, so a value between 2 and 255 is inherently invalid for representing a boolean.
    ///
    /// This can be a subtle difference from `InvalidStructure`, but is primarily indicative of
    /// in-place data corruption, or data being overwritten by an outside process.
    ///
    /// The backing store that was given is returned, along with an error string that briefly
    /// describes the error in a more verbose fashion, suitable for debugging.
    InvalidData(String),

    /// TODO
    Other(String),
}

impl DeserializeError {
    /// Consumes this error and returns the stringified error reason.
    pub fn into_inner(self) -> String {
        match self {
            DeserializeError::InvalidData(s) => format!("invalid data: {s}"),
            DeserializeError::InvalidStructure(s) => format!("invalid structure: {s}"),
            DeserializeError::Other(s) => format!("other: {s}"), // FIXME
        }
    }
}

impl From<Failure> for DeserializeError {
    fn from(e: Failure) -> Self {
        Self::Other(e.to_string())
    }
}

impl std::fmt::Display for DeserializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(format!("{:#?}", self).as_str()); // FIXME
        Ok(())
    }
}

impl std::error::Error for DeserializeError {}

impl Trace for DeserializeError {
    fn trace<R>(self, trace: R) -> Self
    where
        R: core::fmt::Debug + core::fmt::Display + Send + Sync + 'static,
    {
        Self::Other(trace.to_string())
    }
}

impl Source for DeserializeError {
    fn new<T: core::error::Error + Send + Sync + 'static>(source: T) -> Self {
        Self::Other(source.to_string())
    }
}

/// Tries to deserialize the given buffer as the archival type `T`.
///
/// The archived type is assumed to exist starting at index 0 of the buffer.  Additionally, the
/// archived value is checked for data conformance.
///
/// # Errors
///
/// If the buffer does not contained an archived `T`, or there was an issue with too little data, or
/// invalid values, etc, then an error variant will be emitted.  The error will describe the
/// high-level error, as well as provide a string with a more verbose explanation of the error.
pub fn try_as_archive<'a, T>(buf: &'a [u8]) -> Result<&'a T::Archived, DeserializeError>
where
    T: Archive,
    T::Archived: for<'b> CheckBytes<DefaultValidator<'b>>,
{
    debug_assert!(!buf.is_empty());
    check_archived_root::<T>(buf).map_err(Into::into)
}
