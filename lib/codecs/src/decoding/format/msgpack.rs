use std::io::Cursor;

use super::Deserializer;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use derivative::Derivative;
use ordered_float::NotNan;
use serde::Deserialize;
use smallvec::SmallVec;
use vector_core::config::DataType;
use vector_core::event::LogEvent;
use vector_core::{
    config::{LogNamespace, log_schema},
    event::Event,
    schema,
};
use vrl::{
    prelude::Kind,
    value::{KeyString, ObjectMap, Value},
};

/// The extension type reserved by the `MessagePack` spec for timestamps.
const TIMESTAMP_EXT_TYPE: i8 = -1;

/// Nesting limit passed to `rmp_serde`.
const MAX_DEPTH: usize = 128;

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
        // An empty frame is not valid MessagePack. Report it rather than silently producing no
        // events. An empty array (`0x90`) is still valid and yields no events.
        if bytes.is_empty() {
            return Err("Empty frame is not valid MessagePack".into());
        }

        let len = bytes.len() as u64;
        // Read through `rmp_serde` rather than `rmpv::decode::read_value`, which silently maps
        // the reserved `0xC1` marker to nil.
        let mut deserializer = rmp_serde::Deserializer::new(Cursor::new(&bytes[..]));
        deserializer.set_max_depth(MAX_DEPTH);
        let mut events: SmallVec<[Event; 1]> = SmallVec::new();

        // Keep decoding until every byte of the frame has been consumed. A truncated or
        // malformed trailing value surfaces as an error rather than being silently dropped.
        while deserializer.position() < len {
            match rmpv::Value::deserialize(&mut deserializer)? {
                rmpv::Value::Array(array) => {
                    for elem in array {
                        events.push(value_to_event(convert_value(elem)?, log_namespace)?);
                    }
                }
                value => events.push(value_to_event(convert_value(value)?, log_namespace)?),
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

/// Build a log event from a decoded value. The Legacy namespace requires an object root, so any
/// other value is rejected, matching `Event::from_json_value`.
fn value_to_event(value: Value, log_namespace: LogNamespace) -> vector_common::Result<Event> {
    match (log_namespace, value) {
        (LogNamespace::Legacy, value @ Value::Object(_)) | (LogNamespace::Vector, value) => {
            Ok(Event::Log(LogEvent::from(value)))
        }
        (LogNamespace::Legacy, _) => {
            Err("Attempted to convert non-Object MessagePack into an Event.".into())
        }
    }
}

/// Convert a decoded `MessagePack` value into a `Value`.
///
/// This follows the `fluent` source's conversion: unsigned integers above `i64::MAX` are kept as
/// decimal strings, NaN floats become null, and extension types other than timestamps become an
/// object holding the extension code and raw bytes. Unlike `fluent`, non-string map keys are
/// rejected rather than silently dropped.
fn convert_value(value: rmpv::Value) -> vector_common::Result<Value> {
    Ok(match value {
        rmpv::Value::Nil => Value::Null,
        rmpv::Value::Boolean(b) => Value::Boolean(b),
        rmpv::Value::Integer(i) => i
            .as_i64()
            .map_or_else(|| Value::from(i.to_string()), Value::Integer),
        rmpv::Value::F32(f) => NotNan::new(f64::from(f)).map_or(Value::Null, Value::Float),
        rmpv::Value::F64(f) => NotNan::new(f).map_or(Value::Null, Value::Float),
        rmpv::Value::String(s) => Value::Bytes(s.into_bytes().into()),
        rmpv::Value::Binary(bytes) => Value::Bytes(bytes.into()),
        rmpv::Value::Array(values) => Value::Array(
            values
                .into_iter()
                .map(convert_value)
                .collect::<vector_common::Result<_>>()?,
        ),
        rmpv::Value::Map(entries) => Value::Object(
            entries
                .into_iter()
                .map(|(key, value)| {
                    let key = key
                        .as_str()
                        .ok_or("MessagePack map keys must be UTF-8 strings")?;
                    Ok((KeyString::from(key), convert_value(value)?))
                })
                .collect::<vector_common::Result<ObjectMap>>()?,
        ),
        rmpv::Value::Ext(TIMESTAMP_EXT_TYPE, bytes) => Value::Timestamp(decode_timestamp(&bytes)?),
        rmpv::Value::Ext(code, bytes) => Value::Object(ObjectMap::from([
            (
                KeyString::from("msgpack_extension_code"),
                Value::Integer(code.into()),
            ),
            (KeyString::from("bytes"), Value::Bytes(bytes.into())),
        ])),
    })
}

/// Decode the payload of a `MessagePack` timestamp extension. The spec defines 32-bit, 64-bit
/// and 96-bit encodings: <https://github.com/msgpack/msgpack/blob/master/spec.md#timestamp-extension-type>
fn decode_timestamp(bytes: &[u8]) -> vector_common::Result<DateTime<Utc>> {
    let (seconds, nanoseconds) = match bytes.len() {
        4 => {
            let seconds = u32::from_be_bytes(bytes.try_into()?);
            (i64::from(seconds), 0)
        }
        8 => {
            let value = u64::from_be_bytes(bytes.try_into()?);
            // The upper 30 bits hold nanoseconds and the lower 34 bits hold seconds, so both
            // casts are lossless.
            ((value & 0x3_FFFF_FFFF).cast_signed(), (value >> 34) as u32)
        }
        12 => {
            let nanoseconds = u32::from_be_bytes(bytes[..4].try_into()?);
            let seconds = i64::from_be_bytes(bytes[4..].try_into()?);
            (seconds, nanoseconds)
        }
        len => return Err(format!("Invalid MessagePack timestamp length: {len}").into()),
    };
    if nanoseconds >= 1_000_000_000 {
        return Err(format!("Invalid MessagePack timestamp nanoseconds: {nanoseconds}").into());
    }
    DateTime::from_timestamp(seconds, nanoseconds)
        .ok_or_else(|| format!("MessagePack timestamp out of range: {seconds}s").into())
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
    fn deserialize_large_unsigned_integer_as_string() {
        // {"id": 9223372036854775809, "max": 18446744073709551615, "small": 7}
        let input = Bytes::from_static(
            b"\x83\xA2id\xCF\x80\x00\x00\x00\x00\x00\x00\x01\xA3max\xCF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xA5small\xCF\x00\x00\x00\x00\x00\x00\x00\x07",
        );
        let deserializer = MsgPackDeserializer;

        for namespace in [LogNamespace::Legacy, LogNamespace::Vector] {
            let events = deserializer.parse(input.clone(), namespace).unwrap();
            let log = events[0].as_log();
            assert_eq!(log["id"], "9223372036854775809".into());
            assert_eq!(log["max"], "18446744073709551615".into());
            assert_eq!(log["small"], Value::Integer(7));
        }
    }

    #[test]
    fn deserialize_timestamp_extension() {
        // {"ts32": 1, "ts64": 1.5, "ts96": -1.000000001} using each timestamp encoding.
        let input = Bytes::from_static(
            b"\x83\xA4ts32\xD6\xFF\x00\x00\x00\x01\xA4ts64\xD7\xFF\x77\x35\x94\x00\x00\x00\x00\x01\xA4ts96\xC7\x0C\xFF\x3B\x9A\xC9\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFE",
        );
        let deserializer = MsgPackDeserializer;

        for namespace in [LogNamespace::Legacy, LogNamespace::Vector] {
            let events = deserializer.parse(input.clone(), namespace).unwrap();
            let log = events[0].as_log();
            assert_eq!(
                log["ts32"],
                Value::Timestamp(DateTime::from_timestamp(1, 0).unwrap())
            );
            assert_eq!(
                log["ts64"],
                Value::Timestamp(DateTime::from_timestamp(1, 500_000_000).unwrap())
            );
            assert_eq!(
                log["ts96"],
                Value::Timestamp(DateTime::from_timestamp(-2, 999_999_999).unwrap())
            );
        }
    }

    #[test]
    fn deserialize_other_extension_and_binary() {
        // {"ext": ext(5, [0x01, 0x02]), "bin": bin([0x41])}
        let input = Bytes::from_static(b"\x82\xA3ext\xD5\x05\x01\x02\xA3bin\xC4\x01\x41");
        let deserializer = MsgPackDeserializer;

        let events = deserializer.parse(input, LogNamespace::Vector).unwrap();
        let log = events[0].as_log();
        assert_eq!(log["ext.msgpack_extension_code"], Value::Integer(5));
        assert_eq!(
            log["ext.bytes"],
            Value::Bytes(Bytes::from_static(b"\x01\x02"))
        );
        assert_eq!(log["bin"], Value::Bytes(Bytes::from_static(b"A")));
    }

    #[test]
    fn deserialize_error_invalid_timestamp_extension() {
        let deserializer = MsgPackDeserializer;

        // Timestamp extension with a 2-byte payload, which the spec does not define.
        assert!(
            deserializer
                .parse(
                    Bytes::from_static(b"\xD5\xFF\x00\x01"),
                    LogNamespace::Vector
                )
                .is_err()
        );
        // 96-bit timestamp with nanoseconds of 1_000_000_000.
        assert!(
            deserializer
                .parse(
                    Bytes::from_static(
                        b"\xC7\x0C\xFF\x3B\x9A\xCA\x00\x00\x00\x00\x00\x00\x00\x00\x00"
                    ),
                    LogNamespace::Vector
                )
                .is_err()
        );
    }

    #[test]
    fn deserialize_error_non_object_legacy_namespace() {
        let deserializer = MsgPackDeserializer;

        // nil
        assert!(
            deserializer
                .parse(Bytes::from_static(b"\xC0"), LogNamespace::Legacy)
                .is_err()
        );
        // [{"foo": 123}, 1]
        assert!(
            deserializer
                .parse(
                    Bytes::from_static(b"\x92\x81\xA3\x66\x6F\x6F\x7B\x01"),
                    LogNamespace::Legacy
                )
                .is_err()
        );
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

    #[test]
    fn deserialize_error_reserved_marker() {
        let deserializer = MsgPackDeserializer;

        // 0xC1 at the top level, nested in a map ({"x": 0xC1}), and inside an array.
        for input in [&b"\xC1"[..], b"\x81\xA1x\xC1", b"\x92\x81\xA1x\x01\xC1"] {
            for namespace in [LogNamespace::Legacy, LogNamespace::Vector] {
                assert!(
                    deserializer
                        .parse(Bytes::copy_from_slice(input), namespace)
                        .is_err()
                );
            }
        }
    }

    #[test]
    fn deserialize_error_empty_frame() {
        let deserializer = MsgPackDeserializer;

        for namespace in [LogNamespace::Legacy, LogNamespace::Vector] {
            assert!(deserializer.parse(Bytes::new(), namespace).is_err());
        }
    }

    #[test]
    fn deserialize_empty_array() {
        let deserializer = MsgPackDeserializer;

        for namespace in [LogNamespace::Legacy, LogNamespace::Vector] {
            let events = deserializer
                .parse(Bytes::from_static(b"\x90"), namespace)
                .unwrap();
            assert!(events.is_empty());
        }
    }
}
