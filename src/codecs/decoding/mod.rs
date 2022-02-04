//! A collection of support structures that are used in the process of decoding
//! bytes into events.

pub mod format;
pub mod framing;

pub use format::{
    BoxedDeserializer, BytesDeserializer, BytesDeserializerConfig, JsonDeserializer,
    JsonDeserializerConfig,
};
#[cfg(feature = "sources-syslog")]
pub use format::{SyslogDeserializer, SyslogDeserializerConfig};
pub use framing::{
    BoxedFramer, BoxedFramingError, BytesDecoder, BytesDecoderConfig, CharacterDelimitedDecoder,
    CharacterDelimitedDecoderConfig, FramingError, LengthDelimitedDecoder,
    LengthDelimitedDecoderConfig, NewlineDelimitedDecoder, NewlineDelimitedDecoderConfig,
    OctetCountingDecoder, OctetCountingDecoderConfig,
};

use bytes::{Bytes, BytesMut};
use format::Deserializer as _;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::fmt::Debug;

use crate::{
    event::Event,
    internal_events::{DecoderDeserializeFailed, DecoderFramingFailed},
    sources::util::StreamDecodingError,
};

/// An error that occurred while decoding structured events from a byte stream /
/// byte messages.
#[derive(Debug)]
pub enum Error {
    /// The error occurred while producing byte frames from the byte stream /
    /// byte messages.
    FramingError(BoxedFramingError),
    /// The error occurred while parsing structured events from a byte frame.
    ParsingError(crate::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FramingError(error) => write!(formatter, "FramingError({})", error),
            Self::ParsingError(error) => write!(formatter, "ParsingError({})", error),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::FramingError(Box::new(error))
    }
}

impl StreamDecodingError for Error {
    fn can_continue(&self) -> bool {
        match self {
            Self::FramingError(error) => error.can_continue(),
            Self::ParsingError(_) => true,
        }
    }
}

/// Configuration for building a `Framer`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum FramingConfig {
    /// Configures the `BytesDecoder`.
    Bytes(BytesDecoderConfig),
    /// Configures the `CharacterDelimitedDecoder`.
    CharacterDelimited(CharacterDelimitedDecoderConfig),
    /// Configures the `LengthDelimitedDecoder`.
    LengthDelimited(LengthDelimitedDecoderConfig),
    /// Configures the `NewlineDelimitedDecoder`.
    NewlineDelimited(NewlineDelimitedDecoderConfig),
    /// Configures the `OctetCountingDecoder`.
    OctetCounting(OctetCountingDecoderConfig),
}

impl FramingConfig {
    fn build(&self) -> Framer {
        match self {
            FramingConfig::Bytes(config) => Framer::Bytes(config.build()),
            FramingConfig::CharacterDelimited(config) => Framer::CharacterDelimited(config.build()),
            FramingConfig::LengthDelimited(config) => Framer::LengthDelimited(config.build()),
            FramingConfig::NewlineDelimited(config) => Framer::NewlineDelimited(config.build()),
            FramingConfig::OctetCounting(config) => Framer::OctetCounting(config.build()),
        }
    }
}

/// Produce byte frames from a byte stream / byte message.
#[derive(Debug, Clone)]
pub enum Framer {
    /// Uses a `BytesDecoder` for framing.
    Bytes(BytesDecoder),
    /// Uses a `CharacterDelimitedDecoder` for framing.
    CharacterDelimited(CharacterDelimitedDecoder),
    /// Uses a `LengthDelimitedDecoder` for framing.
    LengthDelimited(LengthDelimitedDecoder),
    /// Uses a `NewlineDelimitedDecoder` for framing.
    NewlineDelimited(NewlineDelimitedDecoder),
    /// Uses a `OctetCountingDecoder` for framing.
    OctetCounting(OctetCountingDecoder),
    /// Uses an opaque `Framer` implementation for framing.
    Boxed(BoxedFramer),
}

impl tokio_util::codec::Decoder for Framer {
    type Item = Bytes;
    type Error = BoxedFramingError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        match self {
            Framer::Bytes(framer) => framer.decode(src),
            Framer::CharacterDelimited(framer) => framer.decode(src),
            Framer::LengthDelimited(framer) => framer.decode(src),
            Framer::NewlineDelimited(framer) => framer.decode(src),
            Framer::OctetCounting(framer) => framer.decode(src),
            Framer::Boxed(framer) => framer.decode(src),
        }
    }

    fn decode_eof(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        match self {
            Framer::Bytes(framer) => framer.decode_eof(src),
            Framer::CharacterDelimited(framer) => framer.decode_eof(src),
            Framer::LengthDelimited(framer) => framer.decode_eof(src),
            Framer::NewlineDelimited(framer) => framer.decode_eof(src),
            Framer::OctetCounting(framer) => framer.decode_eof(src),
            Framer::Boxed(framer) => framer.decode_eof(src),
        }
    }
}

