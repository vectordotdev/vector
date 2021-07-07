use crate::{event::Event, internal_events::DecoderParseFailed};
use bytes::{Bytes, BytesMut};

pub trait Parser {
    fn parse(&self, bytes: Bytes) -> crate::Result<Event>;
}

pub struct Decoder<P: Parser> {
    framer: Box<dyn tokio_util::codec::Decoder<Item = Bytes, Error = std::io::Error> + Send + Sync>,
    parser: P,
}

impl<P> tokio_util::codec::Decoder for Decoder<P>
where
    P: Parser,
{
    type Item = (Event, usize);
    type Error = std::io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, std::io::Error> {
        self.framer.decode(buf).map(|frame| {
            frame.and_then(|frame| match self.parser.parse(frame) {
                Ok(event) => Some((event, frame.len())),
                Err(error) => {
                    emit!(DecoderParseFailed { error });
                    None
                }
            })
        })
    }

    fn decode_eof(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, std::io::Error> {
        self.framer.decode_eof(buf).map(|frame| {
            frame.and_then(|frame| match self.parser.parse(frame) {
                Ok(event) => Some((event, frame.len())),
                Err(error) => {
                    emit!(DecoderParseFailed { error });
                    None
                }
            })
        })
    }
}
