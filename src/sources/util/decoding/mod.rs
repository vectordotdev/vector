mod config;
mod framing;
mod parsers;

use crate::{event::Event, internal_events::DecoderParseFailed, sources::util::TcpError};
use bytes::{Bytes, BytesMut};
pub use config::{DecodingConfig, FramingConfig, ParserConfig};
use dyn_clone::DynClone;
pub use framing::*;
pub use parsers::*;
use std::sync::Arc;
use tokio_util::codec::LinesCodecError;

pub trait Framer:
    tokio_util::codec::Decoder<Item = Bytes, Error = Error> + DynClone + Send + Sync
{
}

impl<Decoder> Framer for Decoder where
    Decoder: tokio_util::codec::Decoder<Item = Bytes, Error = Error> + Clone + Send + Sync
{
}

dyn_clone::clone_trait_object!(Framer);

pub trait Parser: DynClone + Send + Sync {
    fn parse(&self, bytes: Bytes) -> crate::Result<Event>;
}

dyn_clone::clone_trait_object!(Parser);

pub trait DecoderError: std::error::Error + TcpError + Send + Sync {}

#[derive(Debug)]
pub struct Error {
    error: Arc<dyn DecoderError>,
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.error)
    }
}

impl std::error::Error for Error {}

impl TcpError for Error {
    fn is_fatal(&self) -> bool {
        self.error.is_fatal()
    }
}

impl DecoderError for std::io::Error {}

impl DecoderError for LinesCodecError {}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self {
            error: Arc::new(error),
        }
    }
}

impl From<LinesCodecError> for Error {
    fn from(error: LinesCodecError) -> Self {
        Self {
            error: Arc::new(error),
        }
    }
}

pub type BoxedFramer = Box<dyn Framer + Send + Sync>;

pub type BoxedParser = Box<dyn Parser + Send + Sync + 'static>;

#[derive(Clone)]
pub struct Decoder {
    framer: BoxedFramer,
    parser: BoxedParser,
}

impl Decoder {
    pub fn new(framer: BoxedFramer, parser: BoxedParser) -> Self {
        Self { framer, parser }
    }
}

impl tokio_util::codec::Decoder for Decoder {
    type Item = (Vec<Event>, usize);
    type Error = Error;

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
