//! A collection of support structures that are used in the process of encoding
//! events into bytes.

pub mod format;
pub mod framing;

pub use format::{
    BoxedSerializer, JsonSerializer, JsonSerializerConfig, RawMessageSerializer,
    RawMessageSerializerConfig,
};
pub use framing::{
    BoxedFramer, BoxedFramingError, CharacterDelimitedEncoder, CharacterDelimitedEncoderConfig,
    CharacterDelimitedEncoderOptions, NewlineDelimitedEncoder, NewlineDelimitedEncoderConfig,
};

use crate::{
    event::Event,
    internal_events::{EncoderFramingFailed, EncoderSerializeFailed},
    schema,
};
use bytes::BytesMut;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

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

/// Configuration for building a `Framer`.
// Unfortunately, copying options of the nested enum variants is necessary
// since `serde` doesn't allow `flatten`ing these:
// https://github.com/serde-rs/serde/issues/1402.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum FramingConfig {
    /// Configures the `CharacterDelimitedEncoder`.
    CharacterDelimited {
        /// Options for the character delimited encoder.
        character_delimited: CharacterDelimitedEncoderOptions,
    },
    /// Configures the `NewlineDelimitedEncoder`.
    NewlineDelimited,
}

impl From<CharacterDelimitedEncoderConfig> for FramingConfig {
    fn from(config: CharacterDelimitedEncoderConfig) -> Self {
        Self::CharacterDelimited {
            character_delimited: config.character_delimited,
        }
    }
}

impl From<NewlineDelimitedEncoderConfig> for FramingConfig {
    fn from(_: NewlineDelimitedEncoderConfig) -> Self {
        Self::NewlineDelimited
    }
}

impl FramingConfig {
    /// Build the `Framer` from this configuration.
    pub const fn build(self) -> Framer {
        match self {
            FramingConfig::CharacterDelimited {
                character_delimited,
            } => Framer::CharacterDelimited(
                CharacterDelimitedEncoderConfig {
                    character_delimited,
                }
                .build(),
            ),
            FramingConfig::NewlineDelimited => {
                Framer::NewlineDelimited(NewlineDelimitedEncoderConfig.build())
            }
        }
    }
}

/// Produce a byte stream from byte frames.
#[derive(Debug, Clone)]
pub enum Framer {
    /// Uses a `CharacterDelimitedEncoder` for framing.
    CharacterDelimited(CharacterDelimitedEncoder),
    /// Uses a `NewlineDelimitedEncoder` for framing.
    NewlineDelimited(NewlineDelimitedEncoder),
    /// Uses an opaque `Encoder` implementation for framing.
    Boxed(BoxedFramer),
}

impl From<CharacterDelimitedEncoder> for Framer {
    fn from(encoder: CharacterDelimitedEncoder) -> Self {
        Self::CharacterDelimited(encoder)
    }
}

impl From<NewlineDelimitedEncoder> for Framer {
    fn from(encoder: NewlineDelimitedEncoder) -> Self {
        Self::NewlineDelimited(encoder)
    }
}

impl From<BoxedFramer> for Framer {
    fn from(encoder: BoxedFramer) -> Self {
        Self::Boxed(encoder)
    }
}

impl tokio_util::codec::Encoder<()> for Framer {
    type Error = BoxedFramingError;

    fn encode(&mut self, _: (), dst: &mut BytesMut) -> Result<(), Self::Error> {
        match self {
            Framer::CharacterDelimited(framer) => framer.encode((), dst),
            Framer::NewlineDelimited(framer) => framer.encode((), dst),
            Framer::Boxed(framer) => framer.encode((), dst),
        }
    }
}

/// Configuration for building a `Serializer`.
// Unfortunately, copying options of the nested enum variants is necessary
// since `serde` doesn't allow `flatten`ing these:
// https://github.com/serde-rs/serde/issues/1402.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "codec", rename_all = "snake_case")]
pub enum SerializerConfig {
    /// Configures the `JsonSerializer`.
    Json,
    /// Configures the `RawMessageSerializer`.
    RawMessage,
}

impl From<JsonSerializerConfig> for SerializerConfig {
    fn from(_: JsonSerializerConfig) -> Self {
        Self::Json
    }
}

impl From<RawMessageSerializerConfig> for SerializerConfig {
    fn from(_: RawMessageSerializerConfig) -> Self {
        Self::RawMessage
    }
}

impl SerializerConfig {
    /// Build the `Serializer` from this configuration.
    pub const fn build(&self) -> Serializer {
        match self {
            SerializerConfig::Json => Serializer::Json(JsonSerializerConfig.build()),
            SerializerConfig::RawMessage => {
                Serializer::RawMessage(RawMessageSerializerConfig.build())
            }
        }
    }

    /// The schema required by the serializer.
    pub fn schema_requirement(&self) -> schema::Requirement {
        match self {
            SerializerConfig::Json => JsonSerializerConfig.schema_requirement(),
            SerializerConfig::RawMessage => RawMessageSerializerConfig.schema_requirement(),
        }
    }
}

