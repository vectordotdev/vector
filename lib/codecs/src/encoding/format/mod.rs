//! A collection of formats that can be used to convert from structured events
//! to byte frames.

#![deny(missing_docs)]

mod avro;
mod common;
mod csv;
mod gelf;
mod json;
mod logfmt;
mod native;
mod native_json;
mod protobuf;
mod raw_message;
mod text;

use std::fmt::Debug;

pub use self::csv::{CsvSerializer, CsvSerializerConfig};
pub use avro::{AvroSerializer, AvroSerializerConfig, AvroSerializerOptions};
use dyn_clone::DynClone;
pub use gelf::{GelfSerializer, GelfSerializerConfig};
pub use json::{JsonSerializer, JsonSerializerConfig};
pub use logfmt::{LogfmtSerializer, LogfmtSerializerConfig};
pub use native::{NativeSerializer, NativeSerializerConfig};
pub use native_json::{NativeJsonSerializer, NativeJsonSerializerConfig};
pub use protobuf::{ProtobufSerializer, ProtobufSerializerConfig, ProtobufSerializerOptions};
pub use raw_message::{RawMessageSerializer, RawMessageSerializerConfig};
pub use text::{TextSerializer, TextSerializerConfig};
use vector_core::event::Event;

/// Serialize a structured event into a byte frame.
pub trait Serializer:
    tokio_util::codec::Encoder<Event, Error = vector_common::Error> + DynClone + Debug + Send + Sync
{
}

/// Default implementation for `Serializer`s that implement
/// `tokio_util::codec::Encoder`.
impl<Encoder> Serializer for Encoder where
    Encoder: tokio_util::codec::Encoder<Event, Error = vector_common::Error>
        + Clone
        + Debug
        + Send
        + Sync
{
}

dyn_clone::clone_trait_object!(Serializer);