/// Configuration for building a `Deserializer`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "codec", rename_all = "snake_case")]
pub enum DeserializerConfig {
    /// Configures the `BytesDeserializer`.
    Bytes(BytesDeserializerConfig),
    /// Configures the `JsonDeserializer`.
    Json(JsonDeserializerConfig),
    #[cfg(feature = "sources-syslog")]
    /// Configures the `SyslogDeserializer`.
    Syslog(SyslogDeserializerConfig),
}

impl DeserializerConfig {
    fn build(&self) -> Deserializer {
        match self {
            DeserializerConfig::Bytes(config) => Deserializer::Bytes(config.build()),
            DeserializerConfig::Json(config) => Deserializer::Json(config.build()),
            #[cfg(feature = "sources-syslog")]
            DeserializerConfig::Syslog(config) => Deserializer::Syslog(config.build()),
        }
    }
}

/// Parse structured events from bytes.
#[derive(Debug, Clone)]
pub enum Deserializer {
    /// Uses a `BytesDeserializer` for deserialization.
    Bytes(BytesDeserializer),
    /// Uses a `JsonDeserializer` for deserialization.
    Json(JsonDeserializer),
    #[cfg(feature = "sources-syslog")]
    /// Uses a `SyslogDeserializer` for deserialization.
    Syslog(SyslogDeserializer),
    /// Uses an opaque `Deserializer` implementation for deserialization.
    Boxed(BoxedDeserializer),
}

impl format::Deserializer for Deserializer {
    fn parse(&self, bytes: Bytes) -> crate::Result<SmallVec<[Event; 1]>> {
        match self {
            Deserializer::Bytes(deserializer) => deserializer.parse(bytes),
            Deserializer::Json(deserializer) => deserializer.parse(bytes),
            #[cfg(feature = "sources-syslog")]
            Deserializer::Syslog(deserializer) => deserializer.parse(bytes),
            Deserializer::Boxed(deserializer) => deserializer.parse(bytes),
        }
    }
}

/// A decoder that can decode structured events from a byte stream / byte
/// messages.
#[derive(Debug, Clone)]
pub struct Decoder {
    framer: Framer,
    deserializer: Deserializer,
}

impl Default for Decoder {
    fn default() -> Self {
        Self {
            framer: Framer::NewlineDelimited(NewlineDelimitedDecoder::new()),
            deserializer: Deserializer::Bytes(BytesDeserializer::new()),
        }
    }
}

impl Decoder {
    /// Creates a new `Decoder` with the specified `Framer` to produce byte
    /// frames from the byte stream / byte messages and `Deserializer` to parse
    /// structured events from a byte frame.
    pub const fn new(framer: Framer, deserializer: Deserializer) -> Self {
        Self {
            framer,
            deserializer,
        }
    }

    /// Handles the framing result and parses it into a structured event, if
    /// possible.
    ///
    /// Emits logs if either framing or parsing failed.
    fn handle_framing_result(
        &mut self,
        frame: Result<Option<Bytes>, BoxedFramingError>,
    ) -> Result<Option<(SmallVec<[Event; 1]>, usize)>, Error> {
        let frame = frame.map_err(|error| {
            emit!(&DecoderFramingFailed { error: &error });
            Error::FramingError(error)
        })?;

        let frame = match frame {
            Some(frame) => frame,
            _ => return Ok(None),
        };

        let byte_size = frame.len();

        // Parse structured events from the byte frame.
        self.deserializer
            .parse(frame)
            .map(|event| Some((event, byte_size)))
            .map_err(|error| {
                emit!(&DecoderDeserializeFailed { error: &error });
                Error::ParsingError(error)
            })
    }
}

impl tokio_util::codec::Decoder for Decoder {
    type Item = (SmallVec<[Event; 1]>, usize);
    type Error = Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let frame = self.framer.decode(buf);
        self.handle_framing_result(frame)
    }

    fn decode_eof(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let frame = self.framer.decode_eof(buf);
        self.handle_framing_result(frame)
    }
}

/// Config used to build a `Decoder`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DecodingConfig {
    /// The framing config.
    framing: FramingConfig,
    /// The decoding config.
    decoding: DeserializerConfig,
}

impl DecodingConfig {
    /// Creates a new `DecodingConfig` with the provided `FramingConfig` and
    /// `DeserializerConfig`.
    pub const fn new(framing: FramingConfig, decoding: DeserializerConfig) -> Self {
        Self { framing, decoding }
    }

    /// Builds a `Decoder` from the provided configuration.
    pub fn build(&self) -> crate::Result<Decoder> {
        // Build the framer.
        let framer = self.framing.build();

        // Build the deserializer.
        let deserializer = self.decoding.build();

        Ok(Decoder::new(framer, deserializer))
    }
}
