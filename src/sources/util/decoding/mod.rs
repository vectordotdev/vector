mod decoders;

use crate::{
    config::log_schema,
    event::{Event, LogEvent, Value},
    internal_events::DecoderParseFailed,
};
use bytes::{Bytes, BytesMut};
pub use decoders::OctetCountingDecoder;

pub trait Parser {
    fn parse(&self, bytes: Bytes) -> crate::Result<Event>;
}

pub struct Decoder<
    Parser: super::decoding::Parser,
    Error: From<std::io::Error> = std::io::Error,
    Item: Into<Bytes> = Bytes,
> {
    framer: Box<dyn tokio_util::codec::Decoder<Item = Item, Error = Error> + Send + Sync>,
    parser: Parser,
}

impl<Parser, Error, Item> Decoder<Parser, Error, Item>
where
    Error: From<std::io::Error>,
    Parser: super::decoding::Parser,
    Item: Into<Bytes>,
{
    pub fn new(
        framer: Box<dyn tokio_util::codec::Decoder<Item = Item, Error = Error> + Send + Sync>,
        parser: Parser,
    ) -> Self {
        Self { framer, parser }
    }
}

impl<Parser, Error, Item> tokio_util::codec::Decoder for Decoder<Parser, Error, Item>
where
    Error: From<std::io::Error>,
    Parser: super::decoding::Parser,
    Item: Into<Bytes>,
{
    type Item = (Event, usize);
    type Error = Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        self.framer.decode(buf).map(|result| {
            result.and_then(|frame| {
                let bytes = frame.into();
                let byte_size = bytes.len();
                match self.parser.parse(bytes) {
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
                let bytes = frame.into();
                let byte_size = bytes.len();
                match self.parser.parse(bytes) {
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

pub struct BytesParser;

impl Parser for BytesParser {
    fn parse(&self, bytes: Bytes) -> crate::Result<Event> {
        let mut log = LogEvent::default();
        log.insert(log_schema().message_key(), Value::from(bytes));
        Ok(log.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::{assert_event_data_eq, btreemap};
    use tokio_util::codec::{Decoder, LinesCodec};

    #[tokio::test]
    async fn basic_decoder() {
        let mut decoder = super::Decoder::new(Box::new(LinesCodec::new()), BytesParser);
        let mut input = BytesMut::from("foo\nbar\nbaz");

        let mut events = Vec::new();
        while let Some(event) = decoder.decode_eof(&mut input).unwrap() {
            events.push(event);
        }

        assert_eq!(events.len(), 3);
        assert_event_data_eq!(
            events[0].0,
            Event::from(btreemap! {
                "message" => "foo",
            })
        );
        assert_eq!(events[0].1, 3);
        assert_event_data_eq!(
            events[1].0,
            Event::from(btreemap! {
                "message" => "bar",
            })
        );
        assert_eq!(events[1].1, 3);
        assert_event_data_eq!(
            events[2].0,
            Event::from(btreemap! {
                "message" => "baz",
            })
        );
        assert_eq!(events[2].1, 3);
    }
}
