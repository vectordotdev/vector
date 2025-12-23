//! Complex type array builders for Arrow encoding
//!
//! This module handles nested Arrow types: List, Struct (tuples), and Map.

use arrow::array::{
    ArrayBuilder, ArrayRef, BinaryBuilder, BooleanBuilder, Float32Builder, Float64Builder,
    Int8Builder, Int16Builder, Int32Builder, Int64Builder, ListBuilder, MapBuilder, StringBuilder,
    StructBuilder, TimestampMicrosecondBuilder, TimestampMillisecondBuilder,
    TimestampNanosecondBuilder, TimestampSecondBuilder, UInt8Builder, UInt16Builder, UInt32Builder,
    UInt64Builder,
};
use arrow::datatypes::{DataType, Field, Fields, TimeUnit};
use std::sync::Arc;

use super::super::ArrowEncodingError;
use super::create_array_builder_for_type;
use vector_core::event::{Event, Value};

/// Helper macro for downcasting builders
macro_rules! downcast_builder {
    ($builder:expr, $builder_type:ty) => {
        $builder
            .as_any_mut()
            .downcast_mut::<$builder_type>()
            .expect(concat!(
                "Failed to downcast builder to ",
                stringify!($builder_type)
            ))
    };
}

/// Helper function to serialize a Value to JSON string.
/// This is used when the schema expects a string but the data contains complex types.
fn value_to_json_string(value: &Value) -> Result<String, ArrowEncodingError> {
    serde_json::to_string(value).map_err(|e| ArrowEncodingError::Io {
        source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
    })
}

/// Appends a null value to an array builder based on the data type.
fn append_null_to_builder(
    builder: &mut dyn ArrayBuilder,
    data_type: &DataType,
) -> Result<(), ArrowEncodingError> {
    match data_type {
        DataType::Int8 => downcast_builder!(builder, Int8Builder).append_null(),
        DataType::Int16 => downcast_builder!(builder, Int16Builder).append_null(),
        DataType::Int32 => downcast_builder!(builder, Int32Builder).append_null(),
        DataType::Int64 => downcast_builder!(builder, Int64Builder).append_null(),
        DataType::UInt8 => downcast_builder!(builder, UInt8Builder).append_null(),
        DataType::UInt16 => downcast_builder!(builder, UInt16Builder).append_null(),
        DataType::UInt32 => downcast_builder!(builder, UInt32Builder).append_null(),
        DataType::UInt64 => downcast_builder!(builder, UInt64Builder).append_null(),
        DataType::Float32 => downcast_builder!(builder, Float32Builder).append_null(),
        DataType::Float64 => downcast_builder!(builder, Float64Builder).append_null(),
        DataType::Boolean => downcast_builder!(builder, BooleanBuilder).append_null(),
        DataType::Utf8 => downcast_builder!(builder, StringBuilder).append_null(),
        DataType::Binary => downcast_builder!(builder, BinaryBuilder).append_null(),
        DataType::Timestamp(TimeUnit::Second, _) => {
            downcast_builder!(builder, TimestampSecondBuilder).append_null()
        }
        DataType::Timestamp(TimeUnit::Millisecond, _) => {
            downcast_builder!(builder, TimestampMillisecondBuilder).append_null()
        }
        DataType::Timestamp(TimeUnit::Microsecond, _) => {
            downcast_builder!(builder, TimestampMicrosecondBuilder).append_null()
        }
        DataType::Timestamp(TimeUnit::Nanosecond, _) => {
            downcast_builder!(builder, TimestampNanosecondBuilder).append_null()
        }
        DataType::List(_) => {
            builder
                .as_any_mut()
                .downcast_mut::<ListBuilder<Box<dyn ArrayBuilder>>>()
                .expect("Failed to downcast to ListBuilder")
                .append_null();
        }
        DataType::Struct(_) => downcast_builder!(builder, StructBuilder).append_null(),
        DataType::Map(_, _) => {
            builder
                .as_any_mut()
                .downcast_mut::<MapBuilder<StringBuilder, Box<dyn ArrayBuilder>>>()
                .expect("Failed to downcast to MapBuilder")
                .append(false)
                .map_err(|e| ArrowEncodingError::RecordBatchCreation { source: e })?;
        }
        _ => {}
    }
    Ok(())
}

