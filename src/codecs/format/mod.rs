//! A collection of formats that can be used to convert between structured
//! events and byte frames.

#![deny(missing_docs)]

mod bytes;
mod json;
#[cfg(feature = "sources-syslog")]
mod syslog;
mod text;

pub use self::bytes::{BytesDeserializer, BytesDeserializerConfig};
#[cfg(feature = "sources-syslog")]
pub use self::syslog::{SyslogDeserializer, SyslogDeserializerConfig};
pub use json::{JsonDeserializer, JsonDeserializerConfig};
pub use text::{TextSerializer, TextSerializerConfig};
