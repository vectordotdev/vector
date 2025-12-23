use super::*;
use arrow::{
    array::{
        Array, BinaryArray, BooleanArray, Decimal128Array, Decimal256Array, Float64Array,
        Int64Array, StringArray, TimestampMicrosecondArray, TimestampMillisecondArray,
        TimestampNanosecondArray, TimestampSecondArray, UInt8Array, UInt16Array, UInt32Array,
        UInt64Array,
    },
    datatypes::{Field, TimeUnit},
    ipc::reader::StreamReader,
};
use bytes::BytesMut;
use chrono::Utc;
use std::io::Cursor;
use tokio_util::codec::Encoder;
use vector_core::event::{Event, LogEvent};

#[test]
fn test_encode_all_types() {
    let mut log = LogEvent::default();
    log.insert("string_field", "test");
    log.insert("int8_field", 127);
    log.insert("int16_field", 32000);
    log.insert("int32_field", 1000000);
    log.insert("int64_field", 42);
    log.insert("float32_field", 3.15);
    log.insert("float64_field", 3.15);
    log.insert("bool_field", true);
    log.insert("bytes_field", bytes::Bytes::from("binary"));
    log.insert("timestamp_field", Utc::now());

    let events = vec![Event::Log(log)];

    let schema = Arc::new(Schema::new(vec![
        Field::new("string_field", DataType::Utf8, true),
        Field::new("int8_field", DataType::Int8, true),
        Field::new("int16_field", DataType::Int16, true),
        Field::new("int32_field", DataType::Int32, true),
        Field::new("int64_field", DataType::Int64, true),
        Field::new("float32_field", DataType::Float32, true),
        Field::new("float64_field", DataType::Float64, true),
        Field::new("bool_field", DataType::Boolean, true),
        Field::new("bytes_field", DataType::Binary, true),
        Field::new(
            "timestamp_field",
            DataType::Timestamp(TimeUnit::Millisecond, None),
            true,
        ),
    ]));

    let result = encode_events_to_arrow_ipc_stream(&events, Some(Arc::clone(&schema)));
    assert!(result.is_ok());

    let bytes = result.unwrap();
    let cursor = Cursor::new(bytes);
    let mut reader = StreamReader::try_new(cursor, None).unwrap();
    let batch = reader.next().unwrap().unwrap();

    assert_eq!(batch.num_rows(), 1);
    assert_eq!(batch.num_columns(), 10);

    // Verify string field
    assert_eq!(
        batch
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap()
            .value(0),
        "test"
    );

    // Verify int8 field
    assert_eq!(
        batch
            .column(1)
            .as_any()
            .downcast_ref::<arrow::array::Int8Array>()
            .unwrap()
            .value(0),
        127
    );

    // Verify int16 field
    assert_eq!(
        batch
            .column(2)
            .as_any()
            .downcast_ref::<arrow::array::Int16Array>()
            .unwrap()
            .value(0),
        32000
    );

    // Verify int32 field
    assert_eq!(
        batch
            .column(3)
            .as_any()
            .downcast_ref::<arrow::array::Int32Array>()
            .unwrap()
            .value(0),
        1000000
    );

    // Verify int64 field
    assert_eq!(
        batch
            .column(4)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap()
            .value(0),
        42
    );

    // Verify float32 field
    assert!(
        (batch
            .column(5)
            .as_any()
            .downcast_ref::<arrow::array::Float32Array>()
            .unwrap()
            .value(0)
            - 3.15)
            .abs()
            < 0.001
    );

    // Verify float64 field
    assert!(
        (batch
            .column(6)
            .as_any()
            .downcast_ref::<Float64Array>()
            .unwrap()
            .value(0)
            - 3.15)
            .abs()
            < 0.001
    );

    // Verify boolean field
    assert!(
        batch
            .column(7)
            .as_any()
            .downcast_ref::<BooleanArray>()
            .unwrap()
            .value(0),
        "{}",
        true
    );

    // Verify binary field
    assert_eq!(
        batch
            .column(8)
            .as_any()
            .downcast_ref::<BinaryArray>()
            .unwrap()
            .value(0),
        b"binary"
    );

    // Verify timestamp field
    assert!(
        !batch
            .column(9)
            .as_any()
            .downcast_ref::<TimestampMillisecondArray>()
            .unwrap()
            .is_null(0)
    );
}

