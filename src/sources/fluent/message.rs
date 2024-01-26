use std::{collections::BTreeMap, convert::TryInto};

use chrono::{serde::ts_seconds, DateTime, TimeZone, Utc};
use ordered_float::NotNan;
use serde::{Deserialize, Serialize};
use vector_lib::event::{KeyString, ObjectMap, Value};

/// Fluent msgpack messages can be encoded in one of three ways, each with and
/// without options, all using arrays to encode the top-level fields.
///
/// The spec refers to 4 ways, but really CompressedPackedForward is encoded the
/// same as PackedForward, it just has an additional decompression step.
///
/// Not yet handled are the handshake messages.
///
/// <https://github.com/fluent/fluentd/wiki/Forward-Protocol-Specification-v1#event-modes>
#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub(super) enum FluentMessage {
    Message(FluentTag, FluentTimestamp, FluentRecord),
    // I attempted to just one variant for each of these, with and without options, using an
    // `Option` for the last element, but rmp expected the number of elements to match in that case
    // still (it just allows the last element to be `nil`).
    MessageWithOptions(
        FluentTag,
        FluentTimestamp,
        FluentRecord,
        FluentMessageOptions,
    ),
    Forward(FluentTag, Vec<FluentEntry>),
    ForwardWithOptions(FluentTag, Vec<FluentEntry>, FluentMessageOptions),
    PackedForward(FluentTag, serde_bytes::ByteBuf),
    PackedForwardWithOptions(FluentTag, serde_bytes::ByteBuf, FluentMessageOptions),

    // should be last as it'll match any other message
    Heartbeat(rmpv::Value), // should be Nil if heartbeat
}

/// Server options sent by client.
///
/// <https://github.com/fluent/fluentd/wiki/Forward-Protocol-Specification-v1#option>
#[derive(Default, Debug, Deserialize, Serialize)]
#[serde(default)]
pub(super) struct FluentMessageOptions {
    pub(super) size: Option<u64>, // client provided hint for the number of entries
    pub(super) chunk: Option<String>, // client provided chunk identifier for acks
    pub(super) compressed: Option<String>, // this one is required if present
}

/// Fluent entry consisting of timestamp and record.
///
/// <https://github.com/fluent/fluentd/wiki/Forward-Protocol-Specification-v1#forward-mode>
#[derive(Debug, Deserialize, Serialize)]
pub(super) struct FluentEntry(pub(super) FluentTimestamp, pub(super) FluentRecord);

/// Fluent record is just key/value pairs.
pub(super) type FluentRecord = BTreeMap<String, FluentValue>;

/// Fluent message tag.
pub(super) type FluentTag = String;

/// Custom decoder for Fluent's EventTime msgpack extension.
///
/// <https://github.com/fluent/fluentd/wiki/Forward-Protocol-Specification-v1#eventtime-ext-format>
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(super) struct FluentEventTime(DateTime<Utc>);

impl<'de> serde::de::Deserialize<'de> for FluentEventTime {
    fn deserialize<D>(deserializer: D) -> Result<FluentEventTime, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct FluentEventTimeVisitor;

        impl<'de> serde::de::Visitor<'de> for FluentEventTimeVisitor {
            type Value = FluentEventTime;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("fluent timestamp extension")
            }

            fn visit_newtype_struct<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: serde::de::Deserializer<'de>,
            {
                deserializer.deserialize_tuple(2, self)
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let tag: u32 = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(0, &self))?;

                if tag != 0 {
                    return Err(serde::de::Error::custom(format!(
                        "expected extension type 0 for fluent timestamp, got {}",
                        tag
                    )));
                }

                let bytes: serde_bytes::ByteBuf = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(1, &self))?;

                if bytes.len() != 8 {
                    return Err(serde::de::Error::custom(format!(
                        "expected exactly 8 bytes for binary encoded fluent timestamp, got {}",
                        bytes.len()
                    )));
                }

                // length checked right above
                let seconds = u32::from_be_bytes(bytes[..4].try_into().expect("exactly 4 bytes"));
                let nanoseconds =
                    u32::from_be_bytes(bytes[4..].try_into().expect("exactly 4 bytes"));

                Ok(FluentEventTime(
                    Utc.timestamp_opt(seconds.into(), nanoseconds)
                        .single()
                        .expect("invalid timestamp"),
                ))
            }
        }

        deserializer.deserialize_any(FluentEventTimeVisitor)
    }
}

