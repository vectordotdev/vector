//! A collection of formats that can be used to convert from byte frames to
//! structured events.

#![deny(missing_docs)]

mod bytes;
mod gelf;
mod json;
mod native;
mod native_json;
#[cfg(feature = "syslog")]
mod syslog;

use ::bytes::Bytes;
use dyn_clone::DynClone;
pub use gelf::{GelfDeserializer, GelfDeserializerConfig};
pub use json::{JsonDeserializer, JsonDeserializerConfig};
pub use native::{NativeDeserializer, NativeDeserializerConfig};
pub use native_json::{NativeJsonDeserializer, NativeJsonDeserializerConfig};
use smallvec::SmallVec;
use vector_core::config::LogNamespace;
use vector_core::event::Event;

pub use self::bytes::{BytesDeserializer, BytesDeserializerConfig};
#[cfg(feature = "syslog")]
pub use self::syslog::{SyslogDeserializer, SyslogDeserializerConfig};

/// Parse structured events from bytes.
pub trait Deserializer: DynClone + Send + Sync {
    /// Parses structured events from bytes.
    ///
    /// It returns a `SmallVec` rather than an `Event` directly, since one byte
    /// frame can potentially hold multiple events, e.g. when parsing a JSON
    /// array. However, we optimize the most common case of emitting one event
    /// by not requiring heap allocations for it.
    fn parse(
        &self,
        bytes: Bytes,
        log_namespace: LogNamespace,
    ) -> vector_common::Result<SmallVec<[Event; 1]>>;
}

dyn_clone::clone_trait_object!(Deserializer);

/// A `Box` containing a `Deserializer`.
pub type BoxedDeserializer = Box<dyn Deserializer>;