/// Serialize structured events as bytes.
#[derive(Debug, Clone)]
pub enum Serializer {
    /// Uses a `JsonSerializer` for deserialization.
    Json(JsonSerializer),
    /// Uses a `RawMessageSerializer` for deserialization.
    RawMessage(RawMessageSerializer),
}

impl tokio_util::codec::Encoder<Event> for Serializer {
    type Error = crate::Error;

    fn encode(&mut self, item: Event, dst: &mut BytesMut) -> Result<(), Self::Error> {
        match self {
            Serializer::Json(serializer) => serializer.encode(item, dst),
            Serializer::RawMessage(serializer) => serializer.encode(item, dst),
        }
    }
}

#[derive(Debug, Clone)]
/// An encoder that can encode structured events into byte frames.
pub struct Encoder {
    framer: Framer,
    serializer: Serializer,
}

impl Default for Encoder {
    fn default() -> Self {
        Self {
            framer: Framer::NewlineDelimited(NewlineDelimitedEncoder::new()),
            serializer: Serializer::RawMessage(RawMessageSerializer::new()),
        }
    }
}

impl Encoder {
    /// Creates a new `Encoder` with the specified `Serializer` to produce bytes
    /// from a structured event, and the `Framer` to wrap these into a byte
    /// frame.
    pub const fn new(framer: Framer, serializer: Serializer) -> Self {
        Self { framer, serializer }
    }

    /// Get the framer.
    pub const fn framer(&self) -> &Framer {
        &self.framer
    }

    /// Get the serializer.
    pub const fn serializer(&self) -> &Serializer {
        &self.serializer
    }
}

impl tokio_util::codec::Encoder<Event> for Encoder {
    type Error = Error;

    fn encode(&mut self, item: Event, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        let len = buffer.len();

        let mut payload = buffer.split_off(len);

        // Serialize the event.
        self.serializer
            .encode(item, &mut payload)
            .map_err(|error| {
                emit!(&EncoderSerializeFailed { error: &error });
                Error::SerializingError(error)
            })?;

        // Frame the serialized event.
        self.framer.encode((), &mut payload).map_err(|error| {
            emit!(&EncoderFramingFailed { error: &error });
            Error::FramingError(error)
        })?;

        buffer.unsplit(payload);

        Ok(())
    }
}

/// Config used to build an `Encoder`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EncodingConfig {
    /// The framing config.
    framing: FramingConfig,
    /// The encoding config.
    encoding: SerializerConfig,
}

impl EncodingConfig {
    /// Creates a new `EncodingConfig` with the provided `FramingConfig` and
    /// `SerializerConfig`.
    pub const fn new(framing: FramingConfig, encoding: SerializerConfig) -> Self {
        Self { framing, encoding }
    }

    /// Builds an `Encoder` from the provided configuration.
    pub const fn build(self) -> Encoder {
        // Build the framer.
        let framer = self.framing.build();

        // Build the serializer.
        let serializer = self.encoding.build();

        Encoder::new(framer, serializer)
    }
}

#[cfg(test)]
mod tests {
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

    impl tokio_util::codec::Encoder<()> for ParenEncoder {
        type Error = BoxedFramingError;

        fn encode(&mut self, _: (), dst: &mut BytesMut) -> Result<(), Self::Error> {
            dst.reserve(2);
            let inner = dst.split();
            dst.put_u8(b'(');
            dst.unsplit(inner);
            dst.put_u8(b')');
            Ok(())
        }
    }

    #[derive(Debug, Clone)]
    struct ErrorNthEncoder<T>(T, usize, usize)
    where
        T: tokio_util::codec::Encoder<(), Error = BoxedFramingError>;

    impl<T> ErrorNthEncoder<T>
    where
        T: tokio_util::codec::Encoder<(), Error = BoxedFramingError>,
    {
        pub fn new(encoder: T, n: usize) -> Self {
            Self(encoder, 0, n)
        }
    }

    impl<T> tokio_util::codec::Encoder<()> for ErrorNthEncoder<T>
    where
        T: tokio_util::codec::Encoder<(), Error = BoxedFramingError>,
    {
        type Error = BoxedFramingError;

        fn encode(&mut self, _: (), dst: &mut BytesMut) -> Result<(), Self::Error> {
            self.0.encode((), dst)?;
            let result = if self.1 == self.2 {
                Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "error")) as _)
            } else {
                Ok(())
            };
            self.1 += 1;
            result
        }
    }

    #[tokio::test]
    async fn test_encode_events_sink_empty() {
        let encoder = Encoder::new(
            Framer::Boxed(Box::new(ParenEncoder::new())),
            Serializer::RawMessage(RawMessageSerializer::new()),
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
            Framer::Boxed(Box::new(ParenEncoder::new())),
            Serializer::RawMessage(RawMessageSerializer::new()),
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
            Framer::Boxed(Box::new(ErrorNthEncoder::new(ParenEncoder::new(), 1))),
            Serializer::RawMessage(RawMessageSerializer::new()),
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
            Framer::Boxed(Box::new(ErrorNthEncoder::new(ParenEncoder::new(), 1))),
            Serializer::RawMessage(RawMessageSerializer::new()),
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
