//! Schema definition support for Arrow and Parquet encoders

use std::{collections::BTreeMap, sync::Arc};

use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
#[allow(unused_imports)] // Used by vector_config macros
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use vector_config::configurable_component;

/// Error type for schema definition parsing
#[derive(Debug, Snafu)]
pub enum SchemaDefinitionError {
    /// Unknown data type specified in schema
    #[snafu(display("Unknown data type '{}' for field '{}'", data_type, field_name))]
    UnknownDataType {
        /// The field name that had an unknown type
        field_name: String,
        /// The unknown data type string
        data_type: String,
    },
}

/// A schema definition that can be deserialized from configuration
#[configurable_component]
#[derive(Debug, Clone)]
#[serde(untagged)]
pub enum SchemaDefinition {
    /// Simple map of field names to type names
    Simple(BTreeMap<String, String>),
}

impl SchemaDefinition {
    /// Convert the schema definition to an Arrow Schema
    pub fn to_arrow_schema(&self) -> Result<Arc<Schema>, SchemaDefinitionError> {
        match self {
            SchemaDefinition::Simple(fields) => {
                let arrow_fields: Result<Vec<_>, _> = fields
                    .iter()
                    .map(|(name, type_str)| {
                        let data_type = parse_data_type(type_str, name)?;
                        // All fields are nullable by default when defined in config
                        Ok(Arc::new(Field::new(name, data_type, true)))
                    })
                    .collect();

                Ok(Arc::new(Schema::new(arrow_fields?)))
            }
        }
    }
}

/// Parse a data type string into an Arrow DataType
fn parse_data_type(
    type_str: &str,
    field_name: &str,
) -> Result<DataType, SchemaDefinitionError> {
    let data_type = match type_str.to_lowercase().as_str() {
        // String types
        "utf8" | "string" => DataType::Utf8,
        "large_utf8" | "large_string" => DataType::LargeUtf8,

        // Integer types
        "int8" => DataType::Int8,
        "int16" => DataType::Int16,
        "int32" => DataType::Int32,
        "int64" => DataType::Int64,

        // Unsigned integer types
        "uint8" => DataType::UInt8,
        "uint16" => DataType::UInt16,
        "uint32" => DataType::UInt32,
        "uint64" => DataType::UInt64,

        // Floating point types
        "float32" | "float" => DataType::Float32,
        "float64" | "double" => DataType::Float64,

        // Boolean
        "boolean" | "bool" => DataType::Boolean,

        // Binary types
        "binary" => DataType::Binary,
        "large_binary" => DataType::LargeBinary,

        // Timestamp types
        "timestamp_second" | "timestamp_s" => {
            DataType::Timestamp(TimeUnit::Second, None)
        }
        "timestamp_millisecond" | "timestamp_ms" | "timestamp_millis" => {
            DataType::Timestamp(TimeUnit::Millisecond, None)
        }
        "timestamp_microsecond" | "timestamp_us" | "timestamp_micros" => {
            DataType::Timestamp(TimeUnit::Microsecond, None)
        }
        "timestamp_nanosecond" | "timestamp_ns" | "timestamp_nanos" => {
            DataType::Timestamp(TimeUnit::Nanosecond, None)
        }

        // Date types
        "date32" | "date" => DataType::Date32,
        "date64" => DataType::Date64,

        // Time types
        "time32_second" | "time32_s" => DataType::Time32(TimeUnit::Second),
        "time32_millisecond" | "time32_ms" => DataType::Time32(TimeUnit::Millisecond),
        "time64_microsecond" | "time64_us" => DataType::Time64(TimeUnit::Microsecond),
        "time64_nanosecond" | "time64_ns" => DataType::Time64(TimeUnit::Nanosecond),

        // Duration types
        "duration_second" | "duration_s" => DataType::Duration(TimeUnit::Second),
        "duration_millisecond" | "duration_ms" => DataType::Duration(TimeUnit::Millisecond),
        "duration_microsecond" | "duration_us" => DataType::Duration(TimeUnit::Microsecond),
        "duration_nanosecond" | "duration_ns" => DataType::Duration(TimeUnit::Nanosecond),

        // Decimal types
        "decimal128" => DataType::Decimal128(38, 10), // Default precision and scale
        "decimal256" => DataType::Decimal256(76, 10), // Default precision and scale

        // Unknown type
        _ => {
            return Err(SchemaDefinitionError::UnknownDataType {
                field_name: field_name.to_string(),
                data_type: type_str.to_string(),
            })
        }
    };

    Ok(data_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_schema_definition() {
        let mut fields = BTreeMap::new();
        fields.insert("id".to_string(), "int64".to_string());
        fields.insert("name".to_string(), "utf8".to_string());
        fields.insert("value".to_string(), "float64".to_string());

        let schema_def = SchemaDefinition::Simple(fields);
        let schema = schema_def.to_arrow_schema().unwrap();

        assert_eq!(schema.fields().len(), 3);

        let id_field = schema.field_with_name("id").unwrap();
        assert_eq!(id_field.data_type(), &DataType::Int64);
        assert!(id_field.is_nullable());

        let name_field = schema.field_with_name("name").unwrap();
        assert_eq!(name_field.data_type(), &DataType::Utf8);

        let value_field = schema.field_with_name("value").unwrap();
        assert_eq!(value_field.data_type(), &DataType::Float64);
    }

    #[test]
    fn test_timestamp_types() {
        let mut fields = BTreeMap::new();
        fields.insert("ts_s".to_string(), "timestamp_second".to_string());
        fields.insert("ts_ms".to_string(), "timestamp_millisecond".to_string());
        fields.insert("ts_us".to_string(), "timestamp_microsecond".to_string());
        fields.insert("ts_ns".to_string(), "timestamp_nanosecond".to_string());

        let schema_def = SchemaDefinition::Simple(fields);
        let schema = schema_def.to_arrow_schema().unwrap();

        assert_eq!(
            schema.field_with_name("ts_s").unwrap().data_type(),
            &DataType::Timestamp(TimeUnit::Second, None)
        );
        assert_eq!(
            schema.field_with_name("ts_ms").unwrap().data_type(),
            &DataType::Timestamp(TimeUnit::Millisecond, None)
        );
        assert_eq!(
            schema.field_with_name("ts_us").unwrap().data_type(),
            &DataType::Timestamp(TimeUnit::Microsecond, None)
        );
        assert_eq!(
            schema.field_with_name("ts_ns").unwrap().data_type(),
            &DataType::Timestamp(TimeUnit::Nanosecond, None)
        );
    }

    #[test]
    fn test_unknown_data_type() {
        let mut fields = BTreeMap::new();
        fields.insert("bad_field".to_string(), "unknown_type".to_string());

        let schema_def = SchemaDefinition::Simple(fields);
        let result = schema_def.to_arrow_schema();

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("unknown_type"));
    }

}