#[test]
fn test_encode_null_values() {
    let mut log1 = LogEvent::default();
    log1.insert("field_a", 1);
    // field_b is missing

    let mut log2 = LogEvent::default();
    log2.insert("field_b", 2);
    // field_a is missing

    let events = vec![Event::Log(log1), Event::Log(log2)];

    let schema = Arc::new(Schema::new(vec![
        Field::new("field_a", DataType::Int64, true),
        Field::new("field_b", DataType::Int64, true),
    ]));

    let result = encode_events_to_arrow_ipc_stream(&events, Some(Arc::clone(&schema)));
    assert!(result.is_ok());

    let bytes = result.unwrap();
    let cursor = Cursor::new(bytes);
    let mut reader = StreamReader::try_new(cursor, None).unwrap();
    let batch = reader.next().unwrap().unwrap();

    assert_eq!(batch.num_rows(), 2);

    let field_a = batch
        .column(0)
        .as_any()
        .downcast_ref::<Int64Array>()
        .unwrap();
    assert_eq!(field_a.value(0), 1);
    assert!(field_a.is_null(1));

    let field_b = batch
        .column(1)
        .as_any()
        .downcast_ref::<Int64Array>()
        .unwrap();
    assert!(field_b.is_null(0));
    assert_eq!(field_b.value(1), 2);
}

#[test]
fn test_encode_type_mismatches() {
    let mut log1 = LogEvent::default();
    log1.insert("field", 42); // Integer

    let mut log2 = LogEvent::default();
    log2.insert("field", 3.15); // Float - type mismatch!

    let events = vec![Event::Log(log1), Event::Log(log2)];

    // Schema expects Int64
    let schema = Arc::new(Schema::new(vec![Field::new(
        "field",
        DataType::Int64,
        true,
    )]));

    let result = encode_events_to_arrow_ipc_stream(&events, Some(Arc::clone(&schema)));
    assert!(result.is_ok());

    let bytes = result.unwrap();
    let cursor = Cursor::new(bytes);
    let mut reader = StreamReader::try_new(cursor, None).unwrap();
    let batch = reader.next().unwrap().unwrap();

    assert_eq!(batch.num_rows(), 2);

    let field_array = batch
        .column(0)
        .as_any()
        .downcast_ref::<Int64Array>()
        .unwrap();
    assert_eq!(field_array.value(0), 42);
    assert!(field_array.is_null(1)); // Type mismatch becomes null
}

#[test]
fn test_encode_complex_json_values() {
    use serde_json::json;

    let mut log = LogEvent::default();
    log.insert(
        "object_field",
        json!({"key": "value", "nested": {"count": 42}}),
    );
    log.insert("array_field", json!([1, 2, 3]));

    let events = vec![Event::Log(log)];

    let schema = Arc::new(Schema::new(vec![
        Field::new("object_field", DataType::Utf8, true),
        Field::new("array_field", DataType::Utf8, true),
    ]));

    let result = encode_events_to_arrow_ipc_stream(&events, Some(Arc::clone(&schema)));
    assert!(result.is_ok());

    let bytes = result.unwrap();
    let cursor = Cursor::new(bytes);
    let mut reader = StreamReader::try_new(cursor, None).unwrap();
    let batch = reader.next().unwrap().unwrap();

    assert_eq!(batch.num_rows(), 1);

    let object_array = batch
        .column(0)
        .as_any()
        .downcast_ref::<StringArray>()
        .unwrap();
    let object_str = object_array.value(0);
    assert!(object_str.contains("key"));
    assert!(object_str.contains("value"));

    let array_array = batch
        .column(1)
        .as_any()
        .downcast_ref::<StringArray>()
        .unwrap();
    let array_str = array_array.value(0);
    assert_eq!(array_str, "[1,2,3]");
}

#[test]
fn test_encode_unsupported_type() {
    let mut log = LogEvent::default();
    log.insert("field", "value");

    let events = vec![Event::Log(log)];

    // Use an unsupported type
    let schema = Arc::new(Schema::new(vec![Field::new(
        "field",
        DataType::Duration(TimeUnit::Millisecond),
        true,
    )]));

    let result = encode_events_to_arrow_ipc_stream(&events, Some(schema));
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        ArrowEncodingError::UnsupportedType { .. }
    ));
}

