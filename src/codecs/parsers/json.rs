use crate::{
    codecs::{BoxedParser, Parser, ParserConfig},
    config::log_schema,
    event::Event,
};
use bytes::Bytes;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};
use std::convert::TryInto;

/// Config used to build a `JsonParser`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct JsonParserConfig;

#[typetag::serde(name = "json")]
impl ParserConfig for JsonParserConfig {
    fn build(&self) -> crate::Result<BoxedParser> {
        Ok(Box::new(Into::<JsonParser>::into(self)))
    }
}

impl JsonParserConfig {
    /// Creates a new `JsonParserConfig`.
    pub fn new() -> Self {
        Default::default()
    }
}

/// Parser that builds `Event`s from a byte frame containing JSON.
#[derive(Debug, Clone, Default)]
pub struct JsonParser;

impl JsonParser {
    /// Creates a new `JsonParser`.
    pub fn new() -> Self {
        Default::default()
    }
}

impl Parser for JsonParser {
    fn parse(&self, bytes: Bytes) -> crate::Result<SmallVec<[Event; 1]>> {
        // It's common to receive empty frames when parsing NDJSON, since it
        // allows multiple empty newlines. We proceed without a warning here.
        if bytes.is_empty() {
            return Ok(smallvec![]);
        }

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
            let log = event.as_mut_log();
            let timestamp_key = log_schema().timestamp_key();

            if !log.contains(timestamp_key) {
                log.insert(timestamp_key, timestamp);
            }
        }

        Ok(events)
    }
}

impl From<&JsonParserConfig> for JsonParser {
    fn from(_: &JsonParserConfig) -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::log_schema;

    #[test]
    fn parse_json() {
        let input = Bytes::from(r#"{ "foo": 123 }"#);
        let parser = JsonParser::new();

        let events = parser.parse(input).unwrap();
        let mut events = events.into_iter();

        {
            let event = events.next().unwrap();
            let log = event.as_log();
            assert_eq!(log["foo"], 123.into());
            assert!(log.get(log_schema().timestamp_key()).is_some());
        }

        assert_eq!(events.next(), None);
    }

    #[test]
    fn parse_json_array() {
        let input = Bytes::from(r#"[{ "foo": 123 }, { "bar": 456 }]"#);
        let parser = JsonParser::new();

        let events = parser.parse(input).unwrap();
        let mut events = events.into_iter();

        {
            let event = events.next().unwrap();
            let log = event.as_log();
            assert_eq!(log["foo"], 123.into());
            assert!(log.get(log_schema().timestamp_key()).is_some());
        }

        {
            let event = events.next().unwrap();
            let log = event.as_log();
            assert_eq!(log["bar"], 456.into());
            assert!(log.get(log_schema().timestamp_key()).is_some());
        }

        assert_eq!(events.next(), None);
    }

    #[test]
    fn skip_empty() {
        let input = Bytes::from("");
        let parser = JsonParser::new();

        let events = parser.parse(input).unwrap();
        let mut events = events.into_iter();

        assert_eq!(events.next(), None);
    }

    #[test]
    fn error_invalid_json() {
        let input = Bytes::from("{ foo");
        let parser = JsonParser::new();

        assert!(parser.parse(input).is_err());
    }
}
