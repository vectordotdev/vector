use arrow::{
    array::{ArrayRef, Decimal128Builder, Decimal256Builder},
    datatypes::{DataType, i256},
};
use rust_decimal::Decimal;
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

pub(crate) fn build_decimal128_array(
    events: &[Event],
    field_name: &str,
    precision: u8,
    scale: i8,
    nullable: bool,
) -> Result<ArrayRef, ArrowEncodingError> {
    let mut builder = Decimal128Builder::with_capacity(events.len())
        .with_precision_and_scale(precision, scale)
        .map_err(|_| ArrowEncodingError::UnsupportedType {
            field_name: field_name.into(),
            data_type: DataType::Decimal128(precision, scale),
        })?;

    let target_scale = scale.unsigned_abs() as u32;

    for event in events {
        if let Event::Log(log) = event {
            let mut appended = false;
            match log.get(field_name) {
                Some(Value::Float(f)) => {
                    if let Ok(mut decimal) = Decimal::try_from(f.into_inner()) {
                        decimal.rescale(target_scale);
                        let mantissa = decimal.mantissa();
                        builder.append_value(mantissa);
                        appended = true;
                    }
                }
                Some(Value::Integer(i)) => {
                    let mut decimal = Decimal::from(*i);
                    decimal.rescale(target_scale);
                    let mantissa = decimal.mantissa();
                    builder.append_value(mantissa);
                    appended = true;
                }
                _ => {}
            }

            if !appended {
                handle_null_constraints!(builder, nullable, field_name);
            }
        }
    }

    Ok(Arc::new(builder.finish()))
}

pub(crate) fn build_decimal256_array(
    events: &[Event],
    field_name: &str,
    precision: u8,
    scale: i8,
    nullable: bool,
) -> Result<ArrayRef, ArrowEncodingError> {
    let mut builder = Decimal256Builder::with_capacity(events.len())
        .with_precision_and_scale(precision, scale)
        .map_err(|_| ArrowEncodingError::UnsupportedType {
            field_name: field_name.into(),
            data_type: DataType::Decimal256(precision, scale),
        })?;

    let target_scale = scale.unsigned_abs() as u32;

    for event in events {
        if let Event::Log(log) = event {
            let mut appended = false;
            match log.get(field_name) {
                Some(Value::Float(f)) => {
                    if let Ok(mut decimal) = Decimal::try_from(f.into_inner()) {
                        decimal.rescale(target_scale);
                        let mantissa = decimal.mantissa();
                        // rust_decimal does not support i256 natively so we upcast here
                        builder.append_value(i256::from_i128(mantissa));
                        appended = true;
                    }
                }
                Some(Value::Integer(i)) => {
                    let mut decimal = Decimal::from(*i);
                    decimal.rescale(target_scale);
                    let mantissa = decimal.mantissa();
                    builder.append_value(i256::from_i128(mantissa));
                    appended = true;
                }
                _ => {}
            }

            if !appended {
                handle_null_constraints!(builder, nullable, field_name);
            }
        }
    }

    Ok(Arc::new(builder.finish()))
}
