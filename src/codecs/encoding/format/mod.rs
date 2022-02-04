//! A collection of formats that can be used to convert from structured events
//! to byte frames.

#![deny(missing_docs)]

mod json;
mod raw_message;

pub use json::{JsonSerializer, JsonSerializerConfig};
pub use raw_message::{RawMessageSerializer, RawMessageSerializerConfig};

use crate::event::Event;
use dyn_clone::DynClone;
use std::fmt::Debug;

/// Serialize a structured event into a byte frame.
pub trait Serializer:
    tokio_util::codec::Encoder<Event, Error = crate::Error> + DynClone + Debug + Send + Sync
{
}

/// Default implementation for `Serializer`s that implement
/// `tokio_util::codec::Encoder`.
impl<Encoder> Serializer for Encoder where
    Encoder: tokio_util::codec::Encoder<Event, Error = crate::Error> + Clone + Debug + Send + Sync
{
}

dyn_clone::clone_trait_object!(Serializer);

/// A `Box` containing a `Serializer`.
pub type BoxedSerializer = Box<dyn Serializer>;

/// Define options for a serializer and build it from the config object.
///
/// Implementors must annotate the struct with `#[typetag::serde(name = "...")]`
/// to define which value should be read from the `codec` key to select their
/// implementation.
#[typetag::serde(tag = "codec")]
pub trait SerializerConfig: Debug + DynClone + Send + Sync {
    /// Builds a serializer from this configuration.
    ///
    /// Fails if the configuration is invalid.
    fn build(&self) -> crate::Result<BoxedSerializer>;
}

dyn_clone::clone_trait_object!(SerializerConfig);