/// Value for fluent record key.
///
/// Used mostly just to implement value conversion.
#[derive(Debug, Deserialize, PartialEq, Serialize)]
pub(super) struct FluentValue(rmpv::Value);

impl From<rmpv::Value> for FluentValue {
    fn from(value: rmpv::Value) -> Self {
        Self(value)
    }
}

impl From<FluentValue> for Value {
    fn from(value: FluentValue) -> Self {
        match value.0 {
            rmpv::Value::Nil => Value::Null,
            rmpv::Value::Boolean(b) => Value::Boolean(b),
            rmpv::Value::Integer(i) => i
                .as_i64()
                .map(Value::Integer)
                // unwrap large numbers to string similar to how
                // `From<serde_json::Value> for Value` handles it
                .unwrap_or_else(|| Value::Bytes(i.to_string().into())),
            rmpv::Value::F32(f) => {
                // serde_json converts NaN to Null, so we model that behavior here since this is non-fallible
                NotNan::new(f as f64)
                    .map(Value::Float)
                    .unwrap_or(Value::Null)
            }
            rmpv::Value::F64(f) => {
                // serde_json converts NaN to Null, so we model that behavior here since this is non-fallible
                NotNan::new(f).map(Value::Float).unwrap_or(Value::Null)
            }
            rmpv::Value::String(s) => Value::Bytes(s.into_bytes().into()),
            rmpv::Value::Binary(bytes) => Value::Bytes(bytes.into()),
            rmpv::Value::Array(values) => Value::Array(
                values
                    .into_iter()
                    .map(|value| Value::from(FluentValue(value)))
                    .collect(),
            ),
            rmpv::Value::Map(values) => {
                // Per
                // <https://github.com/fluent/fluentd/wiki/Forward-Protocol-Specification-v1#message-modes>
                // we should expect that keys are always stringy. Ultimately a
                // lot hinges on what
                // <https://github.com/fluent/fluentd/wiki/Forward-Protocol-Specification-v1#grammar>
                // defines 'object' as.
                //
                // The current implementation will SILENTLY DROP non-stringy keys.
                Value::Object(
                    values
                        .into_iter()
                        .filter_map(|(key, value)| {
                            key.as_str()
                                .map(|k| (k.into(), Value::from(FluentValue(value))))
                        })
                        .collect(),
                )
            }
            rmpv::Value::Ext(code, bytes) => {
                let mut fields = ObjectMap::new();
                fields.insert(
                    KeyString::from("msgpack_extension_code"),
                    Value::Integer(code.into()),
                );
                fields.insert(KeyString::from("bytes"), Value::Bytes(bytes.into()));
                Value::Object(fields)
            }
        }
    }
}

/// Fluent message timestamp.
///
/// Message timestamps can be a unix timestamp or EventTime messagepack ext.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(untagged)]
pub(super) enum FluentTimestamp {
    #[serde(with = "ts_seconds")]
    Unix(DateTime<Utc>),
    Ext(FluentEventTime),
}

impl From<FluentTimestamp> for Value {
    fn from(timestamp: FluentTimestamp) -> Self {
        match timestamp {
            FluentTimestamp::Unix(timestamp) | FluentTimestamp::Ext(FluentEventTime(timestamp)) => {
                Value::Timestamp(timestamp)
            }
        }
    }
}

#[cfg(test)]
mod test {
    use std::collections::BTreeMap;

    use approx::assert_relative_eq;
    use quickcheck::quickcheck;
    use vrl::value::{ObjectMap, Value};

    use crate::sources::fluent::message::FluentValue;

    quickcheck! {
      fn from_bool(input: bool) -> () {
          assert_eq!(Value::from(FluentValue(rmpv::Value::Boolean(input))),
              Value::Boolean(input))
        }
    }

    quickcheck! {
      fn from_i64(input: i64) -> () {
          assert_eq!(Value::from(FluentValue(rmpv::Value::Integer(rmpv::Integer::from(input)))),
              Value::Integer(input))
        }
    }

