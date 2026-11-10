use std::io::Cursor;

use super::Deserializer;
use bytes::Bytes;
use chrono::Utc;
use derivative::Derivative;
use serde::Deserialize;
use smallvec::SmallVec;
use vector_core::config::DataType;
use vector_core::event::LogEvent;
use vector_core::{
    config::{LogNamespace, log_schema},
    event::Event,
    schema,
};
use vrl::prelude::Kind;

/// Deserializer that builds `Event`s from a byte frame containing `MessagePack`.
///
/// A frame may contain several concatenated `MessagePack` values (for example when the whole
/// stream is buffered as a single frame). Every value is decoded until the frame is exhausted,
/// and a top-level array is expanded into one event per element.
#[derive(Debug, Clone, Derivative)]
#[derivative(Default)]
pub struct MsgPackDeserializer;

impl MsgPackDeserializer {
    /// Create a `MsgPackDeserializer` instance.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

/// Config used to build an `MsgPackDeserializer`.
/// Note that currently there is no configuration for this decoder.
#[derive(Debug, Clone, Default)]
pub struct MsgPackDeserializerConfig;

impl MsgPackDeserializerConfig {
    /// Return the type of event build by this deserializer.
    #[must_use]
    pub fn output_type(&self) -> DataType {
        DataType::Log
    }

    /// The schema produced by the deserializer.
    #[must_use]
    pub fn schema_definition(&self, log_namespace: LogNamespace) -> schema::Definition {
        match log_namespace {
            LogNamespace::Legacy => {
                let mut definition =
                    schema::Definition::empty_legacy_namespace().unknown_fields(Kind::any());

                if let Some(timestamp_key) = log_schema().timestamp_key() {
                    definition = definition.try_with_field(
                        timestamp_key,
                        // The decoder will try to insert a new `timestamp`-type value into the
                        // "timestamp_key" field, but only if that field doesn't already exist.
                        Kind::any().or_timestamp(),
                        Some("timestamp"),
                    );
                }
                definition
            }
            LogNamespace::Vector => {
                schema::Definition::new_with_default_metadata(Kind::any(), [log_namespace])
            }
        }
    }
}

impl Deserializer for MsgPackDeserializer {
    fn parse(
        &self,
        bytes: Bytes,
        log_namespace: LogNamespace,
    ) -> vector_common::Result<SmallVec<[Event; 1]>> {
        let len = bytes.len() as u64;
        let mut deserializer = rmp_serde::Deserializer::new(Cursor::new(&bytes[..]));
        let mut events: SmallVec<[Event; 1]> = SmallVec::new();

        // Keep decoding until every byte of the frame has been consumed. A truncated or
        // malformed trailing value surfaces as an error rather than being silently dropped.
        while deserializer.position() < len {
            match vrl::value::Value::deserialize(&mut deserializer)? {
                vrl::value::Value::Array(array) => events.extend(
                    array
                        .into_iter()
                        .map(|elem| Event::Log(LogEvent::from(elem))),
                ),
                value => events.push(Event::Log(LogEvent::from(value))),
            }
        }
        let events = match log_namespace {
            LogNamespace::Vector => events,
            LogNamespace::Legacy => {
                let timestamp = Utc::now();

                if let Some(timestamp_key) = log_schema().timestamp_key_target_path() {
                    for event in &mut events {
                        let log = event.as_mut_log();
                        if !log.contains(timestamp_key) {
                            log.insert(timestamp_key, timestamp);
                        }
                    }
                }

                events
            }
        };

        Ok(events)
    }
}
#[cfg(test)]
mod tests {
    use vector_core::config::log_schema;
    use vrl::core::Value;

    use super::*;

