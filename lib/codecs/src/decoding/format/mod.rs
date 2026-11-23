//! A collection of formats that can be used to convert from byte frames to
//! structured events.

#![deny(missing_docs)]

mod avro;
mod bytes;
mod gelf;
mod influxdb;
mod json;
mod msgpack;
mod native;
mod native_json;
#[cfg(feature = "opentelemetry")]
mod otlp;
mod protobuf;
#[cfg(feature = "syslog")]
mod syslog;
mod vrl;

use ::bytes::Bytes;
use std::sync::Arc;

pub use avro::{AvroDeserializer, AvroDeserializerConfig, AvroDeserializerOptions};
pub use gelf::{GelfDeserializer, GelfDeserializerConfig, GelfDeserializerOptions};
pub use influxdb::{InfluxdbDeserializer, InfluxdbDeserializerConfig};
pub use json::{JsonDeserializer, JsonDeserializerConfig, JsonDeserializerOptions};
pub use msgpack::{MsgPackDeserializer, MsgPackDeserializerConfig};
pub use native::{NativeDeserializer, NativeDeserializerConfig};
pub use native_json::{
    NativeJsonDeserializer, NativeJsonDeserializerConfig, NativeJsonDeserializerOptions,
};
#[cfg(feature = "opentelemetry")]
pub use otlp::{OtlpDeserializer, OtlpDeserializerConfig, OtlpSignalType};
pub use protobuf::{ProtobufDeserializer, ProtobufDeserializerConfig, ProtobufDeserializerOptions};
use smallvec::SmallVec;
#[cfg(feature = "syslog")]
pub use syslog::{SyslogDeserializer, SyslogDeserializerConfig, SyslogDeserializerOptions};
use vector_core::{config::LogNamespace, event::Event};

pub use self::{
    bytes::{BytesDeserializer, BytesDeserializerConfig},
    vrl::{VrlDeserializer, VrlDeserializerConfig, VrlDeserializerOptions},
};

/// Parse structured events from bytes.
pub trait Deserializer: Send + Sync {
    /// Parses structured events from bytes.
    ///
    /// It returns a `SmallVec` rather than an `Event` directly, since one byte
    /// frame can potentially hold multiple events, e.g. when parsing a JSON
    /// array. However, we optimize the most common case of emitting one event
    /// by not requiring heap allocations for it.
    ///
    /// **Note**: The type of the produced events depends on the implementation.
    ///
    /// TODO: <https://github.com/vectordotdev/vector/issues/25044>
    ///
    /// # Errors
    ///
    /// Implementations must return an error if the bytes cannot be parsed into events of the
    /// expected format.
    fn parse(
        &self,
        bytes: Bytes,
        log_namespace: LogNamespace,
    ) -> vector_common::Result<SmallVec<[Event; 1]>>;
}

/// A shared `Deserializer` trait object.
///
/// Deserializers parse through `&self`, so one instance can serve every
/// connection that uses it. Sharing keeps custom deserializers out of the
/// `Clone` requirement.
pub type SharedDeserializer = Arc<dyn Deserializer>;

/// Default value for the UTF-8 lossy option.
const fn default_lossy() -> bool {
    true
}