    quickcheck! {
        fn from_u64(input: u64) -> () {
            if input > i64::max_value() as u64 {
                assert_eq!(Value::from(FluentValue(rmpv::Value::Integer(rmpv::Integer::from(input)))),
                           Value::Bytes(input.to_string().into()))
            } else {
                assert_eq!(Value::from(FluentValue(rmpv::Value::Integer(rmpv::Integer::from(input)))),
                           Value::Integer(input as i64))
            }
        }
    }

    quickcheck! {
      fn from_f32(input: f32) -> () {
          let val = Value::from(FluentValue(rmpv::Value::F32(input)));
          if input.is_nan() {
              assert_eq!(val, Value::Null);
          } else {
              assert_relative_eq!(input as f64, val.as_float().unwrap().into_inner());
          }
        }
    }

    quickcheck! {
      fn from_f64(input: f64) -> () {
          let val = Value::from(FluentValue(rmpv::Value::F64(input)));
          if input.is_nan() {
              assert_eq!(val, Value::Null);
          } else {
              assert_relative_eq!(input, val.as_float().unwrap().into_inner());
          }
        }
    }

    quickcheck! {
      fn from_string(input: String) -> () {
          assert_eq!(Value::from(FluentValue(rmpv::Value::String(rmpv::Utf8String::from(input.clone())))),
                     Value::Bytes(input.into_bytes().into()))
      }
    }

    quickcheck! {
      fn from_binary(input: Vec<u8>) -> () {
          assert_eq!(Value::from(FluentValue(rmpv::Value::Binary(input.clone()))),
                     Value::Bytes(input.into()))
      }
    }

    quickcheck! {
      fn from_i64_array(input: Vec<i64>) -> () {
          let actual: rmpv::Value = rmpv::Value::Array(input.iter().map(|i| rmpv::Value::from(*i)).collect());
          let expected: Value = Value::Array(input.iter().map(|i| Value::Integer(*i)).collect());
          assert_eq!(Value::from(FluentValue(actual)), expected);
      }
    }

    quickcheck! {
        fn from_map(input: Vec<(String, i64)>) -> () {
            let key_fn = |k| { rmpv::Value::String(rmpv::Utf8String::from(k)) };
            let val_fn = |k| { rmpv::Value::Integer(rmpv::Integer::from(k)) };
            let actual_inner: Vec<(rmpv::Value, rmpv::Value)> = input.clone().into_iter().map(|(k,v)| (key_fn(k), val_fn(v))).collect();
            let actual = rmpv::Value::Map(actual_inner);

            let mut expected_inner = ObjectMap::new();
            for (k,v) in input.into_iter() {
                expected_inner.insert(k.into(), Value::Integer(v));
            }
            let expected = Value::Object(expected_inner);

            assert_eq!(Value::from(FluentValue(actual)), expected);
      }
    }

    quickcheck! {
        fn from_nonstring_key_map(input: Vec<(i64, i64)>) -> () {
            // Any map that has non-string keys will be coerced into an empty
            // map. Such maps are a violation of the fluent protocol and we
            // prefer to silently drop keys rather than crash the process.

            let key_fn = |k| { rmpv::Value::Integer(rmpv::Integer::from(k)) };
            let val_fn = |k| { rmpv::Value::Integer(rmpv::Integer::from(k)) };
            let actual_inner: Vec<(rmpv::Value, rmpv::Value)> = input.into_iter().map(|(k,v)| (key_fn(k), val_fn(v))).collect();
            let actual = rmpv::Value::Map(actual_inner);

            let expected = Value::Object(BTreeMap::new());

            assert_eq!(Value::from(FluentValue(actual)), expected);
      }
    }

    #[test]
    fn from_nil() {
        assert_eq!(Value::from(FluentValue(rmpv::Value::Nil)), Value::Null);
    }

    quickcheck! {
        fn from_ext(code: i8, bytes: Vec<u8>) -> () {
            let actual = rmpv::Value::Ext(code, bytes.clone());

            let mut inner = ObjectMap::new();
            inner.insert("msgpack_extension_code".into(), Value::Integer(code.into()));
            inner.insert("bytes".into(), Value::Bytes(bytes.into()));
            let expected = Value::Object(inner);

            assert_eq!(Value::from(FluentValue(actual)), expected);
      }
    }
}
