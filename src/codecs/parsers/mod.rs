//! A collection of parsers that can be used to parse structured events from
//! byte frames.

#![deny(missing_docs)]

mod bytes;
mod json;
#[cfg(feature = "sources-syslog")]
mod syslog;

pub use self::bytes::{BytesParser, BytesParserConfig};
#[cfg(feature = "sources-syslog")]
pub use self::syslog::{SyslogParser, SyslogParserConfig};
pub use json::{JsonParser, JsonParserConfig};

use crate::event::Event;
use ::bytes::Bytes;
use dyn_clone::DynClone;
use smallvec::SmallVec;
use std::fmt::Debug;

/// Parse structured events from bytes.
pub trait Parser: DynClone + Send + Sync {
    /// Parses structured events from bytes.
    ///
    /// It returns a `SmallVec` rather than an `Event` directly, since one byte
    /// frame can potentially hold multiple events, e.g. when parsing a JSON
    /// array. However, we optimize the most common case of emitting one event
    /// by not requiring heap allocations for it.
    fn parse(&self, bytes: Bytes) -> crate::Result<SmallVec<[Event; 1]>>;
}

dyn_clone::clone_trait_object!(Parser);

/// A `Box` containing a thread-safe `Parser`.
pub type BoxedParser = Box<dyn Parser + Send + Sync>;

/// Define options for a parser and build it from the config object.
///
/// Implementors must annotate the struct with `#[typetag::serde(name = "...")]`
/// to define which value should be read from the `codec` key to select their
/// implementation.
#[typetag::serde(tag = "codec")]
pub trait ParserConfig: Debug + DynClone + Send + Sync {
    /// Builds a parser from this configuration.
    ///
    /// Fails if the configuration is invalid.
    fn build(&self) -> crate::Result<BoxedParser>;
}

dyn_clone::clone_trait_object!(ParserConfig);
