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
pub struct JsonParserConfig {
    #[serde(default)]
    json: JsonParserOptions,
}

/// Options for building a `JsonParser`.
#[derive(Debug, Clone, Derivative, Deserialize, Serialize)]
#[derivative(Default)]
pub struct JsonParserOptions {
    #[serde(default = "crate::serde::default_true")]
    #[derivative(Default(value = "true"))]
    skip_empty: bool,
}

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

    /// Creates a new `JsonParserConfig` with the provided options.
    pub const fn new_with_options(skip_empty: bool) -> Self {
        Self {
            json: JsonParserOptions { skip_empty },
        }
    }
}

/// Parser that builds `Event`s from a byte frame containing JSON.
#[derive(Debug, Clone, Default)]
pub struct JsonParser {
    skip_empty: bool,
}

impl JsonParser {
    /// Creates a new `JsonParser`.
    pub fn new() -> Self {
        Default::default()
    }

    /// Creates a new `JsonParser` with the provided options.
    pub const fn new_with_options(skip_empty: bool) -> Self {
        Self { skip_empty }
    }
}

impl Parser for JsonParser {
    fn parse(&self, bytes: Bytes) -> crate::Result<SmallVec<[Event; 1]>> {
        if self.skip_empty && bytes.is_empty() {
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
    fn from(config: &JsonParserConfig) -> Self {
        Self {
            skip_empty: config.json.skip_empty,
        }
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
        let parser = JsonParser::new_with_options(true);

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

    #[test]
    fn error_empty() {
        let input = Bytes::from("");
        let parser = JsonParser::new_with_options(false);

        assert!(parser.parse(input).is_err());
    }
}
