use super::*;
use arrow::{
    array::{
        Array, ArrayRef, BinaryArray, BooleanArray, Float64Array, Int64Array, ListArray, MapArray,
        StringArray, TimestampMicrosecondArray, TimestampMillisecondArray,
        TimestampNanosecondArray, TimestampSecondArray,
    },
    datatypes::{DataType, Field, Fields, Schema, SchemaRef, TimeUnit},
    ipc::reader::StreamReader,
    record_batch::RecordBatch,
};
use chrono::Utc;
use std::{io::Cursor, sync::Arc};
use vector_core::event::{Event, LogEvent, Value};

/// Helper to encode events and return the decoded RecordBatch
fn encode_and_decode(
    events: Vec<Event>,
    schema: SchemaRef,
) -> Result<RecordBatch, Box<dyn std::error::Error>> {
    let bytes = encode_events_to_arrow_ipc_stream(&events, Some(Arc::clone(&schema)))?;
    let cursor = Cursor::new(bytes);
    let mut reader = StreamReader::try_new(cursor, None)?;
    Ok(reader.next().unwrap()?)
}

/// Create a simple event from key-value pairs
fn create_event<V>(fields: Vec<(&str, V)>) -> Event
where
    V: Into<Value>,
{
    let mut log = LogEvent::default();
    for (key, value) in fields {
        log.insert(key, value.into());
    }
    Event::Log(log)
}

/// Assert a column has expected integer values (with optional nulls)
fn assert_int64_column(batch: &RecordBatch, col_index: usize, expected: &[Option<i64>]) {
    let array = batch
        .column(col_index)
        .as_any()
        .downcast_ref::<Int64Array>()
        .expect("Expected Int64Array");

    assert_eq!(
        array.len(),
        expected.len(),
        "Array length mismatch at column {}",
        col_index
    );

    for (i, &expected_val) in expected.iter().enumerate() {
        match expected_val {
            Some(val) => {
                assert!(
                    !array.is_null(i),
                    "Expected value {} at index {}, got null",
                    val,
                    i
                );
                assert_eq!(array.value(i), val, "Value mismatch at index {}", i);
            }
            None => assert!(array.is_null(i), "Expected null at index {}, got value", i),
        }
    }
}

/// Create a schema with a single field
fn single_field_schema(name: &str, data_type: DataType, nullable: bool) -> SchemaRef {
    SchemaRef::new(Schema::new(vec![Field::new(name, data_type, nullable)]))
}

/// Assert a primitive value at a specific column and row
macro_rules! assert_primitive_value {
    ($batch:expr, $col:expr, $row:expr, $array_type:ty, $expected:expr) => {
        assert_eq!(
            $batch
                .column($col)
                .as_any()
                .downcast_ref::<$array_type>()
                .unwrap()
                .value($row),
            $expected
        )
    };
}

mod comprehensive {
    use super::*;

    #[test]
    fn test_encode_all_types() {
        use arrow::array::{
            Decimal128Array, ListArray, MapArray, UInt8Array, UInt16Array, UInt32Array, UInt64Array,
        };
        use vrl::value::ObjectMap;

        let now = Utc::now();

        // Create a struct (tuple) value
        let mut tuple_value = ObjectMap::new();
        tuple_value.insert("f0".into(), Value::Bytes("nested_str".into()));
        tuple_value.insert("f1".into(), Value::Integer(999));

        // Create a list value
        let list_value = Value::Array(vec![
            Value::Integer(1),
            Value::Integer(2),
            Value::Integer(3),
        ]);

        // Create a map value
        let mut map_value = ObjectMap::new();
        map_value.insert("key1".into(), Value::Integer(100));
        map_value.insert("key2".into(), Value::Integer(200));

        let mut log = LogEvent::default();
        // Primitive types
        log.insert("string_field", "test");
        log.insert("int8_field", 127);
        log.insert("int16_field", 32000);
        log.insert("int32_field", 1000000);
        log.insert("int64_field", 42);
        log.insert("uint8_field", 255);
        log.insert("uint16_field", 65535);
        log.insert("uint32_field", 4000000);
        log.insert("uint64_field", 9000000000_i64);
        log.insert("float32_field", 3.15);
        log.insert("float64_field", 3.15);
        log.insert("bool_field", true);
        log.insert("bytes_field", bytes::Bytes::from("binary"));
        log.insert("timestamp_field", now);
        log.insert("decimal_field", 99.99);
        // Complex types
        log.insert("list_field", list_value);
        log.insert("struct_field", Value::Object(tuple_value));
        log.insert("map_field", Value::Object(map_value));

        let events = vec![Event::Log(log)];

        // Build schema with all supported types
        let struct_fields = arrow::datatypes::Fields::from(vec![
            Field::new("f0", DataType::Utf8, true),
            Field::new("f1", DataType::Int64, true),
        ]);

        let map_entries = Field::new(
            "entries",
            DataType::Struct(arrow::datatypes::Fields::from(vec![
                Field::new("keys", DataType::Utf8, false),
                Field::new("values", DataType::Int64, true),
            ])),
            false,
        );

        let schema = SchemaRef::new(Schema::new(vec![
            Field::new("string_field", DataType::Utf8, true),
            Field::new("int8_field", DataType::Int8, true),
            Field::new("int16_field", DataType::Int16, true),
            Field::new("int32_field", DataType::Int32, true),
            Field::new("int64_field", DataType::Int64, true),
            Field::new("uint8_field", DataType::UInt8, true),
            Field::new("uint16_field", DataType::UInt16, true),
            Field::new("uint32_field", DataType::UInt32, true),
            Field::new("uint64_field", DataType::UInt64, true),
            Field::new("float32_field", DataType::Float32, true),
            Field::new("float64_field", DataType::Float64, true),
            Field::new("bool_field", DataType::Boolean, true),
            Field::new("bytes_field", DataType::Binary, true),
            Field::new(
                "timestamp_field",
                DataType::Timestamp(TimeUnit::Millisecond, None),
                true,
            ),
            Field::new("decimal_field", DataType::Decimal128(10, 2), true),
            Field::new(
                "list_field",
                DataType::List(Field::new("item", DataType::Int64, true).into()),
                true,
            ),
            Field::new("struct_field", DataType::Struct(struct_fields), true),
            Field::new("map_field", DataType::Map(map_entries.into(), false), true),
        ]));

        let batch = encode_and_decode(events, schema).expect("Failed to encode");

        assert_eq!(batch.num_rows(), 1);
        assert_eq!(batch.num_columns(), 18);

        // Verify all primitive types
        assert_eq!(
            batch
                .column(0)
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap()
                .value(0),
            "test"
        );
        assert_primitive_value!(batch, 1, 0, arrow::array::Int8Array, 127);
        assert_primitive_value!(batch, 2, 0, arrow::array::Int16Array, 32000);
        assert_primitive_value!(batch, 3, 0, arrow::array::Int32Array, 1000000);
        assert_primitive_value!(batch, 4, 0, Int64Array, 42);
        assert_primitive_value!(batch, 5, 0, UInt8Array, 255);
        assert_primitive_value!(batch, 6, 0, UInt16Array, 65535);
        assert_primitive_value!(batch, 7, 0, UInt32Array, 4000000);
        assert_primitive_value!(batch, 8, 0, UInt64Array, 9000000000);
        assert!(
            (batch
                .column(9)
                .as_any()
                .downcast_ref::<arrow::array::Float32Array>()
                .unwrap()
                .value(0)
                - 3.15)
                .abs()
                < 0.001
        );
        assert!(
            (batch
                .column(10)
                .as_any()
                .downcast_ref::<Float64Array>()
                .unwrap()
                .value(0)
                - 3.15)
                .abs()
                < 0.001
        );
        assert!(
            batch
                .column(11)
                .as_any()
                .downcast_ref::<BooleanArray>()
                .unwrap()
                .value(0)
        );
        assert_primitive_value!(batch, 12, 0, BinaryArray, b"binary");
        assert_primitive_value!(
            batch,
            13,
            0,
            TimestampMillisecondArray,
            now.timestamp_millis()
        );
        assert_primitive_value!(batch, 14, 0, Decimal128Array, 9999);

        let list_array = batch
            .column(15)
            .as_any()
            .downcast_ref::<ListArray>()
            .unwrap();
        assert!(!list_array.is_null(0));
        let list_value = list_array.value(0);
        assert_eq!(list_value.len(), 3);
        let int_array = list_value.as_any().downcast_ref::<Int64Array>().unwrap();
        assert_eq!(int_array.value(0), 1);
        assert_eq!(int_array.value(1), 2);
        assert_eq!(int_array.value(2), 3);

        // Verify struct field
        let struct_array = batch
            .column(16)
            .as_any()
            .downcast_ref::<arrow::array::StructArray>()
            .unwrap();
        assert!(!struct_array.is_null(0));
        assert_primitive_value!(struct_array, 0, 0, StringArray, "nested_str");
        assert_primitive_value!(struct_array, 1, 0, Int64Array, 999);

        // Verify map field
        let map_array = batch
            .column(17)
            .as_any()
            .downcast_ref::<MapArray>()
            .unwrap();
        assert!(!map_array.is_null(0));
        let map_value = map_array.value(0);
        assert_eq!(map_value.len(), 2);
    }
}