/// Recursively appends a VRL Value to an Arrow array builder.
fn append_value_to_builder(
    builder: &mut dyn ArrayBuilder,
    value: &Value,
    field: &Field,
) -> Result<(), ArrowEncodingError> {
    match (field.data_type(), value) {
        // Integer types with range checking
        (DataType::Int8, Value::Integer(i)) if *i >= i8::MIN as i64 && *i <= i8::MAX as i64 => {
            downcast_builder!(builder, Int8Builder).append_value(*i as i8);
        }
        (DataType::Int16, Value::Integer(i)) if *i >= i16::MIN as i64 && *i <= i16::MAX as i64 => {
            downcast_builder!(builder, Int16Builder).append_value(*i as i16);
        }
        (DataType::Int32, Value::Integer(i)) if *i >= i32::MIN as i64 && *i <= i32::MAX as i64 => {
            downcast_builder!(builder, Int32Builder).append_value(*i as i32);
        }
        (DataType::Int64, Value::Integer(i)) => {
            downcast_builder!(builder, Int64Builder).append_value(*i);
        }
        (DataType::UInt8, Value::Integer(i)) if *i >= 0 && *i <= u8::MAX as i64 => {
            downcast_builder!(builder, UInt8Builder).append_value(*i as u8);
        }
        (DataType::UInt16, Value::Integer(i)) if *i >= 0 && *i <= u16::MAX as i64 => {
            downcast_builder!(builder, UInt16Builder).append_value(*i as u16);
        }
        (DataType::UInt32, Value::Integer(i)) if *i >= 0 && *i <= u32::MAX as i64 => {
            downcast_builder!(builder, UInt32Builder).append_value(*i as u32);
        }
        (DataType::UInt64, Value::Integer(i)) if *i >= 0 => {
            downcast_builder!(builder, UInt64Builder).append_value(*i as u64);
        }
        // Float types
        (DataType::Float32, Value::Float(f)) => {
            downcast_builder!(builder, Float32Builder).append_value(f.into_inner() as f32);
        }
        (DataType::Float32, Value::Integer(i)) => {
            downcast_builder!(builder, Float32Builder).append_value(*i as f32);
        }
        (DataType::Float64, Value::Float(f)) => {
            downcast_builder!(builder, Float64Builder).append_value(f.into_inner());
        }
        (DataType::Float64, Value::Integer(i)) => {
            downcast_builder!(builder, Float64Builder).append_value(*i as f64);
        }
        // Boolean
        (DataType::Boolean, Value::Boolean(b)) => {
            downcast_builder!(builder, BooleanBuilder).append_value(*b);
        }
        // String types
        (DataType::Utf8, Value::Bytes(bytes)) => match std::str::from_utf8(bytes) {
            Ok(s) => downcast_builder!(builder, StringBuilder).append_value(s),
            Err(_) => {
                let s = String::from_utf8_lossy(bytes);
                downcast_builder!(builder, StringBuilder).append_value(&s)
            }
        },
        // Automatic JSON serialization: Object -> String
        (DataType::Utf8, Value::Object(obj)) => {
            let json_str = value_to_json_string(&Value::Object(obj.clone()))?;
            downcast_builder!(builder, StringBuilder).append_value(&json_str);
        }
        // Automatic JSON serialization: Array -> String
        (DataType::Utf8, Value::Array(arr)) => {
            let json_str = value_to_json_string(&Value::Array(arr.clone()))?;
            downcast_builder!(builder, StringBuilder).append_value(&json_str);
        }
        (DataType::Binary, Value::Bytes(bytes)) => {
            downcast_builder!(builder, BinaryBuilder).append_value(bytes);
        }

        // Recursive types: List (Array)
        (DataType::List(inner_field), Value::Array(arr)) => {
            let list_builder = builder
                .as_any_mut()
                .downcast_mut::<ListBuilder<Box<dyn ArrayBuilder>>>()
                .ok_or_else(|| ArrowEncodingError::UnsupportedType {
                    field_name: field.name().clone(),
                    data_type: field.data_type().clone(),
                })?;

            for item in arr.iter() {
                append_value_to_builder(list_builder.values(), item, inner_field)?;
            }
            list_builder.append(true);
        }

        // Recursive types: Struct (Tuple)
        (DataType::Struct(fields), Value::Object(obj)) => {
            let struct_builder = builder
                .as_any_mut()
                .downcast_mut::<StructBuilder>()
                .ok_or_else(|| ArrowEncodingError::UnsupportedType {
                    field_name: field.name().clone(),
                    data_type: field.data_type().clone(),
                })?;

            for (i, field) in fields.iter().enumerate() {
                let key = format!("f{}", i);
                let field_builder = &mut struct_builder.field_builders_mut()[i];
                match obj.get(key.as_str()) {
                    Some(val) => append_value_to_builder(field_builder.as_mut(), val, field)?,
                    None => append_null_to_builder(field_builder.as_mut(), field.data_type())?,
                }
            }
            struct_builder.append(true);
        }

        // Recursive types: Map (nested maps)
        (DataType::Map(entries_field, _), Value::Object(obj)) => {
            let map_builder = builder
                .as_any_mut()
                .downcast_mut::<MapBuilder<StringBuilder, Box<dyn ArrayBuilder>>>()
                .ok_or_else(|| ArrowEncodingError::UnsupportedType {
                    field_name: field.name().clone(),
                    data_type: field.data_type().clone(),
                })?;

            let DataType::Struct(entries_struct) = entries_field.data_type() else {
                return Err(ArrowEncodingError::UnsupportedType {
                    field_name: field.name().clone(),
                    data_type: field.data_type().clone(),
                });
            };

            let value_field = &entries_struct[1];
            for (key, value) in obj.iter() {
                map_builder.keys().append_value(key.as_ref());
                append_value_to_builder(map_builder.values(), value, value_field)?;
            }
            map_builder
                .append(true)
                .map_err(|e| ArrowEncodingError::RecordBatchCreation { source: e })?;
        }

        // Null/missing values
        _ => {
            if field.is_nullable() {
                append_null_to_builder(builder, field.data_type())?;
            } else {
                return Err(ArrowEncodingError::NullConstraint {
                    field_name: field.name().clone(),
                });
            }
        }
    }
    Ok(())
}

