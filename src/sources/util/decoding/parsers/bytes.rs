use crate::{
    event::Event,
    sources::util::decoding::{BoxedParser, Parser, ParserConfig},
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct BytesParserConfig;

impl BytesParserConfig {
    pub fn new() -> Self {
        Self
    }
}

#[typetag::serde(name = "bytes")]
impl ParserConfig for BytesParserConfig {
    fn build(&self) -> BoxedParser {
        Box::new(BytesParser)
    }
}

pub struct BytesParser;

impl Parser for BytesParser {
    fn parse(&self, bytes: Bytes) -> crate::Result<Event> {
        Ok(bytes.into())
    }
}
