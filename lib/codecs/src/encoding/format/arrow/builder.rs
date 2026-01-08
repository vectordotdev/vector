//! Arrow record batch builder
//!
//! Builds Arrow RecordBatches from Vector events by creating appropriate
//! array builders and appending values according to the schema.

use arrow::{
    array::{
        ArrayBuilder, ArrayRef, BinaryBuilder, BooleanBuilder, Decimal128Builder,
        Decimal256Builder, Float32Builder, Float64Builder, Int8Builder, Int16Builder, Int32Builder,
        Int64Builder, ListBuilder, MapBuilder, StringBuilder, StructBuilder,
        TimestampMicrosecondBuilder, TimestampMillisecondBuilder, TimestampNanosecondBuilder,
        TimestampSecondBuilder, UInt8Builder, UInt16Builder, UInt32Builder, UInt64Builder,
    },
    datatypes::{DataType, Field, SchemaRef, TimeUnit, i256},
    record_batch::RecordBatch,
};
use vector_core::event::{Event, Value};

use super::{ArrowEncodingError, types::create_array_builder_for_type};

/// Checks if a data type is supported by the Arrow encoder.
fn is_supported_type(data_type: &DataType) -> bool {
    matches!(
        data_type,
        DataType::Int8
            | DataType::Int16
            | DataType::Int32
            | DataType::Int64
            | DataType::UInt8
            | DataType::UInt16
            | DataType::UInt32
            | DataType::UInt64
            | DataType::Float32
            | DataType::Float64
            | DataType::Boolean
            | DataType::Utf8
            | DataType::Binary
            | DataType::Timestamp(_, _)
            | DataType::Decimal128(_, _)
            | DataType::Decimal256(_, _)
            | DataType::List(_)
            | DataType::Struct(_)
            | DataType::Map(_, _)
    )
}

/// Helper macro for downcasting builders
macro_rules! downcast_builder {
    // Infallible version - used for non-complex types
    ($builder:expr, $builder_type:ty) => {
        $builder
            .as_any_mut()
            .downcast_mut::<$builder_type>()
            .expect(concat!(
                "Failed to downcast builder to ",
                stringify!($builder_type)
            ))
    };

    // Fallible version - used for complex types (returns Result for error handling)
    ($builder:expr, $builder_type:ty, $field:expr) => {
        $builder
            .as_any_mut()
            .downcast_mut::<$builder_type>()
            .ok_or_else(|| ArrowEncodingError::UnsupportedType {
                field_name: $field.name().clone(),
                data_type: $field.data_type().clone(),
            })
    };
}

/// Macro to simplify appending null values by generating match arms
macro_rules! append_null_match {
    ($builder:expr, $data_type:expr, {$($pattern:pat => $builder_type:ty),* $(,)?}) => {
        match $data_type {
            $($pattern => downcast_builder!($builder, $builder_type).append_null(),)*
            _ => {}
        }
    };
}

/// Helper function to serialize a Value to JSON string.
/// This is used when the schema expects a string but the data contains complex types.
fn value_to_json_string(value: &Value) -> Result<String, ArrowEncodingError> {
    serde_json::to_string(value).map_err(|e| ArrowEncodingError::Io {
        source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
    })
}

