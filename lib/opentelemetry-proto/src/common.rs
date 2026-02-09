use bytes::Bytes;
use ordered_float::NotNan;
use tracing::warn;
use vector_core::event::metric::TagValue;
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

// ============================================================================
// Inverse converters: Value → PBValue (for encoding native logs to OTLP)
// ============================================================================

/// Convert a Vector Value to an OTLP PBValue.
/// This is the inverse of the existing `From<PBValue> for Value` implementation.
impl From<Value> for PBValue {
    fn from(v: Value) -> Self {
        match v {
            // Mirrors: PBValue::StringValue(v) => Value::Bytes(Bytes::from(v))
            // Optimization: Try valid UTF-8 first to avoid allocation
            Value::Bytes(b) => PBValue::StringValue(
                String::from_utf8(b.to_vec()).unwrap_or_else(|e| {
                    String::from_utf8_lossy(e.as_bytes()).into_owned()
                }),
            ),

            // Mirrors: PBValue::BoolValue(v) => Value::Boolean(v)
            Value::Boolean(b) => PBValue::BoolValue(b),

            // Mirrors: PBValue::IntValue(v) => Value::Integer(v)
            Value::Integer(i) => PBValue::IntValue(i),

            // Mirrors: PBValue::DoubleValue(v) => NotNan::new(v).map(Value::Float)...
            Value::Float(f) => PBValue::DoubleValue(f.into_inner()),

            // Mirrors: PBValue::ArrayValue(arr) => Value::Array(...)
            Value::Array(arr) => {
                let mut values = Vec::with_capacity(arr.len());
                for v in arr {
                    values.push(AnyValue {
                        value: Some(v.into()),
                    });
                }
                PBValue::ArrayValue(ArrayValue { values })
            }

            // Mirrors: PBValue::KvlistValue(arr) => kv_list_into_value(arr.values)
            Value::Object(obj) => PBValue::KvlistValue(KeyValueList {
                values: value_object_to_kv_list(obj),
            }),

            // Types without direct OTLP equivalent - convert to string representation
            Value::Timestamp(ts) => PBValue::StringValue(ts.to_rfc3339()),
            Value::Null => PBValue::StringValue(String::new()),
            Value::Regex(r) => PBValue::StringValue(r.to_string()),
        }
    }
}

/// Convert a Vector ObjectMap to a Vec<KeyValue> for OTLP.
/// This is the inverse of `kv_list_into_value`.
#[inline]
pub fn value_object_to_kv_list(obj: ObjectMap) -> Vec<KeyValue> {
    // Pre-allocate based on input size (some may be filtered)
    let mut result = Vec::with_capacity(obj.len());
    for (k, v) in obj {
        // Skip null values (OTLP doesn't represent them well)
        if matches!(v, Value::Null) {
            continue;
        }
        result.push(KeyValue {
            key: k.into(),
            value: Some(AnyValue {
                value: Some(v.into()),
            }),
        });
    }
    result
}

/// Convert a hex string to bytes.
/// This is the inverse of `to_hex`.
/// Handles various input formats gracefully (with/without 0x prefix, whitespace).
#[inline]
pub fn from_hex(s: &str) -> Vec<u8> {
    if s.is_empty() {
        return Vec::new();
    }
    let s = s.trim();
    let s = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);

    // hex::decode already pre-allocates correctly
    hex::decode(s).unwrap_or_else(|e| {
        warn!(message = "Invalid hex string, using empty bytes", input = %s, error = %e);
        Vec::new()
    })
}

/// Validate trace_id bytes and return valid 16-byte trace_id or empty.
/// Handles common mistakes like hex strings passed as bytes.
/// Returns owned Vec to allow caller to use directly in protobuf message.
#[inline]
pub fn validate_trace_id(bytes: &[u8]) -> Vec<u8> {
    match bytes.len() {
        0 => Vec::new(),
        16 => bytes.to_vec(),
        32 => {
            // Auto-fix: hex string passed as bytes (common mistake)
            // Try direct hex decode from bytes to avoid UTF-8 conversion
            if bytes.iter().all(|b| b.is_ascii_hexdigit()) {
                // Safe: all bytes are ASCII hex digits
                let s = unsafe { std::str::from_utf8_unchecked(bytes) };
                from_hex(s)
            } else {
                warn!(message = "trace_id appears to be hex string but contains invalid chars");
                Vec::new()
            }
        }
        _ => {
            warn!(
                message = "Invalid trace_id length, clearing",
                length = bytes.len()
            );
            Vec::new()
        }
    }
}

