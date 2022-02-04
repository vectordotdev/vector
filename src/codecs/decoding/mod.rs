//! A collection of support structures that are used in the process of decoding
//! bytes into events.

pub mod format;
pub mod framing;

pub use format::{
    BoxedDeserializer, BytesDeserializer, BytesDeserializerConfig, Deserializer,
    DeserializerConfig, JsonDeserializer, JsonDeserializerConfig,
};
#[cfg(feature = "sources-syslog")]
pub use format::{SyslogDeserializer, SyslogDeserializerConfig};
pub use framing::{
    BoxedFramer, BoxedFramingError, BytesDecoder, BytesDecoderConfig, CharacterDelimitedDecoder,
    CharacterDelimitedDecoderConfig, Framer, FramingConfig, FramingError, LengthDelimitedDecoder,
    LengthDelimitedDecoderConfig, NewlineDelimitedDecoder, NewlineDelimitedDecoderConfig,
    OctetCountingDecoder, OctetCountingDecoderConfig,
};

use bytes::{Bytes, BytesMut};
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

#[derive(Debug, Clone)]
/// A decoder that can decode structured events from a byte stream / byte
/// messages.
pub struct Decoder {
    framer: BoxedFramer,
    deserializer: BoxedDeserializer,
}

impl Default for Decoder {
    fn default() -> Self {
        Self {
            framer: Box::new(NewlineDelimitedDecoder::new()),
            deserializer: Box::new(BytesDeserializer::new()),
        }
    }
}

impl Decoder {
    /// Creates a new `Decoder` with the specified `Framer` to produce byte
    /// frames from the byte stream / byte messages and `Deserializer` to parse
    /// structured events from a byte frame.
    pub fn new(framer: BoxedFramer, deserializer: BoxedDeserializer) -> Self {
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
    framing: Box<dyn FramingConfig>,
    /// The decoding config.
    decoding: Box<dyn DeserializerConfig>,
}

impl DecodingConfig {
    /// Creates a new `DecodingConfig` with the provided `FramingConfig` and
    /// `DeserializerConfig`.
    pub fn new(framing: Box<dyn FramingConfig>, decoding: Box<dyn DeserializerConfig>) -> Self {
        Self { framing, decoding }
    }

    /// Builds a `Decoder` from the provided configuration.
    pub fn build(&self) -> crate::Result<Decoder> {
        // Build the framer.
        let framer: BoxedFramer = self.framing.build()?;

        // Build the deserializer.
        let deserializer: BoxedDeserializer = self.decoding.build()?;

        Ok(Decoder::new(framer, deserializer))
    }
}