mod edge_cases {
    use super::*;

    #[test]
    fn test_encode_null_values() {
        let events = vec![
            create_event(vec![("field_a", 1_i64)]),
            create_event(vec![("field_b", 2_i64)]),
        ];

        let schema = SchemaRef::new(Schema::new(vec![
            Field::new("field_a", DataType::Int64, true),
            Field::new("field_b", DataType::Int64, true),
        ]));

        let batch = encode_and_decode(events, schema).unwrap();

        assert_eq!(batch.num_rows(), 2);
        assert_int64_column(&batch, 0, &[Some(1), None]);
        assert_int64_column(&batch, 1, &[None, Some(2)]);
    }

    #[test]
    fn test_encode_type_mismatches() {
        let events = vec![
            create_event(vec![("field", 42_i64)]),
            create_event(vec![("field", 3.15_f64)]), // Type mismatch!
        ];

        let schema = single_field_schema("field", DataType::Int64, true);
        let batch = encode_and_decode(events, schema).unwrap();

        assert_eq!(batch.num_rows(), 2);
        // Type mismatch becomes null
        assert_int64_column(&batch, 0, &[Some(42), None]);
    }

    #[test]
    fn test_encode_empty_arrays_and_maps() {
        use arrow::array::{ListArray, MapArray};
        use vrl::value::ObjectMap;

        let empty_array = Vec::<Value>::new();
        let empty_map = ObjectMap::new();

        let mut log = LogEvent::default();
        log.insert("empty_array", Value::Array(empty_array));
        log.insert("empty_map", Value::Object(empty_map));
        log.insert(
            "non_empty_array",
            Value::Array(vec![Value::Integer(1), Value::Integer(2)]),
        );

        let events = vec![Event::Log(log)];

        let array_field = Field::new("item", DataType::Int32, true);
        let map_entries = Field::new(
            "entries",
            DataType::Struct(arrow::datatypes::Fields::from(vec![
                Field::new("keys", DataType::Utf8, false),
                Field::new("values", DataType::Int32, true),
            ])),
            false,
        );

        let schema = SchemaRef::new(Schema::new(vec![
            Field::new(
                "empty_array",
                DataType::List(array_field.clone().into()),
                true,
            ),
            Field::new("empty_map", DataType::Map(map_entries.into(), false), true),
            Field::new("non_empty_array", DataType::List(array_field.into()), true),
        ]));

        let batch = encode_and_decode(events, schema).unwrap();

        assert_eq!(batch.num_rows(), 1);
        assert_eq!(batch.num_columns(), 3);

        // Verify empty array
        let empty_array = batch
            .column(0)
            .as_any()
            .downcast_ref::<ListArray>()
            .unwrap();
        assert!(!empty_array.is_null(0));
        assert_eq!(empty_array.value(0).len(), 0);

        // Verify empty map
        let empty_map = batch.column(1).as_any().downcast_ref::<MapArray>().unwrap();
        assert!(!empty_map.is_null(0));
        assert_eq!(empty_map.value(0).len(), 0);

        // Verify non-empty array
        let non_empty_array = batch
            .column(2)
            .as_any()
            .downcast_ref::<ListArray>()
            .unwrap();
        assert!(!non_empty_array.is_null(0));
        assert_eq!(non_empty_array.value(0).len(), 2);
    }
}

mod json_serialization {
    use super::*;

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

        let schema = SchemaRef::new(Schema::new(vec![
            Field::new("object_field", DataType::Utf8, true),
            Field::new("array_field", DataType::Utf8, true),
        ]));

