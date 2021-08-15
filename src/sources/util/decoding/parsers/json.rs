use crate::{
    config::log_schema,
    event::Event,
    sources::util::decoding::{BoxedParser, Parser, ParserConfig},
};
use bytes::Bytes;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::convert::TryInto;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct JsonParserConfig;

impl JsonParserConfig {
    pub fn new() -> Self {
        Self
    }
}

#[typetag::serde(name = "json")]
impl ParserConfig for JsonParserConfig {
    fn build(&self) -> BoxedParser {
        Box::new(JsonParser)
    }
}

#[derive(Debug, Clone)]
pub struct JsonParser;

impl Parser for JsonParser {
    fn parse(&self, bytes: Bytes) -> crate::Result<Event> {
        let json: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|error| format!("Error parsing JSON: {:?}", error))?;
        let mut event: Event = json.try_into()?;
        event
            .as_mut_log()
            .insert(log_schema().timestamp_key(), Utc::now());
        Ok(event)
    }
}
