//! A collection of formats that can be used to convert from byte frames to
//! structured events.

#![deny(missing_docs)]

mod bytes;
mod json;
#[cfg(feature = "sources-syslog")]
mod syslog;

pub use self::bytes::{BytesDeserializer, BytesDeserializerConfig};
#[cfg(feature = "sources-syslog")]
pub use self::syslog::{SyslogDeserializer, SyslogDeserializerConfig};
pub use json::{JsonDeserializer, JsonDeserializerConfig};

use crate::event::Event;
use ::bytes::Bytes;
use dyn_clone::DynClone;
use smallvec::SmallVec;
use std::fmt::Debug;

/// Parse structured events from bytes.
pub trait Deserializer: DynClone + Debug + Send + Sync {
    /// Parses structured events from bytes.
    ///
    /// It returns a `SmallVec` rather than an `Event` directly, since one byte
    /// frame can potentially hold multiple events, e.g. when parsing a JSON
    /// array. However, we optimize the most common case of emitting one event
    /// by not requiring heap allocations for it.
    fn parse(&self, bytes: Bytes) -> crate::Result<SmallVec<[Event; 1]>>;
}

dyn_clone::clone_trait_object!(Deserializer);

/// A `Box` containing a `Deserializer`.
pub type BoxedDeserializer = Box<dyn Deserializer>;
