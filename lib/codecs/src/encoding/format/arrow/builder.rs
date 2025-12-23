use arrow::{
    array::ArrayRef,
    datatypes::{DataType, Schema},
    record_batch::RecordBatch,
};
use std::sync::Arc;
use vector_core::event::Event;

use crate::encoding::format::arrow::{
    ArrowEncodingError,
    types::{
        build_binary_array, build_boolean_array, build_decimal128_array, build_decimal256_array,
        build_float32_array, build_float64_array, build_int8_array, build_int16_array,
        build_int32_array, build_int64_array, build_string_array, build_timestamp_array,
        build_uint8_array, build_uint16_array, build_uint32_array, build_uint64_array,
    },
};

/// Builds an Arrow RecordBatch from events
pub(crate) fn build_record_batch(
    schema: Arc<Schema>,
    events: &[Event],
) -> Result<RecordBatch, ArrowEncodingError> {
    let num_fields = schema.fields().len();
    let mut columns: Vec<ArrayRef> = Vec::with_capacity(num_fields);

    for field in schema.fields() {
        let field_name = field.name();
        let nullable = field.is_nullable();
        let array: ArrayRef = match field.data_type() {
            DataType::Timestamp(time_unit, _) => {
                build_timestamp_array(events, field_name, *time_unit, nullable)?
            }
            DataType::Utf8 => build_string_array(events, field_name, nullable)?,
            DataType::Int8 => build_int8_array(events, field_name, nullable)?,
            DataType::Int16 => build_int16_array(events, field_name, nullable)?,
            DataType::Int32 => build_int32_array(events, field_name, nullable)?,
            DataType::Int64 => build_int64_array(events, field_name, nullable)?,
            DataType::UInt8 => build_uint8_array(events, field_name, nullable)?,
            DataType::UInt16 => build_uint16_array(events, field_name, nullable)?,
            DataType::UInt32 => build_uint32_array(events, field_name, nullable)?,
            DataType::UInt64 => build_uint64_array(events, field_name, nullable)?,
            DataType::Float32 => build_float32_array(events, field_name, nullable)?,
            DataType::Float64 => build_float64_array(events, field_name, nullable)?,
            DataType::Boolean => build_boolean_array(events, field_name, nullable)?,
            DataType::Binary => build_binary_array(events, field_name, nullable)?,
            DataType::Decimal128(precision, scale) => {
                build_decimal128_array(events, field_name, *precision, *scale, nullable)?
            }
            DataType::Decimal256(precision, scale) => {
                build_decimal256_array(events, field_name, *precision, *scale, nullable)?
            }
            other_type => {
                return Err(ArrowEncodingError::UnsupportedType {
                    field_name: field_name.into(),
                    data_type: other_type.clone(),
                });
            }
        };

        columns.push(array);
    }

    RecordBatch::try_new(schema, columns)
        .map_err(|source| ArrowEncodingError::RecordBatchCreation { source })
}
