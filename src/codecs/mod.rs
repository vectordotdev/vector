//! A collection of codecs that can be used to transform between bytes streams /
//! byte messages, byte frames and structured events.

#![deny(missing_docs)]

mod framers;
mod parsers;

use crate::{
    event::Event,
    internal_events::{DecoderFramingFailed, DecoderParseFailed},
    sources::util::TcpError,
};
use bytes::{Bytes, BytesMut};
pub use framers::*;
pub use parsers::*;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

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

impl TcpError for Error {
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
    parser: BoxedParser,
}

impl Default for Decoder {
    fn default() -> Self {
        Self {
            framer: Box::new(NewlineDelimitedCodec::new()),
            parser: Box::new(BytesParser::new()),
        }
    }
}

impl Decoder {
    /// Creates a new `Decoder` with the specified `Framer` to produce byte
    /// frames from the byte stream / byte messages and `Parser` to parse
    /// structured events from a byte frame.
    pub fn new(framer: BoxedFramer, parser: BoxedParser) -> Self {
        Self { framer, parser }
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
        self.parser
            .parse(frame)
            .map(|event| Some((event, byte_size)))
            .map_err(|error| {
                emit!(&DecoderParseFailed { error: &error });
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
///
/// Usually used in source configs via `#[serde(flatten)]`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct DecodingConfig {
    /// The framing config.
    framing: Option<Box<dyn FramingConfig>>,
    /// The decoding config.
    decoding: Option<Box<dyn ParserConfig>>,
}

impl DecodingConfig {
    /// Creates a new `DecodingConfig` with the provided `FramingConfig` and
    /// `ParserConfig`.
    pub fn new(
        framing: Option<Box<dyn FramingConfig>>,
        decoding: Option<Box<dyn ParserConfig>>,
    ) -> Self {
        Self { framing, decoding }
    }

    /// Builds a `Decoder` from the provided configuration.
    ///
    /// Fails if any of the provided `framing` or `decoding` configs fail to
    /// build.
    pub fn build(&self) -> crate::Result<Decoder> {
        // Build the framer or use a newline delimited decoder if not provided.
        let framer: BoxedFramer = self
            .framing
            .as_ref()
            .map(|config| config.build())
            .unwrap_or_else(|| NewlineDelimitedDecoderConfig::new().build())?;

        // Build the parser or use a plain bytes parser if not provided.
        let parser: BoxedParser = self
            .decoding
            .as_ref()
            .map(|config| config.build())
            .unwrap_or_else(|| BytesParserConfig::new().build())?;

        Ok(Decoder::new(framer, parser))
    }
}
