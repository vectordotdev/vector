//! A collection of framing methods that can be used to convert from byte chunks
//! to byte frames with defined boundaries.

#![deny(missing_docs)]

mod bytes;
mod character_delimited;
mod newline_delimited;

pub use self::bytes::{BytesEncoder, BytesEncoderConfig};
pub use character_delimited::{
    CharacterDelimitedEncoder, CharacterDelimitedEncoderConfig, CharacterDelimitedEncoderOptions,
};
pub use newline_delimited::{NewlineDelimitedEncoder, NewlineDelimitedEncoderConfig};

use dyn_clone::DynClone;
use std::fmt::Debug;
use tokio_util::codec::LinesCodecError;

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

/// A `Box` containing a `FramingError`.
pub type BoxedFramingError = Box<dyn FramingError>;

/// Wrap bytes into a frame.
pub trait Framer:
    tokio_util::codec::Encoder<(), Error = BoxedFramingError> + DynClone + Debug + Send + Sync
{
}

/// Default implementation for `Framer`s that implement
/// `tokio_util::codec::Encoder`.
impl<Encoder> Framer for Encoder where
    Encoder:
        tokio_util::codec::Encoder<(), Error = BoxedFramingError> + Clone + Debug + Send + Sync
{
}

dyn_clone::clone_trait_object!(Framer);

/// A `Box` containing a `Framer`.
pub type BoxedFramer = Box<dyn Framer>;
