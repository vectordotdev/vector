//! A collection of codecs that can be used to transform between bytes streams /
//! byte messages, byte frames and structured events.

#![deny(missing_docs)]

mod framers;
mod parsers;

use crate::{
    event::Event,
    internal_events::{DecoderFramingFailed, DecoderParseFailed},
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

#[derive(Clone)]
/// A decoder that can decode structured events from a byte stream / byte
/// messages.
pub struct Decoder {
    framer: BoxedFramer,
    parser: BoxedParser,
}

impl Decoder {
    /// Creates a new `Decoder` with the specified `Framer` to produce byte
    /// frames from the byte stream / byte messages and `Parser` to parse
    /// structured events from a byte frame.
    pub fn new(framer: BoxedFramer, parser: BoxedParser) -> Self {
        Self { framer, parser }
    }

    /// Method to combine framing and parsing, such that an incoming byte stream
    /// / byte messages are transformed directly to structured events.
    ///
    /// Zero-byte frames are skipped without parsing.
    fn decode(
        &mut self,
        buf: &mut BytesMut,
        decode_frame: impl Fn(
            &mut BoxedFramer,
            &mut BytesMut,
        ) -> Result<Option<Bytes>, BoxedFramingError>,
    ) -> Result<Option<(SmallVec<[Event; 1]>, usize)>, Error> {
        loop {
            // Frame bytes from the incoming byte stream / byte messages.
            let frame = decode_frame(&mut self.framer, buf).map_err(|error| {
                emit!(DecoderFramingFailed { error: &error });
                Error::FramingError(error)
            })?;

            break if let Some(frame) = frame {
                let byte_size = frame.len();

                // Skip zero-sized frames.
                if byte_size == 0 {
                    continue;
                }

                // Parse structured events from the byte frame.
                self.parser
                    .parse(frame)
                    .map(|event| Some((event, byte_size)))
                    .map_err(|error| {
                        emit!(DecoderParseFailed { error: &error });
                        Error::ParsingError(error)
                    })
            } else {
                Ok(None)
            };
        }
    }
}

impl tokio_util::codec::Decoder for Decoder {
    type Item = (SmallVec<[Event; 1]>, usize);
    type Error = Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        self.decode(buf, |framer, buf| framer.decode(buf))
    }

    fn decode_eof(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        self.decode(buf, |framer, buf| framer.decode_eof(buf))
    }
}

/// Config used to build a `Decoder`.
///
/// Usually used in source configs via `#[serde(flatten)]`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
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
    pub fn build(&self) -> Decoder {
        // Build the framer or use a newline delimited decoder if not provided.
        let framer: BoxedFramer = self
            .framing
            .as_ref()
            .map(|config| config.build())
            .unwrap_or_else(|| NewlineDelimitedDecoderConfig::new().build());

        // Build the parser or use a plain byte parser if not provided.
        let parser: BoxedParser = self
            .decoding
            .as_ref()
            .map(|config| config.build())
            .unwrap_or_else(|| BytesParserConfig::new().build());

        Decoder::new(framer, parser)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::log_schema;
    use tokio_util::codec::Decoder;

    #[tokio::test]
    async fn basic_decoder() {
        let mut decoder = super::Decoder::new(
            Box::new(NewlineDelimitedCodec::new()),
            Box::new(BytesParser),
        );
        let mut input = BytesMut::from("foo\nbar\nbaz");

        let mut events = Vec::new();
        while let Some(next) = decoder.decode_eof(&mut input).unwrap() {
            events.push(next);
        }

        assert_eq!(events.len(), 3);
        assert_eq!(events[0].0.len(), 1);
        assert_eq!(
            events[0].0[0].as_log()[log_schema().message_key()],
            "foo".into()
        );
        assert!(events[0].0[0]
            .as_log()
            .get(log_schema().timestamp_key())
            .is_some());
        assert_eq!(events[0].1, 3);
        assert_eq!(events[1].0.len(), 1);
        assert_eq!(
            events[1].0[0].as_log()[log_schema().message_key()],
            "bar".into()
        );
        assert!(events[1].0[0]
            .as_log()
            .get(log_schema().timestamp_key())
            .is_some());
        assert_eq!(events[1].1, 3);
        assert_eq!(events[2].0.len(), 1);
        assert_eq!(
            events[2].0[0].as_log()[log_schema().message_key()],
            "baz".into()
        );
        assert!(events[2].0[0]
            .as_log()
            .get(log_schema().timestamp_key())
            .is_some());
        assert_eq!(events[2].1, 3);
    }
}
