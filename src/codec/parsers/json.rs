use crate::{
    codec::{BoxedParser, Parser, ParserConfig},
    config::log_schema,
    event::Event,
};
use bytes::Bytes;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};
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
    fn parse(&self, bytes: Bytes) -> crate::Result<SmallVec<[Event; 1]>> {
        let json: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|error| format!("Error parsing JSON: {:?}", error))?;

        let mut events = match json {
            serde_json::Value::Array(values) => values
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<SmallVec<[Event; 1]>, _>>()?,
            _ => smallvec![json.try_into()?],
        };

        let timestamp = Utc::now();

        for event in &mut events {
            event
                .as_mut_log()
                .insert(log_schema().timestamp_key(), timestamp);
        }

        Ok(events)
    }
}
