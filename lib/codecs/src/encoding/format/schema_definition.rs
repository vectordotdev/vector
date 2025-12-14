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

/// Per-column configuration including type and Bloom filter settings
#[configurable_component]
#[derive(Debug, Clone)]
pub struct FieldDefinition {
    /// Data type for this field
    #[configurable(metadata(docs::examples = "utf8"))]
    #[configurable(metadata(docs::examples = "int64"))]
    #[configurable(metadata(docs::examples = "timestamp_ms"))]
    pub r#type: String,

    /// Enable Bloom filter for this specific column
    ///
    /// When enabled, a Bloom filter will be created for this column to improve
    /// query performance for point lookups and IN clauses. Only enable for
    /// high-cardinality columns (UUIDs, user IDs, etc.) to avoid overhead.
    #[serde(default)]
    #[configurable(metadata(docs::examples = true))]
    pub bloom_filter: bool,

    /// Number of distinct values expected for this column's Bloom filter
    ///
    /// This controls the size of the Bloom filter. Should match the actual
    /// cardinality of the column. Will be automatically capped to the batch size.
    ///
    /// - Low cardinality (countries, states): 1,000 - 100,000
    /// - Medium cardinality (cities, products): 100,000 - 1,000,000
    /// - High cardinality (UUIDs, user IDs): 10,000,000+
    #[serde(default, alias = "bloom_filter_ndv")]
    #[configurable(metadata(docs::examples = 1000000))]
    #[configurable(metadata(docs::examples = 10000000))]
    pub bloom_filter_num_distinct_values: Option<u64>,

    /// False positive probability for this column's Bloom filter (as a percentage)
    ///
    /// Lower values create larger but more accurate filters.
    ///
    /// - 0.05 (5%): Good balance for general use
    /// - 0.01 (1%): Better for high-selectivity queries
    #[serde(default, alias = "bloom_filter_fpp")]
    #[configurable(metadata(docs::examples = 0.05))]
    #[configurable(metadata(docs::examples = 0.01))]
    pub bloom_filter_false_positive_pct: Option<f64>,
}

/// Bloom filter configuration for a specific column
#[derive(Debug, Clone)]
pub struct ColumnBloomFilterConfig {
    /// Column name
    pub column_name: String,
    /// Whether Bloom filter is enabled for this column
    pub enabled: bool,
    /// Number of distinct values (if specified)
    pub ndv: Option<u64>,
    /// False positive probability (if specified)
    pub fpp: Option<f64>,
}

/// A schema definition that can be deserialized from configuration
#[configurable_component]
#[derive(Debug, Clone)]
pub struct SchemaDefinition {
    /// Map of field names to their type and Bloom filter configuration
    #[serde(flatten)]
    #[configurable(metadata(docs::additional_props_description = "A field definition specifying the data type and optional Bloom filter configuration."))]
    pub fields: BTreeMap<String, FieldDefinition>,
}

