// TODO.
#![allow(missing_docs)]

use crate::{
    codec::{BoxedParser, Parser, ParserConfig},
    event::Event,
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};

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

#[derive(Debug, Clone)]
pub struct BytesParser;

impl Parser for BytesParser {
    fn parse(&self, bytes: Bytes) -> crate::Result<SmallVec<[Event; 1]>> {
        Ok(smallvec![bytes.into()])
    }
}