/// Validate span_id bytes and return valid 8-byte span_id or empty.
/// Handles common mistakes like hex strings passed as bytes.
#[inline]
pub fn validate_span_id(bytes: &[u8]) -> Vec<u8> {
    match bytes.len() {
        0 => Vec::new(),
        8 => bytes.to_vec(),
        16 => {
            // Auto-fix: hex string passed as bytes (common mistake)
            // Try direct hex decode from bytes to avoid UTF-8 conversion
            if bytes.iter().all(|b| b.is_ascii_hexdigit()) {
                // Safe: all bytes are ASCII hex digits
                let s = unsafe { std::str::from_utf8_unchecked(bytes) };
                from_hex(s)
            } else {
                warn!(message = "span_id appears to be hex string but contains invalid chars");
                Vec::new()
            }
        }
        _ => {
            warn!(
                message = "Invalid span_id length, clearing",
                length = bytes.len()
            );
            Vec::new()
        }
    }
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

    // ========================================================================
    // Tests for Value → PBValue conversion (inverse direction)
    // ========================================================================

    #[test]
    fn test_value_to_pb_string() {
        let v = Value::Bytes(Bytes::from("hello"));
        let pb: PBValue = v.into();
        assert!(matches!(pb, PBValue::StringValue(s) if s == "hello"));
    }

    #[test]
    fn test_value_to_pb_boolean() {
        let v = Value::Boolean(true);
        let pb: PBValue = v.into();
        assert!(matches!(pb, PBValue::BoolValue(true)));
    }

    #[test]
    fn test_value_to_pb_integer() {
        let v = Value::Integer(42);
        let pb: PBValue = v.into();
        assert!(matches!(pb, PBValue::IntValue(42)));
    }

    #[test]
    fn test_value_to_pb_float() {
        let v = Value::Float(NotNan::new(3.14).unwrap());
        let pb: PBValue = v.into();
        match pb {
            PBValue::DoubleValue(f) => assert!((f - 3.14).abs() < 0.001),
            _ => panic!("Expected DoubleValue"),
        }
    }

    #[test]
    fn test_value_to_pb_array() {
        let v = Value::Array(vec![Value::Integer(1), Value::Integer(2)]);
        let pb: PBValue = v.into();
        match pb {
            PBValue::ArrayValue(arr) => assert_eq!(arr.values.len(), 2),
            _ => panic!("Expected ArrayValue"),
        }
    }

    #[test]
    fn test_value_to_pb_object() {
        let mut obj = ObjectMap::new();
        obj.insert("key".into(), Value::Bytes(Bytes::from("value")));
        let v = Value::Object(obj);
        let pb: PBValue = v.into();
        match pb {
            PBValue::KvlistValue(kv) => {
                assert_eq!(kv.values.len(), 1);
                assert_eq!(kv.values[0].key, "key");
            }
            _ => panic!("Expected KvlistValue"),
        }
    }

    #[test]
    fn test_value_to_pb_null_filtered() {
        let mut obj = ObjectMap::new();
        obj.insert("key".into(), Value::Null);
        obj.insert("valid".into(), Value::Integer(1));
        let kv = value_object_to_kv_list(obj);
        // Null should be filtered out
        assert_eq!(kv.len(), 1);
        assert_eq!(kv[0].key, "valid");
    }

    #[test]
    fn test_value_to_pb_invalid_utf8() {
        // Invalid UTF-8 bytes should be handled gracefully
        let invalid = Bytes::from(vec![0xff, 0xfe]);
        let v = Value::Bytes(invalid);
        let pb: PBValue = v.into();
        // Should use lossy conversion, not panic
        assert!(matches!(pb, PBValue::StringValue(_)));
    }

    // ========================================================================
    // Tests for from_hex (inverse of to_hex)
    // ========================================================================

    #[test]
    fn test_from_hex_valid() {
        assert_eq!(from_hex("0123"), vec![0x01, 0x23]);
        assert_eq!(from_hex("abcdef"), vec![0xab, 0xcd, 0xef]);
    }

    #[test]
    fn test_from_hex_empty() {
        let empty: Vec<u8> = vec![];
        assert_eq!(from_hex(""), empty);
    }

    #[test]
    fn test_from_hex_invalid_chars() {
        // Invalid hex should return empty, not panic
        let empty: Vec<u8> = vec![];
        assert_eq!(from_hex("ghij"), empty);
        assert_eq!(from_hex("not-hex"), empty);
        assert_eq!(from_hex("zzzz"), empty);
    }

    #[test]
    fn test_from_hex_odd_length() {
        // Odd length hex is invalid
        let empty: Vec<u8> = vec![];
        assert_eq!(from_hex("123"), empty);
    }

    #[test]
    fn test_from_hex_with_prefix() {
        assert_eq!(from_hex("0x0123"), vec![0x01, 0x23]);
        assert_eq!(from_hex("0X0123"), vec![0x01, 0x23]);
    }

    #[test]
    fn test_from_hex_with_whitespace() {
        assert_eq!(from_hex("  0123  "), vec![0x01, 0x23]);
    }

    // ========================================================================
    // Tests for validate_trace_id and validate_span_id
    // ========================================================================

    #[test]
    fn test_validate_trace_id_valid() {
        let valid_16_bytes = vec![0u8; 16];
        assert_eq!(validate_trace_id(&valid_16_bytes), valid_16_bytes);
    }

    #[test]
    fn test_validate_trace_id_empty() {
        let empty: Vec<u8> = vec![];
        assert_eq!(validate_trace_id(&[]), empty);
    }

    #[test]
    fn test_validate_trace_id_wrong_length() {
        // Too short - should return empty
        let result = validate_trace_id(&[0x01, 0x02]);
        let empty: Vec<u8> = vec![];
        assert_eq!(result, empty);
    }

    #[test]
    fn test_validate_trace_id_hex_string_as_bytes() {
        // User passed hex string as bytes (32 ASCII chars for 16-byte trace_id)
        let hex_as_bytes = b"0123456789abcdef0123456789abcdef"; // 32 bytes of ASCII
        let result = validate_trace_id(hex_as_bytes);
        assert_eq!(result.len(), 16); // Should decode to 16 bytes
    }

    #[test]
    fn test_validate_span_id_valid() {
        let valid_8_bytes = vec![0u8; 8];
        assert_eq!(validate_span_id(&valid_8_bytes), valid_8_bytes);
    }

    #[test]
    fn test_validate_span_id_empty() {
        let empty: Vec<u8> = vec![];
        assert_eq!(validate_span_id(&[]), empty);
    }

    #[test]
    fn test_validate_span_id_wrong_length() {
        // Too short - should return empty
        let result = validate_span_id(&[0x01, 0x02]);
        let empty: Vec<u8> = vec![];
        assert_eq!(result, empty);
    }

    #[test]
    fn test_validate_span_id_hex_string_as_bytes() {
        // User passed hex string as bytes (16 ASCII chars for 8-byte span_id)
        let hex_as_bytes = b"0123456789abcdef"; // 16 bytes of ASCII
        let result = validate_span_id(hex_as_bytes);
        assert_eq!(result.len(), 8); // Should decode to 8 bytes
    }

    // ========================================================================
    // Roundtrip tests: Value → PBValue → Value
    // ========================================================================

    #[test]
    fn test_roundtrip_string() {
        let original = Value::Bytes(Bytes::from("test"));
        let pb: PBValue = original.clone().into();
        let roundtrip: Value = pb.into();
        assert_eq!(original, roundtrip);
    }

    #[test]
    fn test_roundtrip_integer() {
        let original = Value::Integer(12345);
        let pb: PBValue = original.clone().into();
        let roundtrip: Value = pb.into();
        assert_eq!(original, roundtrip);
    }

    #[test]
    fn test_roundtrip_boolean() {
        let original = Value::Boolean(true);
        let pb: PBValue = original.clone().into();
        let roundtrip: Value = pb.into();
        assert_eq!(original, roundtrip);
    }

    #[test]
    fn test_roundtrip_float() {
        let original = Value::Float(NotNan::new(3.14159).unwrap());
        let pb: PBValue = original.clone().into();
        let roundtrip: Value = pb.into();
        assert_eq!(original, roundtrip);
    }
}
