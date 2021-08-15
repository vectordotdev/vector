mod config;
mod framing;
mod parsers;

use crate::{
    event::Event,
    internal_events::{DecoderFramingFailed, DecoderParseFailed},
    sources::util::TcpError,
};
use bytes::{Bytes, BytesMut};
pub use config::{DecodingConfig, FramingConfig, ParserConfig};
use dyn_clone::DynClone;
pub use framing::*;
pub use parsers::*;
use smallvec::SmallVec;
use tokio_util::codec::LinesCodecError;

pub trait Framer:
    tokio_util::codec::Decoder<Item = Bytes, Error = BoxedFramingError> + DynClone + Send + Sync
{
}

impl<Decoder> Framer for Decoder where
    Decoder:
        tokio_util::codec::Decoder<Item = Bytes, Error = BoxedFramingError> + Clone + Send + Sync
{
}

dyn_clone::clone_trait_object!(Framer);

pub trait Parser: DynClone + Send + Sync {
    fn parse(&self, bytes: Bytes) -> crate::Result<SmallVec<[Event; 1]>>;
}

dyn_clone::clone_trait_object!(Parser);

pub trait FramingError: std::error::Error + TcpError + Send + Sync {}

pub type BoxedFramingError = Box<dyn FramingError>;

impl std::error::Error for BoxedFramingError {}

impl FramingError for std::io::Error {}

impl FramingError for LinesCodecError {}

impl From<std::io::Error> for BoxedFramingError {
    fn from(error: std::io::Error) -> Self {
        Box::new(error)
    }
}

impl From<LinesCodecError> for BoxedFramingError {
    fn from(error: LinesCodecError) -> Self {
        Box::new(error)
    }
}

#[derive(Debug)]
pub enum Error {
    FramingError(BoxedFramingError),
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

impl TcpError for Error {
    fn is_fatal(&self) -> bool {
        match self {
            Self::FramingError(error) => error.is_fatal(),
            Self::ParsingError(_) => false,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::FramingError(Box::new(error))
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

    fn decode(
        &mut self,
        buf: &mut BytesMut,
        decode_frame: impl Fn(
            &mut BoxedFramer,
            &mut BytesMut,
        ) -> Result<Option<Bytes>, BoxedFramingError>,
    ) -> Result<Option<(SmallVec<[Event; 1]>, usize)>, Error> {
        loop {
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

                match self.parser.parse(frame) {
                    Ok(event) => Ok(Some((event, byte_size))),
                    Err(error) => {
                        emit!(DecoderParseFailed { error: &error });
                        Err(Error::ParsingError(error))
                    }
                }
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