#[test]
fn test_encode_without_schema_fails() {
    let mut log1 = LogEvent::default();
    log1.insert("message", "hello");

    let events = vec![Event::Log(log1)];

    let result = encode_events_to_arrow_ipc_stream(&events, None);
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        ArrowEncodingError::NoSchemaProvided
    ));
}

#[test]
fn test_encode_empty_events() {
    let events: Vec<Event> = vec![];
    let result = encode_events_to_arrow_ipc_stream(&events, None);
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), ArrowEncodingError::NoEvents));
}

#[test]
fn test_encode_timestamp_precisions() {
    let now = Utc::now();
    let mut log = LogEvent::default();
    log.insert("ts_second", now);
    log.insert("ts_milli", now);
    log.insert("ts_micro", now);
    log.insert("ts_nano", now);

    let events = vec![Event::Log(log)];

    let schema = Arc::new(Schema::new(vec![
        Field::new(
            "ts_second",
            DataType::Timestamp(TimeUnit::Second, None),
            true,
        ),
        Field::new(
            "ts_milli",
            DataType::Timestamp(TimeUnit::Millisecond, None),
            true,
        ),
        Field::new(
            "ts_micro",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            true,
        ),
        Field::new(
            "ts_nano",
            DataType::Timestamp(TimeUnit::Nanosecond, None),
            true,
        ),
    ]));

    let result = encode_events_to_arrow_ipc_stream(&events, Some(Arc::clone(&schema)));
    assert!(result.is_ok());

    let bytes = result.unwrap();
    let cursor = Cursor::new(bytes);
    let mut reader = StreamReader::try_new(cursor, None).unwrap();
    let batch = reader.next().unwrap().unwrap();

    assert_eq!(batch.num_rows(), 1);
    assert_eq!(batch.num_columns(), 4);

    let ts_second = batch
        .column(0)
        .as_any()
        .downcast_ref::<TimestampSecondArray>()
        .unwrap();
    assert!(!ts_second.is_null(0));
    assert_eq!(ts_second.value(0), now.timestamp());

    let ts_milli = batch
        .column(1)
        .as_any()
        .downcast_ref::<TimestampMillisecondArray>()
        .unwrap();
    assert!(!ts_milli.is_null(0));
    assert_eq!(ts_milli.value(0), now.timestamp_millis());

    let ts_micro = batch
        .column(2)
        .as_any()
        .downcast_ref::<TimestampMicrosecondArray>()
        .unwrap();
    assert!(!ts_micro.is_null(0));
    assert_eq!(ts_micro.value(0), now.timestamp_micros());

    let ts_nano = batch
        .column(3)
        .as_any()
        .downcast_ref::<TimestampNanosecondArray>()
        .unwrap();
    assert!(!ts_nano.is_null(0));
    assert_eq!(ts_nano.value(0), now.timestamp_nanos_opt().unwrap());
}

#[test]
fn test_encode_mixed_timestamp_string_and_native() {
    // Test mixing string timestamps with native Timestamp values
    let mut log1 = LogEvent::default();
    log1.insert("ts", "2025-10-22T10:18:44.256Z"); // String

    let mut log2 = LogEvent::default();
    log2.insert("ts", Utc::now()); // Native Timestamp

    let mut log3 = LogEvent::default();
    log3.insert("ts", 1729594724256000000_i64); // Integer (nanoseconds)

    let events = vec![Event::Log(log1), Event::Log(log2), Event::Log(log3)];

    let schema = Arc::new(Schema::new(vec![Field::new(
        "ts",
        DataType::Timestamp(TimeUnit::Nanosecond, None),
        true,
    )]));

    let result = encode_events_to_arrow_ipc_stream(&events, Some(Arc::clone(&schema)));
    assert!(result.is_ok());

    let bytes = result.unwrap();
    let cursor = Cursor::new(bytes);
    let mut reader = StreamReader::try_new(cursor, None).unwrap();
    let batch = reader.next().unwrap().unwrap();

    assert_eq!(batch.num_rows(), 3);

    let ts_array = batch
        .column(0)
        .as_any()
        .downcast_ref::<TimestampNanosecondArray>()
        .unwrap();

    // All three should be non-null
    assert!(!ts_array.is_null(0));
    assert!(!ts_array.is_null(1));
    assert!(!ts_array.is_null(2));

    // First one should match the parsed string
    let expected = chrono::DateTime::parse_from_rfc3339("2025-10-22T10:18:44.256Z")
        .unwrap()
        .timestamp_nanos_opt()
        .unwrap();
    assert_eq!(ts_array.value(0), expected);

    // Third one should match the integer
    assert_eq!(ts_array.value(2), 1729594724256000000_i64);
}

