use bytes::Bytes;
use chrono::SecondsFormat;
use ordered_float::NotNan;
use vector_core::event::metric::{TagValue, TagValueSet};
use vrl::value::{ObjectMap, Value};

use super::proto::common::v1::{
    AnyValue, ArrayValue, KeyValue, KeyValueList, any_value::Value as PBValue,
};

impl From<PBValue> for Value {
    fn from(av: PBValue) -> Self {
        match av {
            PBValue::StringValue(v) => Value::Bytes(Bytes::from(v)),
            PBValue::BoolValue(v) => Value::Boolean(v),
            PBValue::IntValue(v) => Value::Integer(v),
            PBValue::DoubleValue(v) => NotNan::new(v).map_or(Value::Null, Value::Float),
            PBValue::BytesValue(v) => Value::Bytes(Bytes::from(v)),
            PBValue::ArrayValue(arr) => Value::Array(
                arr.values
                    .into_iter()
                    .map(|av| av.value.map_or(Value::Null, Into::into))
                    .collect::<Vec<Value>>(),
            ),
            PBValue::KvlistValue(arr) => kv_list_into_value(arr.values),
        }
    }
}

/// Inverse of `From<PBValue> for Value`. Strings and byte strings that are valid UTF-8 become
/// `string_value`; other byte strings become `bytes_value`. Timestamps and regexes have no
/// OTLP equivalent and are encoded as strings. `Null` becomes an empty `AnyValue`.
impl From<Value> for AnyValue {
    fn from(value: Value) -> Self {
        let value = match value {
            Value::Bytes(bytes) => string_or_bytes(bytes),
            Value::String(string) => string_or_bytes(string.into_bytes()),
            Value::Regex(regex) => PBValue::StringValue(regex.as_str().to_owned()),
            Value::Integer(int) => PBValue::IntValue(int),
            Value::Float(float) => PBValue::DoubleValue(float.into_inner()),
            Value::Boolean(boolean) => PBValue::BoolValue(boolean),
            Value::Timestamp(timestamp) => {
                PBValue::StringValue(timestamp.to_rfc3339_opts(SecondsFormat::AutoSi, true))
            }
            Value::Object(object) => PBValue::KvlistValue(KeyValueList {
                values: object_into_kv_list(object),
            }),
            Value::Array(array) => PBValue::ArrayValue(ArrayValue {
                values: array.into_iter().map(Into::into).collect(),
            }),
            Value::Null => return Self { value: None },
        };
        Self { value: Some(value) }
    }
}

fn string_or_bytes(bytes: Bytes) -> PBValue {
    match String::from_utf8(Vec::from(bytes)) {
        Ok(string) => PBValue::StringValue(string),
        Err(error) => PBValue::BytesValue(error.into_bytes()),
    }
}

impl From<PBValue> for TagValue {
    fn from(pb: PBValue) -> Self {
        match pb {
            PBValue::StringValue(s) => TagValue::from(s),
            PBValue::BoolValue(b) => TagValue::from(b.to_string()),
            PBValue::IntValue(i) => TagValue::from(i.to_string()),
            PBValue::DoubleValue(f) => TagValue::from(f.to_string()),
            PBValue::BytesValue(b) => TagValue::from(String::from_utf8_lossy(&b).to_string()),
            _ => TagValue::from("null"),
        }
    }
}

impl From<TagValue> for AnyValue {
    fn from(tag: TagValue) -> Self {
        match tag {
            TagValue::Value(s) => Self {
                value: Some(PBValue::StringValue(s)),
            },
            TagValue::Bare => Self { value: None },
        }
    }
}

#[must_use]
pub fn str_to_key_value(key: &str, val: TagValue) -> KeyValue {
    KeyValue {
        key: key.to_string(),
        value: Some(val.into()),
    }
}

pub fn tag_set_to_any_value(tag_set: TagValueSet) -> Option<AnyValue> {
    match tag_set {
        TagValueSet::Empty => None,
        TagValueSet::Single(tag) => Some(tag.into()),
        TagValueSet::Set(set) => Some(AnyValue {
            value: Some(PBValue::ArrayValue(ArrayValue {
                values: set.into_iter().map(Into::into).collect(),
            })),
        }),
    }
}

#[must_use]
pub fn kv_list_into_value(arr: Vec<KeyValue>) -> Value {
    Value::Object(
        arr.into_iter()
            .filter_map(|kv| {
                kv.value
                    .map(|av| (kv.key.into(), av.value.map_or(Value::Null, Into::into)))
            })
            .collect::<ObjectMap>(),
    )
}

/// Inverse of [`kv_list_into_value`].
#[must_use]
pub fn object_into_kv_list(object: ObjectMap) -> Vec<KeyValue> {
    object
        .into_iter()
        .map(|(key, value)| KeyValue {
            key: key.into(),
            value: Some(value.into()),
        })
        .collect()
}

#[must_use]
pub fn to_hex(d: &[u8]) -> String {
    if d.is_empty() {
        return String::new();
    }
    hex::encode(d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pb_double_value_nan_handling() {
        // Test that NaN values are converted to Value::Null instead of panicking
        let nan_value = PBValue::DoubleValue(f64::NAN);
        let result = Value::from(nan_value);
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn test_pb_double_value_infinity() {
        // Test that infinity values work correctly
        let inf_value = PBValue::DoubleValue(f64::INFINITY);
        let result = Value::from(inf_value);
        match result {
            Value::Float(f) => {
                assert!(f.into_inner().is_infinite() && f.into_inner().is_sign_positive());
            }
            _ => panic!("Expected Float value, got {result:?}"),
        }

        let neg_inf_value = PBValue::DoubleValue(f64::NEG_INFINITY);
        let result = Value::from(neg_inf_value);
        match result {
            Value::Float(f) => {
                assert!(f.into_inner().is_infinite() && f.into_inner().is_sign_negative());
            }
            _ => panic!("Expected Float value, got {result:?}"),
        }
    }
}