/// Appends a null value to an array builder based on its type.
fn append_null_to_builder(
    builder: &mut dyn ArrayBuilder,
    data_type: &DataType,
) -> Result<(), ArrowEncodingError> {
    append_null_match!(builder, data_type, {
        DataType::Int8 => Int8Builder,
        DataType::Int16 => Int16Builder,
        DataType::Int32 => Int32Builder,
        DataType::Int64 => Int64Builder,
        DataType::UInt8 => UInt8Builder,
        DataType::UInt16 => UInt16Builder,
        DataType::UInt32 => UInt32Builder,
        DataType::UInt64 => UInt64Builder,
        DataType::Float32 => Float32Builder,
        DataType::Float64 => Float64Builder,
        DataType::Boolean => BooleanBuilder,
        DataType::Utf8 => StringBuilder,
        DataType::Binary => BinaryBuilder,
        DataType::Timestamp(TimeUnit::Second, _) => TimestampSecondBuilder,
        DataType::Timestamp(TimeUnit::Millisecond, _) => TimestampMillisecondBuilder,
        DataType::Timestamp(TimeUnit::Microsecond, _) => TimestampMicrosecondBuilder,
        DataType::Timestamp(TimeUnit::Nanosecond, _) => TimestampNanosecondBuilder,
        DataType::Decimal128(_, _) => Decimal128Builder,
        DataType::Decimal256(_, _) => Decimal256Builder,
        DataType::List(_) => ListBuilder<Box<dyn ArrayBuilder>>,
        DataType::Struct(_) => StructBuilder,
    });

    // Special case: Map uses append(false) instead of append_null()
    if matches!(data_type, DataType::Map(_, _)) {
        downcast_builder!(builder, MapBuilder<StringBuilder, Box<dyn ArrayBuilder>>)
            .append(false)
            .map_err(|e| ArrowEncodingError::RecordBatchCreation { source: e })?;
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
        (DataType::Int8, Value::Integer(i)) => {
            let val = (*i >= i8::MIN as i64 && *i <= i8::MAX as i64).then_some(*i as i8);
            downcast_builder!(builder, Int8Builder).append_option(val);
        }
        (DataType::Int16, Value::Integer(i)) => {
            let val = (*i >= i16::MIN as i64 && *i <= i16::MAX as i64).then_some(*i as i16);
            downcast_builder!(builder, Int16Builder).append_option(val);
        }
        (DataType::Int32, Value::Integer(i)) => {
            let val = (*i >= i32::MIN as i64 && *i <= i32::MAX as i64).then_some(*i as i32);
            downcast_builder!(builder, Int32Builder).append_option(val);
        }
        (DataType::Int64, Value::Integer(i)) => {
            downcast_builder!(builder, Int64Builder).append_value(*i);
        }

        // Unsigned integer types with range checking
        (DataType::UInt8, Value::Integer(i)) => {
            let val = (*i >= 0 && *i <= u8::MAX as i64).then_some(*i as u8);
            downcast_builder!(builder, UInt8Builder).append_option(val);
        }
        (DataType::UInt16, Value::Integer(i)) => {
            let val = (*i >= 0 && *i <= u16::MAX as i64).then_some(*i as u16);
            downcast_builder!(builder, UInt16Builder).append_option(val);
        }
        (DataType::UInt32, Value::Integer(i)) => {
            let val = (*i >= 0 && *i <= u32::MAX as i64).then_some(*i as u32);
            downcast_builder!(builder, UInt32Builder).append_option(val);
        }
        (DataType::UInt64, Value::Integer(i)) => {
            let val = (*i >= 0).then_some(*i as u64);
            downcast_builder!(builder, UInt64Builder).append_option(val);
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
        // Object -> String
        (DataType::Utf8, Value::Object(obj)) => {
            let json_str = value_to_json_string(&Value::Object(obj.clone()))?;
            downcast_builder!(builder, StringBuilder).append_value(&json_str);
        }
        // Array -> String
        (DataType::Utf8, Value::Array(arr)) => {
            let json_str = value_to_json_string(&Value::Array(arr.clone()))?;
            downcast_builder!(builder, StringBuilder).append_value(&json_str);
        }
        (DataType::Binary, Value::Bytes(bytes)) => {
            downcast_builder!(builder, BinaryBuilder).append_value(bytes);
        }

        // Timestamp types
        (DataType::Timestamp(time_unit, _), value) => {
            use chrono::Utc;

            let timestamp_value = match value {
                Value::Timestamp(ts) => Some(*ts),
                Value::Bytes(bytes) => std::str::from_utf8(bytes)
                    .ok()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&Utc)),
                _ => None,
            };

            let converted_value = match (time_unit, timestamp_value) {
                (TimeUnit::Second, Some(ts)) => Some(ts.timestamp()),
                (TimeUnit::Millisecond, Some(ts)) => Some(ts.timestamp_millis()),
                (TimeUnit::Microsecond, Some(ts)) => Some(ts.timestamp_micros()),
                (TimeUnit::Nanosecond, Some(ts)) => ts.timestamp_nanos_opt(),
                _ => {
                    // Fallback to raw integer if not a timestamp
                    if let Value::Integer(i) = value {
                        Some(*i)
                    } else {
                        None
                    }
                }
            };

            match time_unit {
                TimeUnit::Second => {
                    downcast_builder!(builder, TimestampSecondBuilder)
                        .append_option(converted_value);
                }
                TimeUnit::Millisecond => {
                    downcast_builder!(builder, TimestampMillisecondBuilder)
                        .append_option(converted_value);
                }
                TimeUnit::Microsecond => {
                    downcast_builder!(builder, TimestampMicrosecondBuilder)
                        .append_option(converted_value);
                }
                TimeUnit::Nanosecond => {
                    downcast_builder!(builder, TimestampNanosecondBuilder)
                        .append_option(converted_value);
                }
            }
        }

        // Decimal types
        (DataType::Decimal128(_precision, scale), value) => {
            use rust_decimal::Decimal;

            let target_scale = scale.unsigned_abs() as u32;

            let mantissa = match value {
                Value::Float(f) => Decimal::try_from(f.into_inner()).ok().map(|mut d| {
                    d.rescale(target_scale);
                    d.mantissa()
                }),
                Value::Integer(i) => {
                    let mut decimal = Decimal::from(*i);
                    decimal.rescale(target_scale);
                    Some(decimal.mantissa())
                }
                _ => None,
            };

            downcast_builder!(builder, Decimal128Builder).append_option(mantissa);
        }

        (DataType::Decimal256(_precision, scale), value) => {
            use rust_decimal::Decimal;

            let target_scale = scale.unsigned_abs() as u32;

            let mantissa = match value {
                Value::Float(f) => Decimal::try_from(f.into_inner()).ok().map(|mut d| {
                    d.rescale(target_scale);
                    i256::from_i128(d.mantissa())
                }),
                Value::Integer(i) => {
                    let mut decimal = Decimal::from(*i);
                    decimal.rescale(target_scale);
                    Some(i256::from_i128(decimal.mantissa()))
                }
                _ => None,
            };

            downcast_builder!(builder, Decimal256Builder).append_option(mantissa);
        }

        // Complex types
        (DataType::List(inner_field), Value::Array(arr)) => {
            let list_builder =
                downcast_builder!(builder, ListBuilder<Box<dyn ArrayBuilder>>, field)?;

            for item in arr.iter() {
                append_value_to_builder(list_builder.values(), item, inner_field)?;
            }
            list_builder.append(true);
        }

        (DataType::Struct(fields), Value::Object(obj)) => {
            let struct_builder = downcast_builder!(builder, StructBuilder, field)?;

            for (i, field) in fields.iter().enumerate() {
                // Use the actual field name from the schema
                // This supports both named tuples and unnamed tuples (which use "f0", "f1", etc.)
                let key = field.name();
                let field_builder = &mut struct_builder.field_builders_mut()[i];
                match obj.get(key.as_str()) {
                    Some(val) => append_value_to_builder(field_builder.as_mut(), val, field)?,
                    None => append_null_to_builder(field_builder.as_mut(), field.data_type())?,
                }
            }
            struct_builder.append(true);
        }

        (DataType::Map(entries_field, _), Value::Object(obj)) => {
            let map_builder = downcast_builder!(builder, MapBuilder<StringBuilder, Box<dyn ArrayBuilder>>, field)?;

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

        // Unsupported type/value combinations
        _ => {
            if !is_supported_type(field.data_type()) {
                return Err(ArrowEncodingError::UnsupportedType {
                    field_name: field.name().clone(),
                    data_type: field.data_type().clone(),
                });
            }

            // Supported type but value is missing/incompatible
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

fn build_array_for_field(events: &[Event], field: &Field) -> Result<ArrayRef, ArrowEncodingError> {
    let mut builder = create_array_builder_for_type(field.data_type(), events.len())?;

    events.iter().try_for_each(|event| {
        let Event::Log(log) = event else {
            return Ok(());
        };

        match log.get(field.name().as_str()) {
            Some(value) => append_value_to_builder(builder.as_mut(), value, field),
            None if field.is_nullable() => {
                append_null_to_builder(builder.as_mut(), field.data_type())
            }
            None => Err(ArrowEncodingError::NullConstraint {
                field_name: field.name().clone(),
            }),
        }
    })?;

    Ok(builder.finish())
}

/// Builds an Arrow RecordBatch from events
pub(crate) fn build_record_batch(
    schema: SchemaRef,
    events: &[Event],
) -> Result<RecordBatch, ArrowEncodingError> {
    let columns: Vec<ArrayRef> = schema
        .fields()
        .iter()
        .map(|field| build_array_for_field(events, field))
        .collect::<Result<_, _>>()?;

    RecordBatch::try_new(schema, columns)
        .map_err(|source| ArrowEncodingError::RecordBatchCreation { source })
}
