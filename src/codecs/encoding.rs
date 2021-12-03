//! A collection of support structures that are used in the process of encoding
//! events into bytes.

use crate::{
    codecs::{NewlineDelimitedEncoder, TextSerializer},
    event::Event,
    internal_events::{EncoderFramingFailed, EncoderSerializeFailed},
};
use bytes::{Bytes, BytesMut};
use dyn_clone::DynClone;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use tokio_util::codec::LinesCodecError;

/// An error that occurred while encoding structured events into byte frames.
#[derive(Debug)]
pub enum Error {
    /// The error occurred while encoding the byte frame boundaries.
    FramingError(BoxedFramingError),
    /// The error occurred while serializing a structured event into bytes.
    SerializingError(crate::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FramingError(error) => write!(formatter, "FramingError({})", error),
            Self::SerializingError(error) => write!(formatter, "SerializingError({})", error),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::FramingError(Box::new(error))
    }
}

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
    tokio_util::codec::Encoder<Bytes, Error = BoxedFramingError> + DynClone + Debug + Send + Sync
{
}

/// Default implementation for `Framer`s that implement
/// `tokio_util::codec::Encoder`.
impl<Encoder> Framer for Encoder where
    Encoder:
        tokio_util::codec::Encoder<Bytes, Error = BoxedFramingError> + Clone + Debug + Send + Sync
{
}

dyn_clone::clone_trait_object!(Framer);

/// A `Box` containing a `Framer`.
pub type BoxedFramer = Box<dyn Framer>;

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

/// Serialize a structured event into byte frames.
pub trait Serializer:
    tokio_util::codec::Encoder<Event, Error = crate::Error> + DynClone + Debug + Send + Sync
{
}

/// Default implementation for `Serializer`s that implement
/// `tokio_util::codec::Encoder`.
impl<Encoder> Serializer for Encoder where
    Encoder: tokio_util::codec::Encoder<Event, Error = crate::Error> + Clone + Debug + Send + Sync
{
}

dyn_clone::clone_trait_object!(Serializer);

/// A `Box` containing a `Serializer`.
pub type BoxedSerializer = Box<dyn Serializer>;

/// Define options for a serializer and build it from the config object.
///
/// Implementors must annotate the struct with `#[typetag::serde(name = "...")]`
/// to define which value should be read from the `codec` key to select their
/// implementation.
#[typetag::serde(tag = "codec")]
pub trait SerializerConfig: Debug + DynClone + Send + Sync {
    /// Builds a serializer from this configuration.
    ///
    /// Fails if the configuration is invalid.
    fn build(&self) -> crate::Result<BoxedSerializer>;
}

dyn_clone::clone_trait_object!(SerializerConfig);

#[derive(Debug, Clone)]
/// An encoder that can encode structured events into byte frames.
pub struct Encoder {
    framer: BoxedFramer,
    serializer: BoxedSerializer,
}

impl Default for Encoder {
    fn default() -> Self {
        Self {
            framer: Box::new(NewlineDelimitedEncoder::new()),
            serializer: Box::new(TextSerializer::new()),
        }
    }
}

impl Encoder {
    /// Creates a new `Encoder` with the specified `Serializer` to produce bytes
    /// from a structured event, and the `Framer` to wrap these into byte
    /// frames.
    pub fn new(framer: BoxedFramer, serializer: BoxedSerializer) -> Self {
        Self { framer, serializer }
    }
}

impl tokio_util::codec::Encoder<Event> for Encoder {
    type Error = Error;

    fn encode(&mut self, item: Event, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        let mut payload = BytesMut::new();

        // Serialize the event.
        self.serializer
            .encode(item, &mut payload)
            .map_err(|error| {
                emit!(&EncoderSerializeFailed { error: &error });
                Error::SerializingError(error)
            })?;

        // Frame the serialized event.
        self.framer
            .encode(payload.freeze(), buffer)
            .map_err(|error| {
                emit!(&EncoderFramingFailed { error: &error });
                Error::FramingError(error)
            })
    }
}

/// Config used to build an `Encoder`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EncodingConfig {
    /// The framing config.
    framing: Box<dyn FramingConfig>,
    /// The encoding config.
    encoding: Box<dyn SerializerConfig>,
}

impl EncodingConfig {
    /// Creates a new `EncodingConfig` with the provided `FramingConfig` and
    /// `SerializerConfig`.
    pub fn new(framing: Box<dyn FramingConfig>, encoding: Box<dyn SerializerConfig>) -> Self {
        Self { framing, encoding }
    }

    /// Builds an `Encoder` from the provided configuration.
    pub fn build(&self) -> crate::Result<Encoder> {
        // Build the framer.
        let framer: BoxedFramer = self.framing.build()?;

        // Build the serializer.
        let serializer: BoxedSerializer = self.encoding.build()?;

        Ok(Encoder::new(framer, serializer))
    }
}
