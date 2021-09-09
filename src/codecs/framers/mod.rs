#![deny(missing_docs)]

mod bytes;
mod character_delimited;
mod length_delimited;
mod newline_delimited;
mod octet_counting;

pub use self::bytes::{BytesCodec, BytesDecoderConfig};
pub use character_delimited::{CharacterDelimitedCodec, CharacterDelimitedDecoderConfig};
pub use length_delimited::{LengthDelimitedCodec, LengthDelimitedDecoderConfig};
pub use newline_delimited::{NewlineDelimitedCodec, NewlineDelimitedDecoderConfig};
pub use octet_counting::{OctetCountingCodec, OctetCountingDecoderConfig};

use crate::sources::util::TcpError;
use ::bytes::Bytes;
use dyn_clone::DynClone;
use std::fmt::Debug;
use tokio_util::codec::LinesCodecError;

/// An error that occurred while producing byte frames from a byte stream / byte
/// message.
///
/// It requires conformance to `TcpError` so that we can determine whether the
/// error is recoverable or if trying to continue will lead to hanging up the
/// TCP source indefinitely.
pub trait FramingError: std::error::Error + TcpError + Send + Sync {}

impl std::error::Error for BoxedFramingError {}

impl FramingError for std::io::Error {}

impl FramingError for LinesCodecError {}

impl From<std::io::Error> for BoxedFramingError {
    fn from(error: std::io::Error) -> Self {
        Box::new(error)
    }
}

impl From<LinesCodecError> for BoxedFramingError {
    fn from(error: LinesCodecError) -> Self {
        Box::new(error)
    }
}

/// A `Box` containing a `FramingError`.
pub type BoxedFramingError = Box<dyn FramingError>;

impl TcpError for BoxedFramingError {
    fn can_continue(&self) -> bool {
        self.as_ref().can_continue()
    }
}

/// Produce byte frames from a byte stream / byte message.
pub trait Framer:
    tokio_util::codec::Decoder<Item = Bytes, Error = BoxedFramingError> + DynClone + Send + Sync
{
}

/// Default implementation for `Framer`s that implement
/// `tokio_util::codec::Decoder` and `Clone`.
impl<Decoder> Framer for Decoder where
    Decoder:
        tokio_util::codec::Decoder<Item = Bytes, Error = BoxedFramingError> + Clone + Send + Sync
{
}

dyn_clone::clone_trait_object!(Framer);

/// A `Box` containing a thread-safe `Framer`.
pub type BoxedFramer = Box<dyn Framer + Send + Sync>;

/// Define options for a framer and build it from the config object.
///
/// Implementors must annotate the struct with `#[typetag::serde(name = "...")]`
/// to define which value should be read from the `method` key to select their
/// implementation.
#[typetag::serde(tag = "method")]
pub trait FramingConfig: Debug + DynClone + Send + Sync {
    /// Builds a framer from this configuration.
    ///
    /// Fails if the configuration is invalid.
    fn build(&self) -> crate::Result<BoxedFramer>;
}

dyn_clone::clone_trait_object!(FramingConfig);