#[test]
fn test_encode_invalid_string_timestamp() {
    // Test that invalid timestamp strings become null
    let mut log1 = LogEvent::default();
    log1.insert("timestamp", "not-a-timestamp");

    let mut log2 = LogEvent::default();
    log2.insert("timestamp", "2025-10-22T10:18:44.256Z"); // Valid

    let mut log3 = LogEvent::default();
    log3.insert("timestamp", "2025-99-99T99:99:99Z"); // Invalid

    let events = vec![Event::Log(log1), Event::Log(log2), Event::Log(log3)];

    let schema = Arc::new(Schema::new(vec![Field::new(
        "timestamp",
        DataType::Timestamp(TimeUnit::Nanosecond, None),
        true,
    )]));

    let result = encode_events_to_arrow_ipc_stream(&events, Some(Arc::clone(&schema)));
    assert!(result.is_ok());

    let bytes = result.unwrap();
    let cursor = Cursor::new(bytes);
    let mut reader = StreamReader::try_new(cursor, None).unwrap();
    let batch = reader.next().unwrap().unwrap();

    assert_eq!(batch.num_rows(), 3);

    let ts_array = batch
        .column(0)
        .as_any()
        .downcast_ref::<TimestampNanosecondArray>()
        .unwrap();

    // Invalid timestamps should be null
    assert!(ts_array.is_null(0));
    assert!(!ts_array.is_null(1)); // Valid one
    assert!(ts_array.is_null(2));
}

#[test]
fn test_encode_decimal128_from_integer() {
    let mut log = LogEvent::default();
    // Store quantity as integer: 1000
    log.insert("quantity", 1000_i64);

    let events = vec![Event::Log(log)];

    // Decimal(10, 3) - will represent 1000 as 1000.000
    let schema = Arc::new(Schema::new(vec![Field::new(
        "quantity",
        DataType::Decimal128(10, 3),
        true,
    )]));

    let result = encode_events_to_arrow_ipc_stream(&events, Some(Arc::clone(&schema)));
    assert!(result.is_ok());

    let bytes = result.unwrap();
    let cursor = Cursor::new(bytes);
    let mut reader = StreamReader::try_new(cursor, None).unwrap();
    let batch = reader.next().unwrap().unwrap();

    assert_eq!(batch.num_rows(), 1);

    let decimal_array = batch
        .column(0)
        .as_any()
        .downcast_ref::<Decimal128Array>()
        .unwrap();

    assert!(!decimal_array.is_null(0));
    // 1000 with scale 3 = 1000 * 10^3 = 1000000
    assert_eq!(decimal_array.value(0), 1000000_i128);
}

#[test]
fn test_encode_decimal256() {
    let mut log = LogEvent::default();
    // Very large precision number
    log.insert("big_value", 123456789.123456_f64);

    let events = vec![Event::Log(log)];

    // Decimal256(50, 6) - high precision decimal
    let schema = Arc::new(Schema::new(vec![Field::new(
        "big_value",
        DataType::Decimal256(50, 6),
        true,
    )]));

    let result = encode_events_to_arrow_ipc_stream(&events, Some(Arc::clone(&schema)));
    assert!(result.is_ok());

    let bytes = result.unwrap();
    let cursor = Cursor::new(bytes);
    let mut reader = StreamReader::try_new(cursor, None).unwrap();
    let batch = reader.next().unwrap().unwrap();

    assert_eq!(batch.num_rows(), 1);

    let decimal_array = batch
        .column(0)
        .as_any()
        .downcast_ref::<Decimal256Array>()
        .unwrap();

    assert!(!decimal_array.is_null(0));
    // Value should be non-null and encoded
    let value = decimal_array.value(0);
    assert!(value.to_i128().is_some());
}

