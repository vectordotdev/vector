use super::Parser;
use crate::event::Event;
use bytes::Bytes;

pub struct BytesParser;

impl Parser for BytesParser {
    fn parse(&self, bytes: Bytes) -> crate::Result<Event> {
        Ok(bytes.into())
    }
}
