use super::Deserializer;
use bytes::Bytes;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};
use std::convert::TryInto;
use value::Kind;
use vector_core::{
    config::{log_schema, DataType},
    event::Event,
    schema,
};

/// Config used to build a `JsonDeserializer`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct JsonDeserializerConfig;

impl JsonDeserializerConfig {
    /// Build the `JsonDeserializer` from this configuration.
    pub fn build(&self) -> JsonDeserializer {
        Into::<JsonDeserializer>::into(self)
    }

    /// The data type of returned events
    pub fn output_type(&self) -> DataType {
        DataType::Log
    }

    /// The schema produced by the deserializer.
    pub fn schema_definition(&self) -> schema::Definition {
        schema::Definition::empty()
            .required_field(
                log_schema().timestamp_key(),
                // The JSON decoder will try to insert a new `timestamp`-type value into the
                // "timestamp_key" field, but only if that field doesn't already exist.
                Kind::json().or_timestamp(),
                Some("timestamp"),
            )
            .unknown_fields(Kind::json())
    }
}

impl JsonDeserializerConfig {
    /// Creates a new `JsonDeserializerConfig`.
    pub fn new() -> Self {
        Default::default()
    }
}

/// Deserializer that builds `Event`s from a byte frame containing JSON.
#[derive(Debug, Clone, Default)]
pub struct JsonDeserializer;

impl JsonDeserializer {
    /// Creates a new `JsonDeserializer`.
    pub fn new() -> Self {
        Default::default()
    }
}

impl Deserializer for JsonDeserializer {
    fn parse(&self, bytes: Bytes) -> vector_core::Result<SmallVec<[Event; 1]>> {
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

impl From<&JsonDeserializerConfig> for JsonDeserializer {
    fn from(_: &JsonDeserializerConfig) -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vector_core::config::log_schema;

    #[test]
    fn deserialize_json() {
        let input = Bytes::from(r#"{ "foo": 123 }"#);
        let deserializer = JsonDeserializer::new();

        let events = deserializer.parse(input).unwrap();
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
    fn deserialize_json_array() {
        let input = Bytes::from(r#"[{ "foo": 123 }, { "bar": 456 }]"#);
        let deserializer = JsonDeserializer::new();

        let events = deserializer.parse(input).unwrap();
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
    fn deserialize_skip_empty() {
        let input = Bytes::from("");
        let deserializer = JsonDeserializer::new();

        let events = deserializer.parse(input).unwrap();
        let mut events = events.into_iter();

        assert_eq!(events.next(), None);
    }

    #[test]
    fn deserialize_error_invalid_json() {
        let input = Bytes::from("{ foo");
        let deserializer = JsonDeserializer::new();

        assert!(deserializer.parse(input).is_err());
    }
}