#[test]
fn test_encode_decimal_null_values() {
    let mut log1 = LogEvent::default();
    log1.insert("price", 99.99_f64);

    let log2 = LogEvent::default();
    // No price field - should be null

    let mut log3 = LogEvent::default();
    log3.insert("price", 50.00_f64);

    let events = vec![Event::Log(log1), Event::Log(log2), Event::Log(log3)];

    let schema = Arc::new(Schema::new(vec![Field::new(
        "price",
        DataType::Decimal128(10, 2),
        true,
    )]));

    let result = encode_events_to_arrow_ipc_stream(&events, Some(Arc::clone(&schema)));
    assert!(result.is_ok());

    let bytes = result.unwrap();
    let cursor = Cursor::new(bytes);
    let mut reader = StreamReader::try_new(cursor, None).unwrap();
    let batch = reader.next().unwrap().unwrap();

    assert_eq!(batch.num_rows(), 3);

    let decimal_array = batch
        .column(0)
        .as_any()
        .downcast_ref::<Decimal128Array>()
        .unwrap();

    // First row: 99.99
    assert!(!decimal_array.is_null(0));
    assert_eq!(decimal_array.value(0), 9999_i128);

    // Second row: null
    assert!(decimal_array.is_null(1));

    // Third row: 50.00
    assert!(!decimal_array.is_null(2));
    assert_eq!(decimal_array.value(2), 5000_i128);
}

#[test]
fn test_encode_unsigned_integer_types() {
    let mut log = LogEvent::default();
    log.insert("uint8_field", 255_i64);
    log.insert("uint16_field", 65535_i64);
    log.insert("uint32_field", 4294967295_i64);
    log.insert("uint64_field", 9223372036854775807_i64);

    let events = vec![Event::Log(log)];

    let schema = Arc::new(Schema::new(vec![
        Field::new("uint8_field", DataType::UInt8, true),
        Field::new("uint16_field", DataType::UInt16, true),
        Field::new("uint32_field", DataType::UInt32, true),
        Field::new("uint64_field", DataType::UInt64, true),
    ]));

    let result = encode_events_to_arrow_ipc_stream(&events, Some(Arc::clone(&schema)));
    assert!(result.is_ok());

    let bytes = result.unwrap();
    let cursor = Cursor::new(bytes);
    let mut reader = StreamReader::try_new(cursor, None).unwrap();
    let batch = reader.next().unwrap().unwrap();

    assert_eq!(batch.num_rows(), 1);
    assert_eq!(batch.num_columns(), 4);

    // Verify uint8
    let uint8_array = batch
        .column(0)
        .as_any()
        .downcast_ref::<UInt8Array>()
        .unwrap();
    assert_eq!(uint8_array.value(0), 255_u8);

    // Verify uint16
    let uint16_array = batch
        .column(1)
        .as_any()
        .downcast_ref::<UInt16Array>()
        .unwrap();
    assert_eq!(uint16_array.value(0), 65535_u16);

    // Verify uint32
    let uint32_array = batch
        .column(2)
        .as_any()
        .downcast_ref::<UInt32Array>()
        .unwrap();
    assert_eq!(uint32_array.value(0), 4294967295_u32);

    // Verify uint64
    let uint64_array = batch
        .column(3)
        .as_any()
        .downcast_ref::<UInt64Array>()
        .unwrap();
    assert_eq!(uint64_array.value(0), 9223372036854775807_u64);
}

#[test]
fn test_encode_unsigned_integers_with_null_and_overflow() {
    let mut log1 = LogEvent::default();
    log1.insert("uint8_field", 100_i64);
    log1.insert("uint32_field", 1000_i64);

    let mut log2 = LogEvent::default();
    log2.insert("uint8_field", 300_i64); // Overflow - should be null
    log2.insert("uint32_field", -1_i64); // Negative - should be null

    let log3 = LogEvent::default();
    // Missing fields - should be null

    let events = vec![Event::Log(log1), Event::Log(log2), Event::Log(log3)];

    let schema = Arc::new(Schema::new(vec![
        Field::new("uint8_field", DataType::UInt8, true),
        Field::new("uint32_field", DataType::UInt32, true),
    ]));

    let result = encode_events_to_arrow_ipc_stream(&events, Some(Arc::clone(&schema)));
    assert!(result.is_ok());

    let bytes = result.unwrap();
    let cursor = Cursor::new(bytes);
    let mut reader = StreamReader::try_new(cursor, None).unwrap();
    let batch = reader.next().unwrap().unwrap();

    assert_eq!(batch.num_rows(), 3);

    // Check uint8 column
    let uint8_array = batch
        .column(0)
        .as_any()
        .downcast_ref::<UInt8Array>()
        .unwrap();
    assert_eq!(uint8_array.value(0), 100_u8); // Valid
    assert!(uint8_array.is_null(1)); // Overflow
    assert!(uint8_array.is_null(2)); // Missing

    // Check uint32 column
    let uint32_array = batch
        .column(1)
        .as_any()
        .downcast_ref::<UInt32Array>()
        .unwrap();
    assert_eq!(uint32_array.value(0), 1000_u32); // Valid
    assert!(uint32_array.is_null(1)); // Negative
    assert!(uint32_array.is_null(2)); // Missing
}