/// Builds a List array from events for a given field.
/// Handles all nested types (including List<Struct>) through recursive builder utilities.
pub(crate) fn build_list_array(
    events: &[Event],
    field_name: &str,
    inner_field: &Field,
    nullable: bool,
) -> Result<ArrayRef, ArrowEncodingError> {
    let inner_builder = create_array_builder_for_type(
        inner_field.data_type(),
        events.len() * 4, // Estimate capacity
    )?;

    let mut list_builder = ListBuilder::new(inner_builder);

    for event in events {
        if let Event::Log(log) = event {
            match log.get(field_name) {
                Some(Value::Array(arr)) => {
                    // Recursively append values (handles primitives, structs, maps, nested lists, etc.)
                    for value in arr.iter() {
                        append_value_to_builder(list_builder.values(), value, inner_field)?;
                    }
                    list_builder.append(true);
                }
                _ => {
                    if !nullable {
                        return Err(ArrowEncodingError::NullConstraint {
                            field_name: field_name.into(),
                        });
                    }
                    list_builder.append_null();
                }
            }
        }
    }

    Ok(Arc::new(list_builder.finish()))
}

/// Builds a Struct array from events for a given field (used for Tuples).
pub(crate) fn build_struct_array(
    events: &[Event],
    field_name: &str,
    fields: &Fields,
    nullable: bool,
) -> Result<ArrayRef, ArrowEncodingError> {
    // Create builders for each field
    let field_builders: Vec<Box<dyn ArrayBuilder>> = fields
        .iter()
        .map(|f| create_array_builder_for_type(f.data_type(), events.len()))
        .collect::<Result<Vec<_>, _>>()?;

    let mut struct_builder = StructBuilder::new(fields.clone(), field_builders);

    for event in events {
        if let Event::Log(log) = event {
            match log.get(field_name) {
                Some(Value::Object(obj)) => {
                    // Tuples are represented as objects with f0, f1, f2... keys
                    let field_builders = struct_builder.field_builders_mut();
                    for (i, (field, builder)) in
                        fields.iter().zip(field_builders.iter_mut()).enumerate()
                    {
                        let key = format!("f{}", i);
                        if let Some(value) = obj.get(key.as_str()) {
                            append_value_to_builder(builder.as_mut(), value, field)?;
                        } else {
                            // If the struct field is non-nullable and the value is missing, error
                            if !field.is_nullable() {
                                return Err(ArrowEncodingError::NullConstraint {
                                    field_name: format!("{}.{}", field_name, field.name()),
                                });
                            }
                            append_null_to_builder(builder.as_mut(), field.data_type())?;
                        }
                    }
                    struct_builder.append(true);
                }
                _ => {
                    if !nullable {
                        return Err(ArrowEncodingError::NullConstraint {
                            field_name: field_name.into(),
                        });
                    }
                    // Append nulls to all field builders
                    let field_builders = struct_builder.field_builders_mut();
                    for (field, builder) in fields.iter().zip(field_builders.iter_mut()) {
                        append_null_to_builder(builder.as_mut(), field.data_type())?;
                    }
                    struct_builder.append(false);
                }
            }
        }
    }

    Ok(Arc::new(struct_builder.finish()))
}

