//! A collection of support structures that are used in the process of encoding
//! events into bytes.

use crate::{
    codecs::{NewlineDelimitedEncoder, RawMessageSerializer},
    event::Event,
    internal_events::{EncoderFramingFailed, EncoderSerializeFailed},
};
use bytes::BytesMut;
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
pub trait Framer: DynClone + Debug + Send + Sync {
    /// Wrap the buffer into a byte frame.
    fn frame(&self, buffer: &mut BytesMut) -> Result<(), BoxedFramingError>;
}

impl tokio_util::codec::Encoder<()> for dyn Framer {
    type Error = BoxedFramingError;

    fn encode(&mut self, _: (), dst: &mut BytesMut) -> Result<(), Self::Error> {
        self.frame(dst)
    }
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

/// Serialize a structured event into a byte frame.
pub trait Serializer: DynClone + Debug + Send + Sync {
    /// Serialize an event into the provided buffer.
    fn serialize(&self, event: Event, buffer: &mut BytesMut) -> crate::Result<()>;
}

impl tokio_util::codec::Encoder<Event> for dyn Serializer {
    type Error = crate::Error;

    fn encode(&mut self, item: Event, dst: &mut BytesMut) -> Result<(), Self::Error> {
        self.serialize(item, dst)
    }
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
            serializer: Box::new(RawMessageSerializer::new()),
        }
    }
}

impl Encoder {
    /// Creates a new `Encoder` with the specified `Serializer` to produce bytes
    /// from a structured event, and the `Framer` to wrap these into a byte
    /// frame.
    pub fn new(framer: BoxedFramer, serializer: BoxedSerializer) -> Self {
        Self { framer, serializer }
    }

    /// Encode an event into the provided buffer by first serializing the event
    /// and subsequently wrapping the buffer into a byte frame.
    pub fn encode(&self, item: Event, buffer: &mut BytesMut) -> Result<(), Error> {
        let len = buffer.len();

        let mut payload = buffer.split_off(len);

        // Serialize the event.
        self.serializer
            .serialize(item, &mut payload)
            .map_err(|error| {
                emit!(&EncoderSerializeFailed { error: &error });
                Error::SerializingError(error)
            })?;

        // Frame the serialized event.
        self.framer.frame(&mut payload).map_err(|error| {
            emit!(&EncoderFramingFailed { error: &error });
            Error::FramingError(error)
        })?;

        buffer.unsplit(payload);

        Ok(())
    }
}

impl tokio_util::codec::Encoder<Event> for Encoder {
    type Error = Error;

    fn encode(&mut self, item: Event, dst: &mut BytesMut) -> Result<(), Self::Error> {
        Self::encode(self, item, dst)
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

#[cfg(test)]
mod tests {
    use std::{
        cell::Cell,
        sync::{Arc, Mutex},
    };

    use super::*;
    use crate::codecs::RawMessageSerializer;
    use bytes::BufMut;
    use futures_util::{SinkExt, StreamExt};
    use tokio_util::codec::FramedWrite;

    #[derive(Debug, Clone)]
    struct ParenEncoder;

    impl ParenEncoder {
        pub const fn new() -> Self {
            Self
        }
    }

    impl Framer for ParenEncoder {
        fn frame(&self, buffer: &mut BytesMut) -> Result<(), BoxedFramingError> {
            buffer.reserve(2);
            let inner = buffer.split();
            buffer.put_u8(b'(');
            buffer.unsplit(inner);
            buffer.put_u8(b')');
            Ok(())
        }
    }

    #[derive(Debug, Clone)]
    struct ErrorNthEncoder<T>(T, Arc<Mutex<Cell<usize>>>, usize)
    where
        T: Framer;

    impl<T> ErrorNthEncoder<T>
    where
        T: Framer,
    {
        pub fn new(encoder: T, n: usize) -> Self {
            Self(encoder, Arc::new(Mutex::new(Cell::new(0))), n)
        }
    }

    impl<T> Framer for ErrorNthEncoder<T>
    where
        T: Framer + Clone,
    {
        fn frame(&self, buffer: &mut BytesMut) -> Result<(), BoxedFramingError> {
            self.0.frame(buffer)?;
            let i = self.1.lock().unwrap();
            let result = if i.get() == self.2 {
                Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "error")) as _)
            } else {
                Ok(())
            };
            i.set(i.get() + 1);
            result
        }
    }

    #[tokio::test]
    async fn test_encode_events_sink_empty() {
        let encoder = Encoder::new(
            Box::new(ParenEncoder::new()),
            Box::new(RawMessageSerializer::new()),
        );
        let source = futures::stream::iter(vec![
            Event::from("foo"),
            Event::from("bar"),
            Event::from("baz"),
        ])
        .map(Ok);
        let sink = Vec::new();
        let mut framed = FramedWrite::new(sink, encoder);
        source.forward(&mut framed).await.unwrap();
        let sink = framed.into_inner();
        assert_eq!(sink, b"(foo)(bar)(baz)");
    }

    #[tokio::test]
    async fn test_encode_events_sink_non_empty() {
        let encoder = Encoder::new(
            Box::new(ParenEncoder::new()),
            Box::new(RawMessageSerializer::new()),
        );
        let source = futures::stream::iter(vec![
            Event::from("bar"),
            Event::from("baz"),
            Event::from("bat"),
        ])
        .map(Ok);
        let sink = Vec::from("(foo)");
        let mut framed = FramedWrite::new(sink, encoder);
        source.forward(&mut framed).await.unwrap();
        let sink = framed.into_inner();
        assert_eq!(sink, b"(foo)(bar)(baz)(bat)");
    }

    #[tokio::test]
    async fn test_encode_events_sink_empty_handle_framing_error() {
        let encoder = Encoder::new(
            Box::new(ErrorNthEncoder::new(ParenEncoder::new(), 1)),
            Box::new(RawMessageSerializer::new()),
        );
        let source = futures::stream::iter(vec![
            Event::from("foo"),
            Event::from("bar"),
            Event::from("baz"),
        ])
        .map(Ok);
        let sink = Vec::new();
        let mut framed = FramedWrite::new(sink, encoder);
        assert!(source.forward(&mut framed).await.is_err());
        framed.flush().await.unwrap();
        let sink = framed.into_inner();
        assert_eq!(sink, b"(foo)");
    }

    #[tokio::test]
    async fn test_encode_events_sink_non_empty_handle_framing_error() {
        let encoder = Encoder::new(
            Box::new(ErrorNthEncoder::new(ParenEncoder::new(), 1)),
            Box::new(RawMessageSerializer::new()),
        );
        let source = futures::stream::iter(vec![
            Event::from("bar"),
            Event::from("baz"),
            Event::from("bat"),
        ])
        .map(Ok);
        let sink = Vec::from("(foo)");
        let mut framed = FramedWrite::new(sink, encoder);
        assert!(source.forward(&mut framed).await.is_err());
        framed.flush().await.unwrap();
        let sink = framed.into_inner();
        assert_eq!(sink, b"(foo)(bar)");
    }
}
