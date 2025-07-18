use super::proto::common::v1::{any_value::Value as PBValue, KeyValue};
use bytes::Bytes;
use ordered_float::NotNan;
use vector_core::event::metric::TagValue;
use vrl::value::{ObjectMap, Value};

impl From<PBValue> for Value {
    fn from(av: PBValue) -> Self {
        match av {
            PBValue::StringValue(v) => Value::Bytes(Bytes::from(v)),
            PBValue::BoolValue(v) => Value::Boolean(v),
            PBValue::IntValue(v) => Value::Integer(v),
            PBValue::DoubleValue(v) => NotNan::new(v).map(Value::Float).unwrap_or(Value::Null),
            PBValue::BytesValue(v) => Value::Bytes(Bytes::from(v)),
            PBValue::ArrayValue(arr) => Value::Array(
                arr.values
                    .into_iter()
                    .map(|av| av.value.map(Into::into).unwrap_or(Value::Null))
                    .collect::<Vec<Value>>(),
            ),
            PBValue::KvlistValue(arr) => kv_list_into_value(arr.values),
        }
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

pub fn kv_list_into_value(arr: Vec<KeyValue>) -> Value {
    Value::Object(
        arr.into_iter()
            .filter_map(|kv| {
                kv.value.map(|av| {
                    (
                        kv.key.into(),
                        av.value.map(Into::into).unwrap_or(Value::Null),
                    )
                })
            })
            .collect::<ObjectMap>(),
    )
}

pub fn to_hex(d: &[u8]) -> String {
    if d.is_empty() {
        return "".to_string();
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
                assert!(f.into_inner().is_infinite() && f.into_inner().is_sign_positive())
            }
            _ => panic!("Expected Float value, got {result:?}"),
        }

        let neg_inf_value = PBValue::DoubleValue(f64::NEG_INFINITY);
        let result = Value::from(neg_inf_value);
        match result {
            Value::Float(f) => {
                assert!(f.into_inner().is_infinite() && f.into_inner().is_sign_negative())
            }
            _ => panic!("Expected Float value, got {result:?}"),
        }
    }
}