#[test]
fn test_encode_non_nullable_field_with_null_value() {
    // Test that encoding fails when a non-nullable field encounters a null value
    let mut log1 = LogEvent::default();
    log1.insert("required_field", 42);

    let log2 = LogEvent::default();
    // log2 is missing required_field - should cause an error

    let events = vec![Event::Log(log1), Event::Log(log2)];

    // Create schema with non-nullable field
    let schema = Arc::new(Schema::new(vec![Field::new(
        "required_field",
        DataType::Int64,
        false, // Not nullable
    )]));

    let result = encode_events_to_arrow_ipc_stream(&events, Some(schema));
    assert!(result.is_err());

    match result.unwrap_err() {
        ArrowEncodingError::NullConstraint { field_name } => {
            assert_eq!(field_name, "required_field");
        }
        other => panic!("Expected NullConstraint error, got: {:?}", other),
    }
}

#[test]
fn test_encode_non_nullable_string_field_with_missing_value() {
    // Test that encoding fails for non-nullable string field
    let mut log1 = LogEvent::default();
    log1.insert("name", "Alice");

    let mut log2 = LogEvent::default();
    log2.insert("name", "Bob");

    let log3 = LogEvent::default();
    // log3 is missing name field

    let events = vec![Event::Log(log1), Event::Log(log2), Event::Log(log3)];

    let schema = Arc::new(Schema::new(vec![Field::new(
        "name",
        DataType::Utf8,
        false, // Not nullable
    )]));

    let result = encode_events_to_arrow_ipc_stream(&events, Some(schema));
    assert!(result.is_err());

    match result.unwrap_err() {
        ArrowEncodingError::NullConstraint { field_name } => {
            assert_eq!(field_name, "name");
        }
        other => panic!("Expected NullConstraint error, got: {:?}", other),
    }
}

#[test]
fn test_encode_non_nullable_field_all_values_present() {
    // Test that encoding succeeds when all values are present for non-nullable field
    let mut log1 = LogEvent::default();
    log1.insert("id", 1);

    let mut log2 = LogEvent::default();
    log2.insert("id", 2);

    let mut log3 = LogEvent::default();
    log3.insert("id", 3);

    let events = vec![Event::Log(log1), Event::Log(log2), Event::Log(log3)];

    let schema = Arc::new(Schema::new(vec![Field::new(
        "id",
        DataType::Int64,
        false, // Not nullable
    )]));

    let result = encode_events_to_arrow_ipc_stream(&events, Some(Arc::clone(&schema)));
    assert!(result.is_ok());

    let bytes = result.unwrap();
    let cursor = Cursor::new(bytes);
    let mut reader = StreamReader::try_new(cursor, None).unwrap();
    let batch = reader.next().unwrap().unwrap();

    assert_eq!(batch.num_rows(), 3);

    let id_array = batch
        .column(0)
        .as_any()
        .downcast_ref::<Int64Array>()
        .unwrap();

    assert_eq!(id_array.value(0), 1);
    assert_eq!(id_array.value(1), 2);
    assert_eq!(id_array.value(2), 3);
    assert!(!id_array.is_null(0));
    assert!(!id_array.is_null(1));
    assert!(!id_array.is_null(2));
}

