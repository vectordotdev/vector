mod config;
mod framing;
mod parsers;

use crate::{event::Event, internal_events::DecoderParseFailed, sources::util::TcpIsErrorFatal};
use bytes::{Bytes, BytesMut};
pub use config::{DecodingConfig, FramingConfig};
pub use framing::OctetCountingDecoder;
pub use parsers::BytesParser;
use tokio_util::codec::LinesCodecError;

pub trait Parser {
    fn parse(&self, bytes: Bytes) -> crate::Result<Event>;
}

pub trait DecoderError: std::error::Error + TcpIsErrorFatal + Send + Sync {}

#[derive(Debug)]
pub struct Error {
    error: std::sync::Arc<dyn DecoderError>,
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.error)
    }
}

impl TcpIsErrorFatal for Error {
    fn is_error_fatal(&self) -> bool {
        self.error.is_error_fatal()
    }
}

impl DecoderError for std::io::Error {}

impl DecoderError for LinesCodecError {}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self {
            error: std::sync::Arc::new(error),
        }
    }
}

impl From<LinesCodecError> for Error {
    fn from(error: LinesCodecError) -> Self {
        Self {
            error: std::sync::Arc::new(error),
        }
    }
}

pub struct Decoder {
    framer: Box<dyn tokio_util::codec::Decoder<Item = Bytes, Error = Error> + Send + Sync>,
    parser: Box<dyn Parser + Send + Sync>,
}

impl Decoder {
    pub fn new(
        framer: Box<
            dyn tokio_util::codec::Decoder<Item = Bytes, Error = Error> + Send + Sync + 'static,
        >,
        parser: Box<dyn Parser + Send + Sync + 'static>,
    ) -> Self {
        Self { framer, parser }
    }
}

impl tokio_util::codec::Decoder for Decoder {
    type Item = (Event, usize);
    type Error = self::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        self.framer.decode(buf).map(|result| {
            result.and_then(|frame| {
                let byte_size = frame.len();
                match self.parser.parse(frame) {
                    Ok(event) => Some((event, byte_size)),
                    Err(error) => {
                        emit!(DecoderParseFailed { error });
                        None
                    }
                }
            })
        })
    }

    fn decode_eof(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        self.framer.decode_eof(buf).map(|result| {
            result.and_then(|frame| {
                let byte_size = frame.len();
                match self.parser.parse(frame) {
                    Ok(event) => Some((event, byte_size)),
                    Err(error) => {
                        emit!(DecoderParseFailed { error });
                        None
                    }
                }
            })
        })
    }
}

pub struct BytesDecoder<Error: From<std::io::Error> + Into<self::Error>, Item: Into<Bytes> = Bytes>
{
    decoder:
        Box<dyn tokio_util::codec::Decoder<Item = Item, Error = Error> + Send + Sync + 'static>,
}

impl<Error: From<std::io::Error> + Into<self::Error>, Item: Into<Bytes>> BytesDecoder<Error, Item> {
    pub fn new(
        decoder: impl tokio_util::codec::Decoder<Item = Item, Error = Error> + Send + Sync + 'static,
    ) -> Self {
        Self {
            decoder: Box::new(decoder),
        }
    }
}

impl<Error: From<std::io::Error> + Into<self::Error>, Item: Into<Bytes>> tokio_util::codec::Decoder
    for BytesDecoder<Error, Item>
{
    type Item = Bytes;
    type Error = self::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        self.decoder
            .decode(buf)
            .map(|result| result.map(|frame| frame.into()))
            .map_err(|error| error.into())
    }

    fn decode_eof(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        self.decoder
            .decode_eof(buf)
            .map(|result| result.map(|frame| frame.into()))
            .map_err(|error| error.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::log_schema;
    use tokio_util::codec::{Decoder, LinesCodec};

    #[tokio::test]
    async fn basic_decoder() {
        let mut decoder = super::Decoder::new(
            Box::new(BytesDecoder::new(LinesCodec::new())),
            Box::new(BytesParser),
        );
        let mut input = BytesMut::from("foo\nbar\nbaz");

        let mut events = Vec::new();
        while let Some(event) = decoder.decode_eof(&mut input).unwrap() {
            events.push(event);
        }

        assert_eq!(events.len(), 3);
        assert_eq!(
            events[0].0.as_log()[log_schema().message_key()],
            "foo".into()
        );
        assert!(events[0]
            .0
            .as_log()
            .get(log_schema().timestamp_key())
            .is_some());
        assert_eq!(events[0].1, 3);
        assert_eq!(
            events[1].0.as_log()[log_schema().message_key()],
            "bar".into()
        );
        assert!(events[1]
            .0
            .as_log()
            .get(log_schema().timestamp_key())
            .is_some());
        assert_eq!(events[1].1, 3);
        assert_eq!(
            events[2].0.as_log()[log_schema().message_key()],
            "baz".into()
        );
        assert!(events[2]
            .0
            .as_log()
            .get(log_schema().timestamp_key())
            .is_some());
        assert_eq!(events[2].1, 3);
    }
}
