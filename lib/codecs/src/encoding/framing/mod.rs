//! A collection of framing methods that can be used to convert from byte chunks
//! to byte frames with defined boundaries.

#![deny(missing_docs)]

mod bytes;
mod character_delimited;
mod framer;
mod length_delimited;
mod newline_delimited;
mod varint_length_delimited;

pub use character_delimited::{
    CharacterDelimitedEncoder, CharacterDelimitedEncoderConfig, CharacterDelimitedEncoderOptions,
};
pub use length_delimited::{LengthDelimitedEncoder, LengthDelimitedEncoderConfig};
pub use newline_delimited::{NewlineDelimitedEncoder, NewlineDelimitedEncoderConfig};
use tokio_util::codec::LinesCodecError;

#[cfg(test)]
pub(crate) use self::framer::TestFramer;
pub use self::{
    bytes::{BytesEncoder, BytesEncoderConfig},
    framer::{Framer, FramingConfig},
    varint_length_delimited::{VarintLengthDelimitedEncoder, VarintLengthDelimitedEncoderConfig},
};

/// An error that occurred while framing bytes.
pub trait FramingError: std::error::Error + Send + Sync {}

impl std::error::Error for BoxedFramingError {}

impl FramingError for std::io::Error {}

impl FramingError for LinesCodecError {}

impl From<std::io::Error> for BoxedFramingError {
    fn from(error: std::io::Error) -> Self {
        Box::new(error)
    }
}

impl From<varint_length_delimited::VarintFramingError> for BoxedFramingError {
    fn from(error: varint_length_delimited::VarintFramingError) -> Self {
        Box::new(error)
    }
}

/// A `Box` containing a `FramingError`.
pub type BoxedFramingError = Box<dyn FramingError>;