    #[test]
    fn deserialize_msgpack() {
        let input = Bytes::from_static(b"\x81\xA3\x66\x6F\x6F\x7B");
        let deserializer = MsgPackDeserializer;

        for namespace in [LogNamespace::Legacy, LogNamespace::Vector] {
            let events = deserializer.parse(input.clone(), namespace).unwrap();
            let mut events = events.into_iter();

            {
                let event = events.next().unwrap();
                let log = event.as_log();
                assert_eq!(log["foo"], 123.into());
                assert_eq!(
                    log.get((
                        lookup::PathPrefix::Event,
                        log_schema().timestamp_key().unwrap()
                    ))
                    .is_some(),
                    namespace == LogNamespace::Legacy
                );
            }

            assert_eq!(events.next(), None);
        }
    }

    #[test]
    fn deserialize_non_object_vector_namespace() {
        let input = Bytes::from_static(b"\xC0");
        let deserializer = MsgPackDeserializer;

        let namespace = LogNamespace::Vector;
        let events = deserializer.parse(input, namespace).unwrap();
        let mut events = events.into_iter();

        let event = events.next().unwrap();
        let log = event.as_log();
        assert_eq!(log["."], Value::Null);

        assert_eq!(events.next(), None);
    }

    #[test]
    fn deserialize_msgpack_array() {
        let input =
            Bytes::from_static(b"\x92\x81\xA3\x66\x6F\x6F\x7B\x81\xA3\x62\x61\x72\xCD\x01\xC8");
        let deserializer = MsgPackDeserializer;
        for namespace in [LogNamespace::Legacy, LogNamespace::Vector] {
            let events = deserializer.parse(input.clone(), namespace).unwrap();
            let mut events = events.into_iter();

            {
                let event = events.next().unwrap();
                let log = event.as_log();
                assert_eq!(log["foo"], 123.into());
                assert_eq!(
                    log.get((
                        lookup::PathPrefix::Event,
                        log_schema().timestamp_key().unwrap()
                    ))
                    .is_some(),
                    namespace == LogNamespace::Legacy
                );
            }

            {
                let event = events.next().unwrap();
                let log = event.as_log();
                assert_eq!(log["bar"], 456.into());
                assert_eq!(
                    log.get(log_schema().timestamp_key_target_path().unwrap())
                        .is_some(),
                    namespace == LogNamespace::Legacy
                );
            }

            assert_eq!(events.next(), None);
        }
    }

    #[test]
    fn deserialize_concatenated_msgpack_values() {
        // {"foo": 123} followed by [{"bar": 456}, {"baz": 789}]
        let input = Bytes::from_static(
            b"\x81\xA3\x66\x6F\x6F\x7B\x92\x81\xA3\x62\x61\x72\xCD\x01\xC8\x81\xA3\x62\x61\x7A\xCD\x03\x15",
        );
        let deserializer = MsgPackDeserializer;

        for namespace in [LogNamespace::Legacy, LogNamespace::Vector] {
            let events = deserializer.parse(input.clone(), namespace).unwrap();
            assert_eq!(events.len(), 3);
            assert_eq!(events[0].as_log()["foo"], 123.into());
            assert_eq!(events[1].as_log()["bar"], 456.into());
            assert_eq!(events[2].as_log()["baz"], 789.into());
        }
    }

    #[test]
    fn deserialize_error_trailing_bytes() {
        // A complete {"foo": 123} followed by a truncated map.
        let input = Bytes::from_static(b"\x81\xA3\x66\x6F\x6F\x7B\x81\xA3\x66");
        let deserializer = MsgPackDeserializer;

        for namespace in [LogNamespace::Legacy, LogNamespace::Vector] {
            assert!(deserializer.parse(input.clone(), namespace).is_err());
        }
    }

    #[test]
    fn deserialize_error_invalid_msgpack() {
        let input = Bytes::from_static(b"\x92\x81\xA3\x66");
        let deserializer = MsgPackDeserializer;

        for namespace in [LogNamespace::Legacy, LogNamespace::Vector] {
            assert!(deserializer.parse(input.clone(), namespace).is_err());
        }
    }
}
