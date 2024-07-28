//! A collection of framing methods that can be used to convert from byte frames
//! with defined boundaries to byte chunks.

#![deny(missing_docs)]

mod bytes;
mod character_delimited;
mod chunked_gelf;
mod length_delimited;
mod newline_delimited;
mod octet_counting;

use std::{any::Any, fmt::Debug};

use ::bytes::Bytes;
pub use character_delimited::{
    CharacterDelimitedDecoder, CharacterDelimitedDecoderConfig, CharacterDelimitedDecoderOptions,
};
pub use chunked_gelf::{ChunkedGelfDecoder, ChunkedGelfDecoderConfig, ChunkedGelfDecoderOptions};
use dyn_clone::DynClone;
pub use length_delimited::{LengthDelimitedDecoder, LengthDelimitedDecoderConfig};
pub use newline_delimited::{
    NewlineDelimitedDecoder, NewlineDelimitedDecoderConfig, NewlineDelimitedDecoderOptions,
};
pub use octet_counting::{
    OctetCountingDecoder, OctetCountingDecoderConfig, OctetCountingDecoderOptions,
};
use tokio_util::codec::LinesCodecError;

pub use self::bytes::{BytesDecoder, BytesDecoderConfig};
use super::StreamDecodingError;

/// An error that occurred while producing byte frames from a byte stream / byte
/// message.
///
/// It requires conformance to `TcpError` so that we can determine whether the
/// error is recoverable or if trying to continue will lead to hanging up the
/// TCP source indefinitely.
pub trait FramingError: std::error::Error + StreamDecodingError + Send + Sync + Any {
    /// Coerces the error to a `dyn Any`.
    /// This is useful for downcasting the error to a concrete type, as we are dealing
    /// with Box<dyn FramingError> instead of a concrete `FramingError` enum.
    fn as_any(&self) -> &dyn Any;
}

impl std::error::Error for BoxedFramingError {}

impl FramingError for std::io::Error {
    fn as_any(&self) -> &dyn Any {
        self as &dyn Any
    }
}

impl FramingError for LinesCodecError {
    fn as_any(&self) -> &dyn Any {
        self as &dyn Any
    }
}

impl<T> From<T> for BoxedFramingError
where
    T: FramingError + 'static,
{
    fn from(value: T) -> Self {
        Box::new(value)
    }
}

/// A `Box` containing a `FramingError`.
pub type BoxedFramingError = Box<dyn FramingError>;

impl StreamDecodingError for BoxedFramingError {
    fn can_continue(&self) -> bool {
        self.as_ref().can_continue()
    }
}

/// Produce byte frames from a byte stream / byte message.
pub trait Framer:
    tokio_util::codec::Decoder<Item = Bytes, Error = BoxedFramingError> + DynClone + Debug + Send + Sync
{
}

/// Default implementation for `Framer`s that implement
/// `tokio_util::codec::Decoder`.
impl<Decoder> Framer for Decoder where
    Decoder: tokio_util::codec::Decoder<Item = Bytes, Error = BoxedFramingError>
        + Clone
        + Debug
        + Send
        + Sync
{
}

dyn_clone::clone_trait_object!(Framer);

/// A `Box` containing a `Framer`.
pub type BoxedFramer = Box<dyn Framer>;
