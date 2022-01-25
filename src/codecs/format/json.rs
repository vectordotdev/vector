use std::convert::TryInto;

use bytes::{BufMut, Bytes};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};

use crate::{
    codecs::{
        decoding::{BoxedDeserializer, Deserializer, DeserializerConfig},
        encoding::{BoxedSerializer, Serializer, SerializerConfig},
    },
    config::log_schema,
    event::Event,
};

/// Config used to build a `JsonDeserializer`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct JsonDeserializerConfig;

#[typetag::serde(name = "json")]
impl DeserializerConfig for JsonDeserializerConfig {
    fn build(&self) -> crate::Result<BoxedDeserializer> {
        Ok(Box::new(Into::<JsonDeserializer>::into(self)))
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

impl From<&JsonDeserializerConfig> for JsonDeserializer {
    fn from(_: &JsonDeserializerConfig) -> Self {
        Self
    }
}

/// Config used to build a `JsonSerializer`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct JsonSerializerConfig;

impl JsonSerializerConfig {
    /// Creates a new `JsonSerializerConfig`.
    pub const fn new() -> Self {
        Self
    }
}

#[typetag::serde(name = "json")]
impl SerializerConfig for JsonSerializerConfig {
    fn build(&self) -> crate::Result<BoxedSerializer> {
        Ok(Box::new(JsonSerializer))
    }
}

/// Serializer that converts an `Event` to bytes using the JSON format.
#[derive(Debug, Clone)]
pub struct JsonSerializer;

impl JsonSerializer {
    /// Creates a new `JsonSerializer`.
    pub const fn new() -> Self {
        Self
    }
}

impl Serializer for JsonSerializer {
    fn serialize(&self, event: Event, buffer: &mut bytes::BytesMut) -> crate::Result<()> {
        let writer = buffer.writer();
        match event {
            Event::Log(log) => serde_json::to_writer(writer, &log),
            Event::Metric(metric) => serde_json::to_writer(writer, &metric),
        }
        .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::log_schema;
    use crate::event::Value;
    use bytes::BytesMut;
    use shared::btreemap;

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
    fn serialize_json() {
        let event = Event::from(btreemap! {
            "foo" => Value::from("bar")
        });
        let serializer = JsonSerializer::new();
        let mut bytes = BytesMut::new();

        serializer.serialize(event, &mut bytes).unwrap();

        assert_eq!(bytes.freeze(), r#"{"foo":"bar"}"#);
    }

    #[test]
    fn skip_empty() {
        let input = Bytes::from("");
        let deserializer = JsonDeserializer::new();

        let events = deserializer.parse(input).unwrap();
        let mut events = events.into_iter();

        assert_eq!(events.next(), None);
    }

    #[test]
    fn error_invalid_json() {
        let input = Bytes::from("{ foo");
        let deserializer = JsonDeserializer::new();

        assert!(deserializer.parse(input).is_err());
    }
}
