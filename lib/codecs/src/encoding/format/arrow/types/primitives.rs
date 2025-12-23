use arrow::array::{
    ArrayRef, BinaryBuilder, BooleanBuilder, Float32Builder, Float64Builder, Int8Builder,
    Int16Builder, Int32Builder, Int64Builder, StringBuilder, UInt8Builder, UInt16Builder,
    UInt32Builder, UInt64Builder,
};
use std::sync::Arc;
use vector_core::event::{Event, Value};

use crate::encoding::format::arrow::ArrowEncodingError;

/// Macro to handle appending null or returning an error for non-nullable fields.
macro_rules! handle_null_constraints {
    ($builder:expr, $nullable:expr, $field_name:expr) => {{
        if !$nullable {
            return Err(ArrowEncodingError::NullConstraint {
                field_name: $field_name.into(),
            });
        }
        $builder.append_null();
    }};
}

/// Macro to generate a `build_*_array` function for primitive types.
macro_rules! define_build_primitive_array_fn {
    (
        $fn_name:ident, // The function name (e.g., build_int8_array)
        $builder_ty:ty, // The builder type (e.g., Int8Builder)
        // One or more match arms for valid Value types
        $( $value_pat:pat $(if $guard:expr)? => $append_expr:expr ),+
    ) => {
        pub(crate) fn $fn_name(
            events: &[Event],
            field_name: &str,
            nullable: bool,
        ) -> Result<ArrayRef, ArrowEncodingError> {
            let mut builder = <$builder_ty>::with_capacity(events.len());

            for event in events {
                if let Event::Log(log) = event {
                    match log.get(field_name) {
                        $(
                            $value_pat $(if $guard)? => builder.append_value($append_expr),
                        )+
                        // All other patterns are treated as null/invalid
                        _ => handle_null_constraints!(builder, nullable, field_name),
                    }
                }
            }
            Ok(Arc::new(builder.finish()))
        }
    };
}

pub(crate) fn build_string_array(
    events: &[Event],
    field_name: &str,
    nullable: bool,
) -> Result<ArrayRef, ArrowEncodingError> {
    let mut builder = StringBuilder::with_capacity(events.len(), 0);

    for event in events {
        if let Event::Log(log) = event {
            let mut appended = false;
            if let Some(value) = log.get(field_name) {
                match value {
                    Value::Bytes(bytes) => {
                        // Attempt direct UTF-8 conversion first, fallback to lossy
                        match std::str::from_utf8(bytes) {
                            Ok(s) => builder.append_value(s),
                            Err(_) => builder.append_value(&String::from_utf8_lossy(bytes)),
                        }
                        appended = true;
                    }
                    Value::Object(obj) => {
                        if let Ok(s) = serde_json::to_string(&obj) {
                            builder.append_value(s);
                            appended = true;
                        }
                    }
                    Value::Array(arr) => {
                        if let Ok(s) = serde_json::to_string(&arr) {
                            builder.append_value(s);
                            appended = true;
                        }
                    }
                    _ => {
                        builder.append_value(&value.to_string_lossy());
                        appended = true;
                    }
                }
            }

            if !appended {
                handle_null_constraints!(builder, nullable, field_name);
            }
        }
    }

    Ok(Arc::new(builder.finish()))
}

define_build_primitive_array_fn!(
    build_int8_array,
    Int8Builder,
    Some(Value::Integer(i)) if *i >= i8::MIN as i64 && *i <= i8::MAX as i64 => *i as i8
);

define_build_primitive_array_fn!(
    build_int16_array,
    Int16Builder,
    Some(Value::Integer(i)) if *i >= i16::MIN as i64 && *i <= i16::MAX as i64 => *i as i16
);

define_build_primitive_array_fn!(
    build_int32_array,
    Int32Builder,
    Some(Value::Integer(i)) if *i >= i32::MIN as i64 && *i <= i32::MAX as i64 => *i as i32
);

define_build_primitive_array_fn!(
    build_int64_array,
    Int64Builder,
    Some(Value::Integer(i)) => *i
);

define_build_primitive_array_fn!(
    build_uint8_array,
    UInt8Builder,
    Some(Value::Integer(i)) if *i >= 0 && *i <= u8::MAX as i64 => *i as u8
);

define_build_primitive_array_fn!(
    build_uint16_array,
    UInt16Builder,
    Some(Value::Integer(i)) if *i >= 0 && *i <= u16::MAX as i64 => *i as u16
);

define_build_primitive_array_fn!(
    build_uint32_array,
    UInt32Builder,
    Some(Value::Integer(i)) if *i >= 0 && *i <= u32::MAX as i64 => *i as u32
);

define_build_primitive_array_fn!(
    build_uint64_array,
    UInt64Builder,
    Some(Value::Integer(i)) if *i >= 0 => *i as u64
);

define_build_primitive_array_fn!(
    build_float32_array,
    Float32Builder,
    Some(Value::Float(f)) => f.into_inner() as f32,
    Some(Value::Integer(i)) => *i as f32
);

define_build_primitive_array_fn!(
    build_float64_array,
    Float64Builder,
    Some(Value::Float(f)) => f.into_inner(),
    Some(Value::Integer(i)) => *i as f64
);

define_build_primitive_array_fn!(
    build_boolean_array,
    BooleanBuilder,
    Some(Value::Boolean(b)) => *b
);

pub(crate) fn build_binary_array(
    events: &[Event],
    field_name: &str,
    nullable: bool,
) -> Result<ArrayRef, ArrowEncodingError> {
    let mut builder = BinaryBuilder::with_capacity(events.len(), 0);

    for event in events {
        if let Event::Log(log) = event {
            match log.get(field_name) {
                Some(Value::Bytes(bytes)) => builder.append_value(bytes),
                _ => handle_null_constraints!(builder, nullable, field_name),
            }
        }
    }

    Ok(Arc::new(builder.finish()))
}