/// Builds a Map array from events for a given field.
pub(crate) fn build_map_array(
    events: &[Event],
    field_name: &str,
    entries_field: &Field,
    nullable: bool,
) -> Result<ArrayRef, ArrowEncodingError> {
    // Extract key and value fields from entries struct
    let entries_struct = match entries_field.data_type() {
        DataType::Struct(fields) => fields,
        _ => {
            return Err(ArrowEncodingError::UnsupportedType {
                field_name: field_name.into(),
                data_type: entries_field.data_type().clone(),
            });
        }
    };

    if entries_struct.len() != 2 {
        return Err(ArrowEncodingError::UnsupportedType {
            field_name: field_name.into(),
            data_type: entries_field.data_type().clone(),
        });
    }

    let value_field = &entries_struct[1];

    // Create builders for keys and values
    let key_builder = StringBuilder::with_capacity(events.len() * 4, 0);
    let value_builder = create_array_builder_for_type(value_field.data_type(), events.len() * 4)?;

    let mut map_builder = MapBuilder::new(None, key_builder, value_builder);

    for event in events {
        if let Event::Log(log) = event {
            match log.get(field_name) {
                Some(Value::Object(obj)) => {
                    // Append each key-value pair
                    for (key, value) in obj.iter() {
                        map_builder.keys().append_value(key.as_ref());
                        append_value_to_builder(map_builder.values(), value, value_field)?;
                    }
                    map_builder
                        .append(true)
                        .map_err(|e| ArrowEncodingError::RecordBatchCreation { source: e })?;
                }
                _ => {
                    if !nullable {
                        return Err(ArrowEncodingError::NullConstraint {
                            field_name: field_name.into(),
                        });
                    }
                    // For null maps, we need to call append(false)
                    map_builder
                        .append(false)
                        .map_err(|e| ArrowEncodingError::RecordBatchCreation { source: e })?;
                }
            }
        }
    }

    Ok(Arc::new(map_builder.finish()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{
        Array, Int32Array, Int64Array, ListArray, MapArray, StringArray, StructArray,
    };
    use arrow::datatypes::{DataType, Field, Fields};
    use std::sync::Arc;
    use vector_core::event::{Event, LogEvent, Value};
    use vrl::value::ObjectMap;

    #[test]
    fn test_build_list_array_with_primitives() {
        let mut log1 = LogEvent::default();
        log1.insert(
            "numbers",
            Value::Array(vec![
                Value::Integer(1),
                Value::Integer(2),
                Value::Integer(3),
            ]),
        );

        let mut log2 = LogEvent::default();
        log2.insert(
            "numbers",
            Value::Array(vec![Value::Integer(4), Value::Integer(5)]),
        );

        let events = vec![Event::Log(log1), Event::Log(log2)];

        let inner_field = Field::new("item", DataType::Int64, true);
        let result = build_list_array(&events, "numbers", &inner_field, true);

        assert!(result.is_ok());
        let array = result.unwrap();
        let list_array = array.as_any().downcast_ref::<ListArray>().unwrap();

        assert_eq!(list_array.len(), 2);
        assert!(!list_array.is_null(0));
        assert!(!list_array.is_null(1));

        // Check first list [1, 2, 3]
        let first_list = list_array.value(0);
        let int_array = first_list.as_any().downcast_ref::<Int64Array>().unwrap();
        assert_eq!(int_array.len(), 3);
        assert_eq!(int_array.value(0), 1);
        assert_eq!(int_array.value(1), 2);
        assert_eq!(int_array.value(2), 3);

        // Check second list [4, 5]
        let second_list = list_array.value(1);
        let int_array = second_list.as_any().downcast_ref::<Int64Array>().unwrap();
        assert_eq!(int_array.len(), 2);
        assert_eq!(int_array.value(0), 4);
        assert_eq!(int_array.value(1), 5);
    }

    #[test]
    fn test_build_list_array_with_nulls() {
        let mut log1 = LogEvent::default();
        log1.insert("numbers", Value::Array(vec![Value::Integer(1)]));

        let log2 = LogEvent::default(); // Missing field

        let mut log3 = LogEvent::default();
        log3.insert("numbers", Value::Array(vec![Value::Integer(3)]));

        let events = vec![Event::Log(log1), Event::Log(log2), Event::Log(log3)];

        let inner_field = Field::new("item", DataType::Int64, true);
        let result = build_list_array(&events, "numbers", &inner_field, true);

        assert!(result.is_ok());
        let array = result.unwrap();
        let list_array = array.as_any().downcast_ref::<ListArray>().unwrap();

        assert_eq!(list_array.len(), 3);
        assert!(!list_array.is_null(0));
        assert!(list_array.is_null(1)); // Missing field
        assert!(!list_array.is_null(2));
    }

    #[test]
    fn test_build_struct_array_with_missing_fields() {
        let mut tuple = ObjectMap::new();
        tuple.insert("f0".into(), Value::Bytes("partial".into()));
        // f1 is missing

        let mut log = LogEvent::default();
        log.insert("tuple", Value::Object(tuple));

        let events = vec![Event::Log(log)];

        let fields = Fields::from(vec![
            Field::new("f0", DataType::Utf8, true),
            Field::new("f1", DataType::Int64, true), // Nullable
        ]);

        let result = build_struct_array(&events, "tuple", &fields, true);

        assert!(result.is_ok());
        let array = result.unwrap();
        let struct_array = array.as_any().downcast_ref::<StructArray>().unwrap();

        assert_eq!(struct_array.len(), 1);

        // f0 should have value
        let f0_array = struct_array
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(f0_array.value(0), "partial");

        // f1 should be null
        let f1_array = struct_array
            .column(1)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap();
        assert!(f1_array.is_null(0));
    }

    #[test]
    fn test_build_map_array_with_null() {
        let mut map1 = ObjectMap::new();
        map1.insert("key1".into(), Value::Integer(100));

        let mut log1 = LogEvent::default();
        log1.insert("map", Value::Object(map1));

        let log2 = LogEvent::default(); // Missing map field

        let events = vec![Event::Log(log1), Event::Log(log2)];

        let entries_field = Field::new(
            "entries",
            DataType::Struct(Fields::from(vec![
                Field::new("keys", DataType::Utf8, false),
                Field::new("values", DataType::Int64, true),
            ])),
            false,
        );

        let result = build_map_array(&events, "map", &entries_field, true);

        assert!(result.is_ok());
        let array = result.unwrap();
        let map_array = array.as_any().downcast_ref::<MapArray>().unwrap();

        assert_eq!(map_array.len(), 2);
        assert!(!map_array.is_null(0));
        assert!(map_array.is_null(1));
    }

    #[test]
    fn test_build_map_array_empty_map() {
        let mut log = LogEvent::default();
        log.insert("map", Value::Object(ObjectMap::new())); // Empty map

        let events = vec![Event::Log(log)];

        let entries_field = Field::new(
            "entries",
            DataType::Struct(Fields::from(vec![
                Field::new("keys", DataType::Utf8, false),
                Field::new("values", DataType::Int64, true),
            ])),
            false,
        );

        let result = build_map_array(&events, "map", &entries_field, true);

        assert!(result.is_ok());
        let array = result.unwrap();
        let map_array = array.as_any().downcast_ref::<MapArray>().unwrap();

        assert_eq!(map_array.len(), 1);
        assert!(!map_array.is_null(0));

        let map_value = map_array.value(0);
        assert_eq!(map_value.len(), 0); // Empty but not null
    }

    #[test]
    fn test_json_serialization_object_to_string() {
        let mut obj = ObjectMap::new();
        obj.insert("name".into(), Value::Bytes("test".into()));
        obj.insert("count".into(), Value::Integer(42));

        let mut log = LogEvent::default();
        log.insert("data", Value::Array(vec![Value::Object(obj)]));

        let events = vec![Event::Log(log)];

        // Schema expects List<String>
        let inner_field = Field::new("item", DataType::Utf8, true);
        let result = build_list_array(&events, "data", &inner_field, true);

        assert!(result.is_ok());
        let array = result.unwrap();
        let list_array = array.as_any().downcast_ref::<ListArray>().unwrap();

        let values = list_array.value(0);
        let string_array = values.as_any().downcast_ref::<StringArray>().unwrap();
        let json_str = string_array.value(0);

        // Should be JSON serialized
        assert!(json_str.contains("\"name\""));
        assert!(json_str.contains("test"));
        assert!(json_str.contains("\"count\""));
        assert!(json_str.contains("42"));
    }

    #[test]
    fn test_json_serialization_array_to_string() {
        let mut log = LogEvent::default();
        log.insert(
            "data",
            Value::Array(vec![Value::Array(vec![
                Value::Integer(1),
                Value::Integer(2),
                Value::Integer(3),
            ])]),
        );

        let events = vec![Event::Log(log)];

        // Schema expects List<String>
        let inner_field = Field::new("item", DataType::Utf8, true);
        let result = build_list_array(&events, "data", &inner_field, true);

        assert!(result.is_ok());
        let array = result.unwrap();
        let list_array = array.as_any().downcast_ref::<ListArray>().unwrap();

        let values = list_array.value(0);
        let string_array = values.as_any().downcast_ref::<StringArray>().unwrap();
        let json_str = string_array.value(0);

        // Should be JSON serialized array
        assert_eq!(json_str, "[1,2,3]");
    }

    #[test]
    fn test_nested_list_of_structs() {
        let mut tuple1 = ObjectMap::new();
        tuple1.insert("f0".into(), Value::Integer(1));
        tuple1.insert("f1".into(), Value::Bytes("a".into()));

        let mut tuple2 = ObjectMap::new();
        tuple2.insert("f0".into(), Value::Integer(2));
        tuple2.insert("f1".into(), Value::Bytes("b".into()));

        let mut log = LogEvent::default();
        log.insert(
            "data",
            Value::Array(vec![Value::Object(tuple1), Value::Object(tuple2)]),
        );

        let events = vec![Event::Log(log)];

        let struct_fields = Fields::from(vec![
            Field::new("f0", DataType::Int32, true),
            Field::new("f1", DataType::Utf8, true),
        ]);

        let inner_field = Field::new("item", DataType::Struct(struct_fields), true);
        let result = build_list_array(&events, "data", &inner_field, true);

        assert!(result.is_ok());
        let array = result.unwrap();
        let list_array = array.as_any().downcast_ref::<ListArray>().unwrap();

        let values = list_array.value(0);
        let struct_array = values.as_any().downcast_ref::<StructArray>().unwrap();

        assert_eq!(struct_array.len(), 2);

        let f0_array = struct_array
            .column(0)
            .as_any()
            .downcast_ref::<Int32Array>()
            .unwrap();
        assert_eq!(f0_array.value(0), 1);
        assert_eq!(f0_array.value(1), 2);

        let f1_array = struct_array
            .column(1)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(f1_array.value(0), "a");
        assert_eq!(f1_array.value(1), "b");
    }

    #[test]
    fn test_nested_struct_with_list() {
        let mut tuple = ObjectMap::new();
        tuple.insert("f0".into(), Value::Bytes("name".into()));
        tuple.insert(
            "f1".into(),
            Value::Array(vec![
                Value::Integer(1),
                Value::Integer(2),
                Value::Integer(3),
            ]),
        );

        let mut log = LogEvent::default();
        log.insert("data", Value::Object(tuple));

        let events = vec![Event::Log(log)];

        let fields = Fields::from(vec![
            Field::new("f0", DataType::Utf8, true),
            Field::new(
                "f1",
                DataType::List(Arc::new(Field::new("item", DataType::Int64, true))),
                true,
            ),
        ]);

        let result = build_struct_array(&events, "data", &fields, true);

        assert!(result.is_ok());
        let array = result.unwrap();
        let struct_array = array.as_any().downcast_ref::<StructArray>().unwrap();

        // Check f0 (string)
        let f0_array = struct_array
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(f0_array.value(0), "name");

        // Check f1 (list)
        let f1_array = struct_array
            .column(1)
            .as_any()
            .downcast_ref::<ListArray>()
            .unwrap();
        let list_values = f1_array.value(0);
        let int_array = list_values.as_any().downcast_ref::<Int64Array>().unwrap();
        assert_eq!(int_array.len(), 3);
        assert_eq!(int_array.value(0), 1);
        assert_eq!(int_array.value(1), 2);
        assert_eq!(int_array.value(2), 3);
    }

    #[test]
    fn test_nested_map_with_struct_values() {
        let mut struct_value = ObjectMap::new();
        struct_value.insert("f0".into(), Value::Integer(42));
        struct_value.insert("f1".into(), Value::Bytes("test".into()));

        let mut map = ObjectMap::new();
        map.insert("key1".into(), Value::Object(struct_value));

        let mut log = LogEvent::default();
        log.insert("data", Value::Object(map));

        let events = vec![Event::Log(log)];

        let struct_fields = Fields::from(vec![
            Field::new("f0", DataType::Int64, true),
            Field::new("f1", DataType::Utf8, true),
        ]);

        let entries_field = Field::new(
            "entries",
            DataType::Struct(Fields::from(vec![
                Field::new("keys", DataType::Utf8, false),
                Field::new("values", DataType::Struct(struct_fields), true),
            ])),
            false,
        );

        let result = build_map_array(&events, "data", &entries_field, true);

        assert!(result.is_ok());
        let array = result.unwrap();
        let map_array = array.as_any().downcast_ref::<MapArray>().unwrap();

        assert_eq!(map_array.len(), 1);
        assert!(!map_array.is_null(0));

        let map_value = map_array.value(0);
        assert_eq!(map_value.len(), 1);

        // Verify struct values
        let struct_array = map_array
            .values()
            .as_any()
            .downcast_ref::<StructArray>()
            .unwrap();
        let f0_array = struct_array
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap();
        assert_eq!(f0_array.value(0), 42);

        let f1_array = struct_array
            .column(1)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(f1_array.value(0), "test");
    }
}