impl SchemaDefinition {
    /// Convert the schema definition to an Arrow Schema
    pub fn to_arrow_schema(&self) -> Result<Arc<Schema>, SchemaDefinitionError> {
        let arrow_fields: Result<Vec<_>, _> = self
            .fields
            .iter()
            .map(|(name, field_def)| {
                let data_type = parse_data_type(&field_def.r#type, name)?;
                // All fields are nullable by default when defined in config
                Ok(Arc::new(Field::new(name, data_type, true)))
            })
            .collect();

        Ok(Arc::new(Schema::new(arrow_fields?)))
    }

    /// Extract per-column Bloom filter configurations
    pub fn extract_bloom_filter_configs(&self) -> Vec<ColumnBloomFilterConfig> {
        self.fields
            .iter()
            .filter_map(|(name, field_def)| {
                if field_def.bloom_filter {
                    Some(ColumnBloomFilterConfig {
                        column_name: name.clone(),
                        enabled: true,
                        ndv: field_def.bloom_filter_num_distinct_values,
                        fpp: field_def.bloom_filter_false_positive_pct,
                    })
                } else {
                    None
                }
            })
            .collect()
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
        fields.insert(
            "id".to_string(),
            FieldDefinition {
                r#type: "int64".to_string(),
                bloom_filter: false,
                bloom_filter_num_distinct_values: None,
                bloom_filter_false_positive_pct: None,
            },
        );
        fields.insert(
            "name".to_string(),
            FieldDefinition {
                r#type: "utf8".to_string(),
                bloom_filter: false,
                bloom_filter_num_distinct_values: None,
                bloom_filter_false_positive_pct: None,
            },
        );
        fields.insert(
            "value".to_string(),
            FieldDefinition {
                r#type: "float64".to_string(),
                bloom_filter: false,
                bloom_filter_num_distinct_values: None,
                bloom_filter_false_positive_pct: None,
            },
        );

        let schema_def = SchemaDefinition { fields };
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
        fields.insert(
            "ts_s".to_string(),
            FieldDefinition {
                r#type: "timestamp_second".to_string(),
                bloom_filter: false,
                bloom_filter_num_distinct_values: None,
                bloom_filter_false_positive_pct: None,
            },
        );
        fields.insert(
            "ts_ms".to_string(),
            FieldDefinition {
                r#type: "timestamp_millisecond".to_string(),
                bloom_filter: false,
                bloom_filter_num_distinct_values: None,
                bloom_filter_false_positive_pct: None,
            },
        );
        fields.insert(
            "ts_us".to_string(),
            FieldDefinition {
                r#type: "timestamp_microsecond".to_string(),
                bloom_filter: false,
                bloom_filter_num_distinct_values: None,
                bloom_filter_false_positive_pct: None,
            },
        );
        fields.insert(
            "ts_ns".to_string(),
            FieldDefinition {
                r#type: "timestamp_nanosecond".to_string(),
                bloom_filter: false,
                bloom_filter_num_distinct_values: None,
                bloom_filter_false_positive_pct: None,
            },
        );

        let schema_def = SchemaDefinition { fields };
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
        fields.insert(
            "bad_field".to_string(),
            FieldDefinition {
                r#type: "unknown_type".to_string(),
                bloom_filter: false,
                bloom_filter_num_distinct_values: None,
                bloom_filter_false_positive_pct: None,
            },
        );

        let schema_def = SchemaDefinition { fields };
        let result = schema_def.to_arrow_schema();

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("unknown_type"));
    }

    #[test]
    fn test_bloom_filter_extraction() {
        let mut fields = BTreeMap::new();
        fields.insert(
            "id".to_string(),
            FieldDefinition {
                r#type: "int64".to_string(),
                bloom_filter: false,
                bloom_filter_num_distinct_values: None,
                bloom_filter_false_positive_pct: None,
            },
        );
        fields.insert(
            "user_id".to_string(),
            FieldDefinition {
                r#type: "utf8".to_string(),
                bloom_filter: true,
                bloom_filter_num_distinct_values: Some(10_000_000),
                bloom_filter_false_positive_pct: Some(0.01),
            },
        );
        fields.insert(
            "request_id".to_string(),
            FieldDefinition {
                r#type: "utf8".to_string(),
                bloom_filter: true,
                bloom_filter_num_distinct_values: None, // Will use global default
                bloom_filter_false_positive_pct: None,
            },
        );

        let schema_def = SchemaDefinition { fields };
        let bloom_configs = schema_def.extract_bloom_filter_configs();

        assert_eq!(bloom_configs.len(), 2);

        // Check user_id config
        let user_id_config = bloom_configs
            .iter()
            .find(|c| c.column_name == "user_id")
            .unwrap();
        assert!(user_id_config.enabled);
        assert_eq!(user_id_config.ndv, Some(10_000_000));
        assert_eq!(user_id_config.fpp, Some(0.01));

        // Check request_id config
        let request_id_config = bloom_configs
            .iter()
            .find(|c| c.column_name == "request_id")
            .unwrap();
        assert!(request_id_config.enabled);
        assert_eq!(request_id_config.ndv, None);
        assert_eq!(request_id_config.fpp, None);
    }

}
