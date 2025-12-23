use arrow::array::{
    ArrayBuilder, ListBuilder, MapBuilder, StringBuilder, StructBuilder, make_builder,
};
use arrow::datatypes::DataType;

use super::ArrowEncodingError;

mod complex;
mod decimal;
mod primitives;
mod temporal;

pub(crate) use complex::{build_list_array, build_map_array, build_struct_array};
pub(crate) use decimal::{build_decimal128_array, build_decimal256_array};
pub(crate) use primitives::{
    build_binary_array, build_boolean_array, build_float32_array, build_float64_array,
    build_int8_array, build_int16_array, build_int32_array, build_int64_array, build_string_array,
    build_uint8_array, build_uint16_array, build_uint32_array, build_uint64_array,
};
pub(crate) use temporal::build_timestamp_array;

const NESTED_CAPACITY_MULTIPLIER: usize = 4;

/// Creates an array builder for a given Arrow data type.
///
/// Uses Arrow's `make_builder` for most types, but provides custom handling
/// for complex nested types (List, Struct, Map) to ensure proper recursive
/// builder creation, especially for nested Maps which `make_builder` doesn't
/// fully support.
pub(crate) fn create_array_builder_for_type(
    data_type: &DataType,
    capacity: usize,
) -> Result<Box<dyn ArrayBuilder>, ArrowEncodingError> {
    match data_type {
        DataType::List(inner_field) => {
            create_list_builder(inner_field.data_type(), capacity)
        }
        DataType::Struct(fields) => {
            create_struct_builder(fields, capacity)
        }
        DataType::Map(entries_field, _) => {
            create_map_builder(entries_field.data_type(), capacity)
        }
        _ => Ok(make_builder(data_type, capacity)),
    }
}

fn create_list_builder(
    inner_type: &DataType,
    capacity: usize,
) -> Result<Box<dyn ArrayBuilder>, ArrowEncodingError> {
    let nested_capacity = capacity * NESTED_CAPACITY_MULTIPLIER;
    let inner_builder = create_array_builder_for_type(inner_type, nested_capacity)?;
    Ok(Box::new(ListBuilder::new(inner_builder)))
}

fn create_struct_builder(
    fields: &arrow::datatypes::Fields,
    capacity: usize,
) -> Result<Box<dyn ArrayBuilder>, ArrowEncodingError> {
    let field_builders = fields
        .iter()
        .map(|f| create_array_builder_for_type(f.data_type(), capacity))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Box::new(StructBuilder::new(fields.clone(), field_builders)))
}

fn create_map_builder(
    entries_type: &DataType,
    capacity: usize,
) -> Result<Box<dyn ArrayBuilder>, ArrowEncodingError> {
    let DataType::Struct(entries_fields) = entries_type else {
        return Err(ArrowEncodingError::UnsupportedType {
            field_name: "dynamic".into(),
            data_type: entries_type.clone(),
        });
    };

    let nested_capacity = capacity * NESTED_CAPACITY_MULTIPLIER;
    let key_builder = StringBuilder::with_capacity(nested_capacity, 0);
    let value_builder = create_array_builder_for_type(
        entries_fields[1].data_type(),
        nested_capacity,
    )?;
    
    Ok(Box::new(MapBuilder::new(None, key_builder, value_builder)))
}
