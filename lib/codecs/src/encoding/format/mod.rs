//! A collection of formats that can be used to convert from structured events
//! to byte frames.

#![deny(missing_docs)]

#[cfg(feature = "arrow")]
mod arrow;
mod avro;
mod cef;
mod common;
mod csv;
mod gelf;
mod json;
mod logfmt;
mod native;
mod native_json;
#[cfg(feature = "opentelemetry")]
mod otlp;
mod protobuf;
mod raw_message;
mod text;

use std::fmt::Debug;

#[cfg(feature = "arrow")]
pub use arrow::{
    ArrowEncodingError, ArrowStreamSerializer, ArrowStreamSerializerConfig, SchemaProvider,
};
pub use avro::{AvroSerializer, AvroSerializerConfig, AvroSerializerOptions};
pub use cef::{CefSerializer, CefSerializerConfig};
use dyn_clone::DynClone;
pub use gelf::{GelfSerializer, GelfSerializerConfig};
pub use json::{JsonSerializer, JsonSerializerConfig, JsonSerializerOptions};
pub use logfmt::{LogfmtSerializer, LogfmtSerializerConfig};
pub use native::{NativeSerializer, NativeSerializerConfig};
pub use native_json::{NativeJsonSerializer, NativeJsonSerializerConfig};
#[cfg(feature = "opentelemetry")]
pub use otlp::{OtlpSerializer, OtlpSerializerConfig};
pub use protobuf::{ProtobufSerializer, ProtobufSerializerConfig, ProtobufSerializerOptions};
pub use raw_message::{RawMessageSerializer, RawMessageSerializerConfig};
pub use text::{TextSerializer, TextSerializerConfig};
use vector_core::event::Event;

pub use self::csv::{CsvSerializer, CsvSerializerConfig};

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