        let batch = encode_and_decode(events, schema).unwrap();

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
        assert_eq!(array_array.value(0), "[1,2,3]");
    }
}

mod error_handling {
    use super::*;

    #[test]
    fn test_encode_unsupported_type() {
        let events = vec![create_event(vec![("field", "value")])];

        let schema = single_field_schema("field", DataType::Duration(TimeUnit::Millisecond), true);

        let result = encode_events_to_arrow_ipc_stream(&events, Some(schema));
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ArrowEncodingError::UnsupportedType { .. }
        ));
    }

    #[test]
    fn test_encode_without_schema_fails() {
        let events = vec![create_event(vec![("message", "hello")])];

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
}

mod temporal_types {
    use super::*;

    #[test]
    fn test_encode_timestamp_precisions() {
        let now = Utc::now();
        let mut log = LogEvent::default();
        log.insert("ts_second", now);
        log.insert("ts_milli", now);
        log.insert("ts_micro", now);
        log.insert("ts_nano", now);

        let events = vec![Event::Log(log)];

        let schema = SchemaRef::new(Schema::new(vec![
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

        let batch = encode_and_decode(events, schema).unwrap();

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

        let schema = SchemaRef::new(Schema::new(vec![Field::new(
            "ts",
            DataType::Timestamp(TimeUnit::Nanosecond, None),
            true,
        )]));

        let batch = encode_and_decode(events, schema).unwrap();

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

        let schema = SchemaRef::new(Schema::new(vec![Field::new(
            "timestamp",
            DataType::Timestamp(TimeUnit::Nanosecond, None),
            true,
        )]));

        let batch = encode_and_decode(events, schema).unwrap();

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
}

mod decimal_types {
    use super::*;

    #[test]
    fn test_encode_decimal128_from_integer() {
        use arrow::array::Decimal128Array;

        let mut log = LogEvent::default();
        // Store quantity as integer: 1000
        log.insert("quantity", 1000_i64);

        let events = vec![Event::Log(log)];

        // Decimal(10, 3) - will represent 1000 as 1000.000
        let schema = SchemaRef::new(Schema::new(vec![Field::new(
            "quantity",
            DataType::Decimal128(10, 3),
            true,
        )]));

        let batch = encode_and_decode(events, schema).unwrap();

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
        use arrow::array::Decimal256Array;

        let mut log = LogEvent::default();
        // Very large precision number
        log.insert("big_value", 123456789.123456_f64);

        let events = vec![Event::Log(log)];

        // Decimal256(50, 6) - high precision decimal
        let schema = SchemaRef::new(Schema::new(vec![Field::new(
            "big_value",
            DataType::Decimal256(50, 6),
            true,
        )]));

        let batch = encode_and_decode(events, schema).unwrap();

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
        use arrow::array::Decimal128Array;

        let mut log1 = LogEvent::default();
        log1.insert("price", 99.99_f64);

        let log2 = LogEvent::default();
        // No price field - should be null

        let mut log3 = LogEvent::default();
        log3.insert("price", 50.00_f64);

        let events = vec![Event::Log(log1), Event::Log(log2), Event::Log(log3)];

        let schema = SchemaRef::new(Schema::new(vec![Field::new(
            "price",
            DataType::Decimal128(10, 2),
            true,
        )]));

        let batch = encode_and_decode(events, schema).unwrap();

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
}

mod primitive_types {
    use super::*;

    #[test]
    fn test_encode_unsigned_integers_with_null_and_overflow() {
        use arrow::array::{UInt8Array, UInt32Array};

        let mut log1 = LogEvent::default();
        log1.insert("uint8_field", 100_i64);
        log1.insert("uint32_field", 1000_i64);

        let mut log2 = LogEvent::default();
        log2.insert("uint8_field", 300_i64); // Overflow - should be null
        log2.insert("uint32_field", -1_i64); // Negative - should be null

        let log3 = LogEvent::default();
        // Missing fields - should be null

        let events = vec![Event::Log(log1), Event::Log(log2), Event::Log(log3)];

        let schema = SchemaRef::new(Schema::new(vec![
            Field::new("uint8_field", DataType::UInt8, true),
            Field::new("uint32_field", DataType::UInt32, true),
        ]));

        let batch = encode_and_decode(events, schema).unwrap();

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
        let events = vec![
            create_event(vec![("required_field", 42_i64)]),
            LogEvent::default().into(), // Missing required field
        ];

        let schema = single_field_schema("required_field", DataType::Int64, false);
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
    fn test_encode_non_nullable_field_all_values_present() {
        let events = vec![
            create_event(vec![("id", 1_i64)]),
            create_event(vec![("id", 2_i64)]),
            create_event(vec![("id", 3_i64)]),
        ];

        let schema = single_field_schema("id", DataType::Int64, false);
        let batch = encode_and_decode(events, schema).unwrap();

        assert_eq!(batch.num_rows(), 3);
        assert_int64_column(&batch, 0, &[Some(1), Some(2), Some(3)]);
    }
}

mod config_tests {
    use super::*;
    use tokio_util::codec::Encoder;

    #[test]
    fn test_config_allow_nullable_fields_overrides_schema() {
        let mut log1 = LogEvent::default();
        log1.insert("strict_field", 42);
        let log2 = LogEvent::default();
        let events = vec![Event::Log(log1), Event::Log(log2)];

        let schema = Schema::new(vec![Field::new("strict_field", DataType::Int64, false)]);

        let mut config = ArrowStreamSerializerConfig::new(schema);
        config.allow_nullable_fields = true;

        let mut serializer =
            ArrowStreamSerializer::new(config).expect("Failed to create serializer");

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
        let inner_struct_field = Field::new("nested_field", DataType::Int64, false);
        let inner_struct =
            DataType::Struct(arrow::datatypes::Fields::from(vec![inner_struct_field]));
        let list_field = Field::new("item", inner_struct, false);
        let list_type = DataType::List(list_field.into());
        let outer_field = Field::new("inner_list", list_type, false);
        let outer_struct = DataType::Struct(arrow::datatypes::Fields::from(vec![outer_field]));

        let original_field = Field::new("root", outer_struct, false);
        let nullable_field = make_field_nullable(&original_field);

        assert!(
            nullable_field.is_nullable(),
            "Root field should be nullable"
        );

        if let DataType::Struct(root_fields) = nullable_field.data_type() {
            let inner_list_field = &root_fields[0];
            assert!(inner_list_field.is_nullable());

            if let DataType::List(list_item_field) = inner_list_field.data_type() {
                assert!(list_item_field.is_nullable());

                if let DataType::Struct(inner_struct_fields) = list_item_field.data_type() {
                    let nested_field = &inner_struct_fields[0];
                    assert!(nested_field.is_nullable());
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
        let key_field = Field::new("key", DataType::Utf8, false);
        let value_field = Field::new("value", DataType::Int64, false);
        let entries_struct =
            DataType::Struct(arrow::datatypes::Fields::from(vec![key_field, value_field]));
        let entries_field = Field::new("entries", entries_struct, false);
        let map_type = DataType::Map(entries_field.into(), false);

        let original_field = Field::new("my_map", map_type, false);
        let nullable_field = make_field_nullable(&original_field);

        assert!(
            nullable_field.is_nullable(),
            "Root map field should be nullable"
        );

        if let DataType::Map(entries_field, _sorted) = nullable_field.data_type() {
            assert!(
                !entries_field.is_nullable(),
                "Map entries field should be non-nullable"
            );

            if let DataType::Struct(struct_fields) = entries_field.data_type() {
                let key_field = &struct_fields[0];
                let value_field = &struct_fields[1];
                assert!(
                    !key_field.is_nullable(),
                    "Map key field should be non-nullable"
                );
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
}

mod nested_types {
    use super::*;

    #[test]
    fn test_encode_nested_maps() {
        use arrow::array::MapArray;
        use vrl::value::ObjectMap;

        // Create nested map: Map<String, Map<String, Int32>>
        // {"outer_key1": {"inner_key1": 100, "inner_key2": 200}, "outer_key2": {"inner_key3": 300}}
        let mut inner_map1 = ObjectMap::new();
        inner_map1.insert("inner_key1".into(), Value::Integer(100));
        inner_map1.insert("inner_key2".into(), Value::Integer(200));

        let mut inner_map2 = ObjectMap::new();
        inner_map2.insert("inner_key3".into(), Value::Integer(300));

        let mut outer_map = ObjectMap::new();
        outer_map.insert("outer_key1".into(), Value::Object(inner_map1));
        outer_map.insert("outer_key2".into(), Value::Object(inner_map2));

        let mut log = LogEvent::default();
        log.insert("nested_map", Value::Object(outer_map));

        let events = vec![Event::Log(log)];

        // Define schema: Map<Utf8, Map<Utf8, Int32>>
        // Note: MapBuilder uses "keys" and "values" (plural) as field names
        let inner_map_entries = Field::new(
            "entries",
            DataType::Struct(arrow::datatypes::Fields::from(vec![
                Field::new("keys", DataType::Utf8, false),
                Field::new("values", DataType::Int32, true),
            ])),
            false,
        );
        let inner_map_type = DataType::Map(inner_map_entries.into(), false);

        let outer_map_entries = Field::new(
            "entries",
            DataType::Struct(arrow::datatypes::Fields::from(vec![
                Field::new("keys", DataType::Utf8, false),
                Field::new("values", inner_map_type, true),
            ])),
            false,
        );
        let outer_map_type = DataType::Map(outer_map_entries.into(), false);

        let schema = SchemaRef::new(Schema::new(vec![Field::new(
            "nested_map",
            outer_map_type,
            true,
        )]));

        let batch = encode_and_decode(events, schema).expect("Failed to encode nested maps");

        assert_eq!(batch.num_rows(), 1);
        assert_eq!(batch.num_columns(), 1);

        // Verify the outer map exists
        let outer_map_array = batch.column(0).as_any().downcast_ref::<MapArray>().unwrap();
        assert_eq!(outer_map_array.len(), 1);
        assert!(!outer_map_array.is_null(0), "Outer map should not be null");

        // Get the outer map's values (which are inner maps)
        let outer_map_value = outer_map_array.value(0);
        assert_eq!(outer_map_value.len(), 2, "Outer map should have 2 entries");

        // The outer map's values are themselves a MapArray
        let inner_maps = outer_map_array.values();
        let inner_maps_array = inner_maps.as_any().downcast_ref::<MapArray>().unwrap();

        // Verify we have 2 inner maps (one for each outer key)
        // Total entries across both inner maps: 2 + 1 = 3
        assert_eq!(inner_maps_array.len(), 2, "Should have 2 inner maps");

        // Verify first inner map has 2 entries
        let first_inner_map = inner_maps_array.value(0);
        assert_eq!(
            first_inner_map.len(),
            2,
            "First inner map should have 2 entries"
        );

        // Verify second inner map has 1 entry
        let second_inner_map = inner_maps_array.value(1);
        assert_eq!(
            second_inner_map.len(),
            1,
            "Second inner map should have 1 entry"
        );
    }

    #[test]
    fn test_encode_array_of_maps() {
        use arrow::array::ListArray;
        use vrl::value::ObjectMap;

        // Create array of maps: Array<Map<String, Int32>>
        // [{"key1": 100, "key2": 200}, {"key3": 300}]
        let mut map1 = ObjectMap::new();
        map1.insert("key1".into(), Value::Integer(100));
        map1.insert("key2".into(), Value::Integer(200));

        let mut map2 = ObjectMap::new();
        map2.insert("key3".into(), Value::Integer(300));

        let array_of_maps = Value::Array(vec![Value::Object(map1), Value::Object(map2)]);

        let mut log = LogEvent::default();
        log.insert("array_of_maps", array_of_maps);

        let events = vec![Event::Log(log)];

        // Define schema: List<Map<Utf8, Int32>>
        // Note: MapBuilder uses "keys" and "values" (plural) as field names
        let map_entries = Field::new(
            "entries",
            DataType::Struct(arrow::datatypes::Fields::from(vec![
                Field::new("keys", DataType::Utf8, false),
                Field::new("values", DataType::Int32, true),
            ])),
            false,
        );
        let map_type = DataType::Map(map_entries.into(), false);
        let list_field = Field::new("item", map_type, true);

        let schema = SchemaRef::new(Schema::new(vec![Field::new(
            "array_of_maps",
            DataType::List(list_field.into()),
            true,
        )]));

        let batch = encode_and_decode(events, schema).expect("Failed to encode array of maps");

        assert_eq!(batch.num_rows(), 1);
        assert_eq!(batch.num_columns(), 1);

        // Verify the array exists
        let list_array = batch
            .column(0)
            .as_any()
            .downcast_ref::<ListArray>()
            .unwrap();
        assert!(!list_array.is_null(0), "Array should not be null");
        assert_eq!(list_array.value(0).len(), 2, "Array should have 2 maps");

        // Verify the maps inside the array
        let maps = list_array.value(0);
        let map_array = maps
            .as_any()
            .downcast_ref::<arrow::array::MapArray>()
            .unwrap();

        // First map should have 2 entries
        let first_map = map_array.value(0);
        assert_eq!(first_map.len(), 2, "First map should have 2 entries");

        // Second map should have 1 entry
        let second_map = map_array.value(1);
        assert_eq!(second_map.len(), 1, "Second map should have 1 entry");
    }

    #[test]
    fn test_encode_array_of_structs() {
        use arrow::array::ListArray;
        use vrl::value::ObjectMap;

        // Create array of structs (tuples): Array<Struct(String, Int32)>
        // [{"f0": "value1", "f1": 100}, {"f0": "value2", "f1": 200}]
        let mut tuple1 = ObjectMap::new();
        tuple1.insert("f0".into(), Value::Bytes("value1".into()));
        tuple1.insert("f1".into(), Value::Integer(100));

        let mut tuple2 = ObjectMap::new();
        tuple2.insert("f0".into(), Value::Bytes("value2".into()));
        tuple2.insert("f1".into(), Value::Integer(200));

        let array_of_structs = Value::Array(vec![Value::Object(tuple1), Value::Object(tuple2)]);

        let mut log = LogEvent::default();
        log.insert("array_of_structs", array_of_structs);

        let events = vec![Event::Log(log)];

        // Define schema: List<Struct(Utf8, Int32)>
        let struct_fields = arrow::datatypes::Fields::from(vec![
            Field::new("f0", DataType::Utf8, true),
            Field::new("f1", DataType::Int32, true),
        ]);
        let struct_type = DataType::Struct(struct_fields);
        let list_field = Field::new("item", struct_type, true);

        let schema = SchemaRef::new(Schema::new(vec![Field::new(
            "array_of_structs",
            DataType::List(list_field.into()),
            true,
        )]));

        let batch = encode_and_decode(events, schema).expect("Failed to encode array of structs");

        assert_eq!(batch.num_rows(), 1);
        assert_eq!(batch.num_columns(), 1);

        // Verify the array exists and has the correct number of elements
        let list_array = batch
            .column(0)
            .as_any()
            .downcast_ref::<ListArray>()
            .unwrap();
        assert!(!list_array.is_null(0), "Array should not be null");
        assert_eq!(list_array.value(0).len(), 2, "Array should have 2 structs");

        // Verify the structs inside the array
        let struct_array = list_array.value(0);
        let struct_array = struct_array
            .as_any()
            .downcast_ref::<arrow::array::StructArray>()
            .unwrap();

        // Check first struct field (f0 - strings)
        let f0_array = struct_array
            .column(0)
            .as_any()
            .downcast_ref::<arrow::array::StringArray>()
            .unwrap();
        assert_eq!(f0_array.value(0), "value1");
        assert_eq!(f0_array.value(1), "value2");

        // Check second struct field (f1 - integers)
        let f1_array = struct_array
            .column(1)
            .as_any()
            .downcast_ref::<arrow::array::Int32Array>()
            .unwrap();
        assert_eq!(f1_array.value(0), 100);
        assert_eq!(f1_array.value(1), 200);
    }

    #[test]
    fn test_encode_deep_nesting() {
        use arrow::array::ListArray;

        // Create deeply nested array structure (6 levels):
        // Array -> Array -> Array -> Array -> Array -> Int32
        let level_5 = Value::Array(vec![Value::Integer(42), Value::Integer(99)]);
        let level_4 = Value::Array(vec![level_5]);
        let level_3 = Value::Array(vec![level_4]);
        let level_2 = Value::Array(vec![level_3]);
        let level_1 = Value::Array(vec![level_2]);

        let mut log = LogEvent::default();
        log.insert("deep_array", level_1);

        let events = vec![Event::Log(log)];

        // Define schema for deep array nesting (6 levels total)
        let mut current_field = Field::new("item", DataType::Int32, true);
        for _ in 0..5 {
            current_field = Field::new("item", DataType::List(current_field.into()), true);
        }

        let schema = SchemaRef::new(Schema::new(vec![Field::new(
            "deep_array",
            current_field.data_type().clone(),
            true,
        )]));

        let batch =
            encode_and_decode(events, schema).expect("Failed to encode deeply nested arrays");

        assert_eq!(batch.num_rows(), 1);
        assert_eq!(batch.num_columns(), 1);

        // Verify deep array by navigating down through all levels
        // Store intermediate arrays to avoid lifetime issues
        let mut arrays: Vec<ArrayRef> = Vec::new();
        arrays.push(batch.column(0).clone());

        // Navigate through 5 nested List levels
        for level in 0..5 {
            let list_array = arrays[level]
                .as_any()
                .downcast_ref::<ListArray>()
                .unwrap_or_else(|| panic!("Expected ListArray at level {}", level));
            assert!(
                !list_array.is_null(0),
                "Array should not be null at level {}",
                level
            );
            assert_eq!(
                list_array.len(),
                1,
                "Array should have 1 element at level {}",
                level
            );
            arrays.push(list_array.value(0));
        }

        // Final level (level 5) should be Int32Array with values [42, 99]
        let int_array = arrays[5]
            .as_any()
            .downcast_ref::<arrow::array::Int32Array>()
            .unwrap();
        assert_eq!(int_array.len(), 2, "Final array should have 2 elements");
        assert_eq!(int_array.value(0), 42);
        assert_eq!(int_array.value(1), 99);
    }

    #[test]
    fn test_encode_struct_with_list_and_map() {
        use arrow::array::{ListArray, MapArray};
        use vrl::value::ObjectMap;

        // Create a struct containing both a list and a map
        // Struct { list_field: [1, 2, 3], map_field: {"k1": 10, "k2": 20} }
        let mut struct_value = ObjectMap::new();
        struct_value.insert(
            "f0".into(),
            Value::Array(vec![
                Value::Integer(1),
                Value::Integer(2),
                Value::Integer(3),
            ]),
        );

        let mut map_value = ObjectMap::new();
        map_value.insert("k1".into(), Value::Integer(10));
        map_value.insert("k2".into(), Value::Integer(20));
        struct_value.insert("f1".into(), Value::Object(map_value));

        let mut log = LogEvent::default();
        log.insert("complex_struct", Value::Object(struct_value));

        let events = vec![Event::Log(log)];

        // Define schema: Struct { list_field: List<Int32>, map_field: Map<Utf8, Int32> }
        let map_entries = Field::new(
            "entries",
            DataType::Struct(arrow::datatypes::Fields::from(vec![
                Field::new("keys", DataType::Utf8, false),
                Field::new("values", DataType::Int32, true),
            ])),
            false,
        );

        let struct_fields = arrow::datatypes::Fields::from(vec![
            Field::new(
                "f0",
                DataType::List(Field::new("item", DataType::Int32, true).into()),
                true,
            ),
            Field::new("f1", DataType::Map(map_entries.into(), false), true),
        ]);

        let schema = SchemaRef::new(Schema::new(vec![Field::new(
            "complex_struct",
            DataType::Struct(struct_fields),
            true,
        )]));

        let batch =
            encode_and_decode(events, schema).expect("Failed to encode struct with list and map");

        assert_eq!(batch.num_rows(), 1);
        assert_eq!(batch.num_columns(), 1);

        // Verify the struct
        let struct_array = batch
            .column(0)
            .as_any()
            .downcast_ref::<arrow::array::StructArray>()
            .unwrap();
        assert!(!struct_array.is_null(0));

        // Verify the list inside the struct (f0)
        let list_array = struct_array
            .column(0)
            .as_any()
            .downcast_ref::<ListArray>()
            .unwrap();
        assert!(!list_array.is_null(0));
        let list_value = list_array.value(0);
        assert_eq!(list_value.len(), 3);
        let int_array = list_value
            .as_any()
            .downcast_ref::<arrow::array::Int32Array>()
            .unwrap();
        assert_eq!(int_array.value(0), 1);
        assert_eq!(int_array.value(1), 2);
        assert_eq!(int_array.value(2), 3);

        // Verify the map inside the struct (f1)
        let map_array = struct_array
            .column(1)
            .as_any()
            .downcast_ref::<MapArray>()
            .unwrap();
        assert!(!map_array.is_null(0));
        let map_value = map_array.value(0);
        assert_eq!(map_value.len(), 2);
    }

    #[test]
    fn test_encode_map_with_struct_values() {
        use arrow::array::MapArray;
        use vrl::value::ObjectMap;

        // Create a map where values are structs
        // Map<String, Struct { name: String, count: Int32 }>
        // {"item1": {"f0": "Alice", "f1": 10}, "item2": {"f0": "Bob", "f1": 20}}
        let mut struct1 = ObjectMap::new();
        struct1.insert("f0".into(), Value::Bytes("Alice".into()));
        struct1.insert("f1".into(), Value::Integer(10));

        let mut struct2 = ObjectMap::new();
        struct2.insert("f0".into(), Value::Bytes("Bob".into()));
        struct2.insert("f1".into(), Value::Integer(20));

        let mut map_value = ObjectMap::new();
        map_value.insert("item1".into(), Value::Object(struct1));
        map_value.insert("item2".into(), Value::Object(struct2));

        let mut log = LogEvent::default();
        log.insert("map_with_structs", Value::Object(map_value));

        let events = vec![Event::Log(log)];

        // Define schema: Map<Utf8, Struct { f0: Utf8, f1: Int32 }>
        let struct_fields = arrow::datatypes::Fields::from(vec![
            Field::new("f0", DataType::Utf8, true),
            Field::new("f1", DataType::Int32, true),
        ]);

        let map_entries = Field::new(
            "entries",
            DataType::Struct(arrow::datatypes::Fields::from(vec![
                Field::new("keys", DataType::Utf8, false),
                Field::new("values", DataType::Struct(struct_fields), true),
            ])),
            false,
        );

        let schema = SchemaRef::new(Schema::new(vec![Field::new(
            "map_with_structs",
            DataType::Map(map_entries.into(), false),
            true,
        )]));

        let batch =
            encode_and_decode(events, schema).expect("Failed to encode map with struct values");

        assert_eq!(batch.num_rows(), 1);
        assert_eq!(batch.num_columns(), 1);

        // Verify the map
        let map_array = batch.column(0).as_any().downcast_ref::<MapArray>().unwrap();
        assert!(!map_array.is_null(0));
        let map_value = map_array.value(0);
        assert_eq!(map_value.len(), 2);

        // Verify the struct values in the map
        let struct_array = map_array
            .values()
            .as_any()
            .downcast_ref::<arrow::array::StructArray>()
            .unwrap();
        assert_eq!(struct_array.len(), 2);

        // Check f0 field (names)
        let names_array = struct_array
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let name1 = names_array.value(0);
        let name2 = names_array.value(1);
        assert!(name1 == "Alice" || name1 == "Bob");
        assert!(name2 == "Alice" || name2 == "Bob");
        assert_ne!(name1, name2);

        // Check f1 field (counts)
        let counts_array = struct_array
            .column(1)
            .as_any()
            .downcast_ref::<arrow::array::Int32Array>()
            .unwrap();
        assert!(counts_array.value(0) == 10 || counts_array.value(0) == 20);
        assert!(counts_array.value(1) == 10 || counts_array.value(1) == 20);
    }

    #[test]
    fn test_encode_list_of_structs_containing_maps() {
        use arrow::array::{ListArray, MapArray};
        use vrl::value::ObjectMap;

        // Create a list of structs, where each struct contains a map
        // List<Struct { id: Int32, attributes: Map<String, String> }>
        // [
        //   {"f0": 1, "f1": {"color": "red", "size": "large"}},
        //   {"f0": 2, "f1": {"color": "blue", "size": "small"}}
        // ]
        let mut attrs1 = ObjectMap::new();
        attrs1.insert("color".into(), Value::Bytes("red".into()));
        attrs1.insert("size".into(), Value::Bytes("large".into()));

        let mut struct1 = ObjectMap::new();
        struct1.insert("f0".into(), Value::Integer(1));
        struct1.insert("f1".into(), Value::Object(attrs1));

        let mut attrs2 = ObjectMap::new();
        attrs2.insert("color".into(), Value::Bytes("blue".into()));
        attrs2.insert("size".into(), Value::Bytes("small".into()));

        let mut struct2 = ObjectMap::new();
        struct2.insert("f0".into(), Value::Integer(2));
        struct2.insert("f1".into(), Value::Object(attrs2));

        let list_value = Value::Array(vec![Value::Object(struct1), Value::Object(struct2)]);

        let mut log = LogEvent::default();
        log.insert("list_of_structs_with_maps", list_value);

        let events = vec![Event::Log(log)];

        // Define schema
        let map_entries = Field::new(
            "entries",
            DataType::Struct(arrow::datatypes::Fields::from(vec![
                Field::new("keys", DataType::Utf8, false),
                Field::new("values", DataType::Utf8, true),
            ])),
            false,
        );

        let struct_fields = arrow::datatypes::Fields::from(vec![
            Field::new("f0", DataType::Int32, true),
            Field::new("f1", DataType::Map(map_entries.into(), false), true),
        ]);

        let list_field = Field::new("item", DataType::Struct(struct_fields), true);

        let schema = SchemaRef::new(Schema::new(vec![Field::new(
            "list_of_structs_with_maps",
            DataType::List(list_field.into()),
            true,
        )]));

        let batch =
            encode_and_decode(events, schema).expect("Failed to encode list of structs with maps");

        assert_eq!(batch.num_rows(), 1);
        assert_eq!(batch.num_columns(), 1);

        // Verify the list
        let list_array = batch
            .column(0)
            .as_any()
            .downcast_ref::<ListArray>()
            .unwrap();
        assert!(!list_array.is_null(0));
        let list_value = list_array.value(0);
        assert_eq!(list_value.len(), 2);

        // Verify the structs in the list
        let struct_array = list_value
            .as_any()
            .downcast_ref::<arrow::array::StructArray>()
            .unwrap();
        assert_eq!(struct_array.len(), 2);

        // Verify IDs (f0)
        let id_array = struct_array
            .column(0)
            .as_any()
            .downcast_ref::<arrow::array::Int32Array>()
            .unwrap();
        assert_eq!(id_array.value(0), 1);
        assert_eq!(id_array.value(1), 2);

        // Verify maps (f1)
        let map_array = struct_array
            .column(1)
            .as_any()
            .downcast_ref::<MapArray>()
            .unwrap();
        assert_eq!(map_array.len(), 2);
        assert!(!map_array.is_null(0));
        assert!(!map_array.is_null(1));

        // Verify first map has 2 entries
        let first_map = map_array.value(0);
        assert_eq!(first_map.len(), 2);

        // Verify second map has 2 entries
        let second_map = map_array.value(1);
        assert_eq!(second_map.len(), 2);
    }

    #[test]
    fn test_encode_deeply_nested_mixed_types() {
        use arrow::array::{ListArray, MapArray};
        use vrl::value::ObjectMap;

        // Create a very complex nested structure:
        // Struct {
        //   data: List<Map<String, Struct { values: List<Int32>, metadata: Map<String, String> }>>
        // }
        let mut metadata = ObjectMap::new();
        metadata.insert("key1".into(), Value::Bytes("value1".into()));

        let mut inner_struct = ObjectMap::new();
        inner_struct.insert("f0".into(), Value::Array(vec![Value::Integer(100)]));
        inner_struct.insert("f1".into(), Value::Object(metadata));

        let mut map_in_list = ObjectMap::new();
        map_in_list.insert("item_key".into(), Value::Object(inner_struct));

        let mut outer_struct = ObjectMap::new();
        outer_struct.insert("f0".into(), Value::Array(vec![Value::Object(map_in_list)]));

        let mut log = LogEvent::default();
        log.insert("deeply_nested", Value::Object(outer_struct));

        let events = vec![Event::Log(log)];

        // Define schema
        let metadata_map_entries = Field::new(
            "entries",
            DataType::Struct(arrow::datatypes::Fields::from(vec![
                Field::new("keys", DataType::Utf8, false),
                Field::new("values", DataType::Utf8, true),
            ])),
            false,
        );

        let inner_struct_fields = arrow::datatypes::Fields::from(vec![
            Field::new(
                "f0",
                DataType::List(Field::new("item", DataType::Int32, true).into()),
                true,
            ),
            Field::new(
                "f1",
                DataType::Map(metadata_map_entries.into(), false),
                true,
            ),
        ]);

        let map_entries = Field::new(
            "entries",
            DataType::Struct(arrow::datatypes::Fields::from(vec![
                Field::new("keys", DataType::Utf8, false),
                Field::new("values", DataType::Struct(inner_struct_fields), true),
            ])),
            false,
        );

        let list_field = Field::new("item", DataType::Map(map_entries.into(), false), true);

        let outer_struct_fields = arrow::datatypes::Fields::from(vec![Field::new(
            "f0",
            DataType::List(list_field.into()),
            true,
        )]);

        let schema = SchemaRef::new(Schema::new(vec![Field::new(
            "deeply_nested",
            DataType::Struct(outer_struct_fields),
            true,
        )]));

        let batch =
            encode_and_decode(events, schema).expect("Failed to encode deeply nested mixed types");

        assert_eq!(batch.num_rows(), 1);
        assert_eq!(batch.num_columns(), 1);

        // Verify the outer struct
        let outer_struct = batch
            .column(0)
            .as_any()
            .downcast_ref::<arrow::array::StructArray>()
            .unwrap();
        assert!(!outer_struct.is_null(0));

        // Verify the list inside the outer struct
        let list_array = outer_struct
            .column(0)
            .as_any()
            .downcast_ref::<ListArray>()
            .unwrap();
        assert!(!list_array.is_null(0));
        let list_value = list_array.value(0);
        assert_eq!(list_value.len(), 1);

        // Verify the map inside the list
        let map_array = list_value.as_any().downcast_ref::<MapArray>().unwrap();
        assert_eq!(map_array.len(), 1);
        assert!(!map_array.is_null(0));

        // Verify the struct inside the map
        let struct_values = map_array
            .values()
            .as_any()
            .downcast_ref::<arrow::array::StructArray>()
            .unwrap();
        assert_eq!(struct_values.len(), 1);

        // Verify the list inside the struct
        let inner_list = struct_values
            .column(0)
            .as_any()
            .downcast_ref::<ListArray>()
            .unwrap();
        assert!(!inner_list.is_null(0));
        let inner_list_value = inner_list.value(0);
        assert_eq!(inner_list_value.len(), 1);

        // Verify the innermost map
        let inner_map = struct_values
            .column(1)
            .as_any()
            .downcast_ref::<MapArray>()
            .unwrap();
        assert!(!inner_map.is_null(0));
        let inner_map_value = inner_map.value(0);
        assert_eq!(inner_map_value.len(), 1);
    }

    #[test]
    fn test_automatic_json_serialization_for_array_of_objects() {
        use vrl::value::ObjectMap;

        // Create array of objects (like the user's components data)
        let mut obj1 = ObjectMap::new();
        obj1.insert("name".into(), Value::Bytes("service.api.v1".into()));
        obj1.insert("alias".into(), Value::Bytes("widget-alpha".into()));
        obj1.insert("timeout".into(), Value::Integer(60000));

        let mut obj2 = ObjectMap::new();
        obj2.insert("name".into(), Value::Bytes("service.backend".into()));
        obj2.insert("alias".into(), Value::Bytes("widget-beta".into()));
        obj2.insert("timeout".into(), Value::Integer(30000));

        let components = Value::Array(vec![Value::Object(obj1), Value::Object(obj2)]);

        let mut log = LogEvent::default();
        log.insert("components", components);

        let events = vec![Event::Log(log)];

        // Schema expects Array(String), but we're providing Array(Object)
        // The encoder should automatically serialize objects to JSON strings
        let schema = Schema::new(vec![Field::new(
            "components",
            DataType::List(Field::new("item", DataType::Utf8, true).into()),
            false,
        )]);

        let batch = encode_and_decode(events, Arc::new(schema))
            .expect("Encoding should succeed with automatic JSON serialization");

        assert_eq!(batch.num_rows(), 1);

        let list_array = batch
            .column(0)
            .as_any()
            .downcast_ref::<ListArray>()
            .unwrap();
        assert!(!list_array.is_null(0));

        let list_value = list_array.value(0);
        let string_array = list_value.as_any().downcast_ref::<StringArray>().unwrap();

        // Should have 2 strings (JSON serialized objects)
        assert_eq!(string_array.len(), 2);

        // Verify the first object was serialized to JSON
        let json1 = string_array.value(0);
        assert!(json1.contains("\"name\":\"service.api.v1\""));
        assert!(json1.contains("\"alias\":\"widget-alpha\""));
        assert!(json1.contains("\"timeout\":60000"));

        // Verify the second object was serialized to JSON
        let json2 = string_array.value(1);
        assert!(json2.contains("\"name\":\"service.backend\""));
        assert!(json2.contains("\"alias\":\"widget-beta\""));
        assert!(json2.contains("\"timeout\":30000"));
    }

    #[test]
    fn test_object_in_map_values_to_string() {
        use vrl::value::ObjectMap;

        // Create a map with object values: Map<String, Object>
        // Schema expects Map<String, String>, so objects should serialize to JSON
        let mut inner_obj = ObjectMap::new();
        inner_obj.insert("config".into(), Value::Bytes("enabled".into()));
        inner_obj.insert("timeout".into(), Value::Integer(5000));

        let mut map_value = ObjectMap::new();
        map_value.insert("setting1".into(), Value::Object(inner_obj));
        map_value.insert("setting2".into(), Value::Bytes("simple string".into()));

        let mut log = LogEvent::default();
        log.insert("settings", Value::Object(map_value));

        let events = vec![Event::Log(log)];

        // Schema: Map<String, String> (expects string values, but we have objects)
        let key_field = Field::new("keys", DataType::Utf8, false);
        let value_field = Field::new("values", DataType::Utf8, true);
        let entries_struct = DataType::Struct(Fields::from(vec![key_field, value_field]));
        let entries_field = Field::new("entries", entries_struct, false);
        let map_type = DataType::Map(entries_field.into(), false);

        let schema = Schema::new(vec![Field::new("settings", map_type, false)]);

        let batch = encode_and_decode(events, Arc::new(schema))
            .expect("Map with object values should serialize to JSON strings");

        assert_eq!(batch.num_rows(), 1);

        let map_array = batch.column(0).as_any().downcast_ref::<MapArray>().unwrap();
        assert!(!map_array.is_null(0));

        // Get the values from the map
        let values_array = map_array
            .values()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();

        // One value should be a JSON object, one should be a plain string
        let mut found_json_object = false;
        let mut found_plain_string = false;

        for i in 0..values_array.len() {
            let value = values_array.value(i);
            if value.contains("\"config\"") && value.contains("\"timeout\"") {
                found_json_object = true;
            } else if value == "simple string" {
                found_plain_string = true;
            }
        }

        assert!(
            found_json_object,
            "Should find JSON-serialized object in map values"
        );
        assert!(found_plain_string, "Should find plain string in map values");
    }

    #[test]
    fn test_nested_arrays_with_objects() {
        use vrl::value::ObjectMap;

        // Array of arrays, where inner arrays contain objects
        let mut obj = ObjectMap::new();
        obj.insert("id".into(), Value::Integer(123));

        let inner_array = Value::Array(vec![Value::Object(obj.clone())]);
        let outer_array = Value::Array(vec![inner_array]);

        let mut log = LogEvent::default();
        log.insert("nested", outer_array);

        let events = vec![Event::Log(log)];

        // Schema: Array(Array(String))
        let inner_field = Field::new("item", DataType::Utf8, true);
        let middle_field = Field::new("item", DataType::List(inner_field.into()), true);
        let outer_list = DataType::List(middle_field.into());

        let schema = Schema::new(vec![Field::new("nested", outer_list, false)]);

        let batch = encode_and_decode(events, Arc::new(schema))
            .expect("Nested arrays with objects should serialize");

        assert_eq!(batch.num_rows(), 1);

        // Navigate to the deepest array
        let outer_list = batch
            .column(0)
            .as_any()
            .downcast_ref::<ListArray>()
            .unwrap();
        let outer_value = outer_list.value(0);
        let middle_list = outer_value.as_any().downcast_ref::<ListArray>().unwrap();
        let middle_value = middle_list.value(0);
        let inner_strings = middle_value.as_any().downcast_ref::<StringArray>().unwrap();

        // Should have one JSON string
        assert_eq!(inner_strings.len(), 1);
        let json_str = inner_strings.value(0);
        assert!(
            json_str.contains("\"id\":123"),
            "Deeply nested object should be serialized to JSON"
        );
    }
}