#[test]
fn test_config_allow_nullable_fields_overrides_schema() {
    // Create events: One valid, one missing the "required" field
    let mut log1 = LogEvent::default();
    log1.insert("strict_field", 42);
    let log2 = LogEvent::default();
    let events = vec![Event::Log(log1), Event::Log(log2)];

    let schema = Schema::new(vec![Field::new("strict_field", DataType::Int64, false)]);

    let mut config = ArrowStreamSerializerConfig::new(schema);
    config.allow_nullable_fields = true;

    let mut serializer = ArrowStreamSerializer::new(config).expect("Failed to create serializer");

    let mut buffer = BytesMut::new();
    serializer
        .encode(events, &mut buffer)
        .expect("Encoding should succeed when allow_nullable_fields is true");

    let cursor = Cursor::new(buffer);
    let mut reader = StreamReader::try_new(cursor, None).expect("Failed to create reader");
    let batch = reader.next().unwrap().expect("Failed to read batch");

    assert_eq!(batch.num_rows(), 2);

    let binding = batch.schema();
    let output_field = binding.field(0);
    assert!(
        output_field.is_nullable(),
        "The output schema field should have been transformed to nullable=true"
    );

    let array = batch
        .column(0)
        .as_any()
        .downcast_ref::<Int64Array>()
        .unwrap();

    assert_eq!(array.value(0), 42);
    assert!(!array.is_null(0));
    assert!(
        array.is_null(1),
        "The missing value should be encoded as null"
    );
}

#[test]
fn test_make_field_nullable_with_nested_types() {
    use crate::encoding::format::arrow::make_field_nullable;

    // Test that make_field_nullable recursively handles List and Struct types

    // Create a nested structure: Struct containing a List of Structs
    // struct { inner_list: [{ nested_field: Int64 }] }
    let inner_struct_field = Field::new("nested_field", DataType::Int64, false);
    let inner_struct = DataType::Struct(arrow::datatypes::Fields::from(vec![inner_struct_field]));
    let list_field = Field::new("item", inner_struct, false);
    let list_type = DataType::List(Arc::new(list_field));
    let outer_field = Field::new("inner_list", list_type, false);
    let outer_struct = DataType::Struct(arrow::datatypes::Fields::from(vec![outer_field]));

    let original_field = Field::new("root", outer_struct, false);

    // Apply make_field_nullable
    let nullable_field = make_field_nullable(&original_field);

    // Verify root field is nullable
    assert!(
        nullable_field.is_nullable(),
        "Root field should be nullable"
    );

    // Verify nested struct is nullable
    if let DataType::Struct(root_fields) = nullable_field.data_type() {
        let inner_list_field = &root_fields[0];
        assert!(
            inner_list_field.is_nullable(),
            "inner_list field should be nullable"
        );

        // Verify list element is nullable
        if let DataType::List(list_item_field) = inner_list_field.data_type() {
            assert!(
                list_item_field.is_nullable(),
                "List item field should be nullable"
            );

            // Verify inner struct fields are nullable
            if let DataType::Struct(inner_struct_fields) = list_item_field.data_type() {
                let nested_field = &inner_struct_fields[0];
                assert!(
                    nested_field.is_nullable(),
                    "nested_field should be nullable"
                );
            } else {
                panic!("Expected Struct type for list items");
            }
        } else {
            panic!("Expected List type for inner_list");
        }
    } else {
        panic!("Expected Struct type for root field");
    }
}

#[test]
fn test_make_field_nullable_with_map_type() {
    use crate::encoding::format::arrow::make_field_nullable;

    // Test that make_field_nullable handles Map types
    // Map is internally represented as List<Struct<key, value>>

    // Create a map: Map<Utf8, Int64>
    // Internally: List<Struct<entries: {key: Utf8, value: Int64}>>
    let key_field = Field::new("key", DataType::Utf8, false);
    let value_field = Field::new("value", DataType::Int64, false);
    let entries_struct =
        DataType::Struct(arrow::datatypes::Fields::from(vec![key_field, value_field]));
    let entries_field = Field::new("entries", entries_struct, false);
    let map_type = DataType::Map(Arc::new(entries_field), false);

    let original_field = Field::new("my_map", map_type, false);

    // Apply make_field_nullable
    let nullable_field = make_field_nullable(&original_field);

    // Verify root field is nullable
    assert!(
        nullable_field.is_nullable(),
        "Root map field should be nullable"
    );

    // Verify map entries are nullable
    if let DataType::Map(entries_field, _sorted) = nullable_field.data_type() {
        assert!(
            entries_field.is_nullable(),
            "Map entries field should be nullable"
        );

        // Verify the struct inside the map is nullable
        if let DataType::Struct(struct_fields) = entries_field.data_type() {
            let key_field = &struct_fields[0];
            let value_field = &struct_fields[1];
            assert!(key_field.is_nullable(), "Map key field should be nullable");
            assert!(
                value_field.is_nullable(),
                "Map value field should be nullable"
            );
        } else {
            panic!("Expected Struct type for map entries");
        }
    } else {
        panic!("Expected Map type for my_map field");
    }
}
