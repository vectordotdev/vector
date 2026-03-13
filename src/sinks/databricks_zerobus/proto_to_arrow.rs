//! Conversion from protobuf `MessageDescriptor` to Arrow `Schema`.
//!
//! Maps protobuf field types to their Arrow equivalents so that the
//! `ArrowStreamSerializer` can encode Vector events using the table's
//! protobuf schema definition.

use std::sync::Arc;

use arrow::datatypes::{DataType, Field, Fields, Schema};
use prost_reflect::{Cardinality, FieldDescriptor, Kind, MessageDescriptor};

use super::error::ZerobusSinkError;

/// Convert a protobuf `MessageDescriptor` into an Arrow `Schema`.
pub fn proto_descriptor_to_arrow_schema(
    descriptor: &MessageDescriptor,
) -> Result<Schema, ZerobusSinkError> {
    let fields: Result<Vec<Field>, _> = descriptor
        .fields()
        .map(|field| proto_field_to_arrow_field(&field))
        .collect();

    Ok(Schema::new(fields?))
}

/// Convert a single protobuf field descriptor into an Arrow `Field`.
fn proto_field_to_arrow_field(field: &FieldDescriptor) -> Result<Field, ZerobusSinkError> {
    let name = field.name().to_string();
    let nullable = field.cardinality() != Cardinality::Required;

    if field.is_map() {
        return Err(ZerobusSinkError::ConfigError {
            message: format!(
                "Map fields are not supported in proto-to-Arrow conversion (field: '{}')",
                name
            ),
        });
    }

    let data_type = proto_kind_to_arrow_type(&field.kind(), &name)?;

    if field.is_list() {
        Ok(Field::new(
            name,
            DataType::List(Arc::new(Field::new("item", data_type, true))),
            nullable,
        ))
    } else {
        Ok(Field::new(name, data_type, nullable))
    }
}

/// Map a protobuf `Kind` to an Arrow `DataType`.
fn proto_kind_to_arrow_type(kind: &Kind, _field_name: &str) -> Result<DataType, ZerobusSinkError> {
    match kind {
        Kind::Double => Ok(DataType::Float64),
        Kind::Float => Ok(DataType::Float32),
        Kind::Int32 | Kind::Sint32 | Kind::Sfixed32 => Ok(DataType::Int32),
        Kind::Int64 | Kind::Sint64 | Kind::Sfixed64 => Ok(DataType::Int64),
        Kind::Uint32 | Kind::Fixed32 => Ok(DataType::UInt32),
        Kind::Uint64 | Kind::Fixed64 => Ok(DataType::UInt64),
        Kind::Bool => Ok(DataType::Boolean),
        Kind::String => Ok(DataType::LargeUtf8),
        Kind::Bytes => Ok(DataType::LargeBinary),
        Kind::Enum(_) => Ok(DataType::Int32),
        Kind::Message(msg_descriptor) => {
            let fields: Result<Vec<Field>, _> = msg_descriptor
                .fields()
                .map(|f| proto_field_to_arrow_field(&f))
                .collect();
            Ok(DataType::Struct(Fields::from(fields?)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use vrl::protobuf::descriptor::get_message_descriptor;

    /// Load the test User descriptor from the test .desc file.
    /// Message: test_proto.User { string id, string name, int32 age, repeated string emails }
    fn load_test_user_descriptor() -> MessageDescriptor {
        let path = Path::new("tests/data/protobuf/test_proto.desc");
        get_message_descriptor(path, "test_proto.User")
            .expect("Failed to load test_proto.User descriptor")
    }

    #[test]
    fn test_user_schema_fields() {
        let descriptor = load_test_user_descriptor();
        let schema = proto_descriptor_to_arrow_schema(&descriptor).unwrap();

        assert_eq!(schema.fields().len(), 4);

        let id_field = schema.field_with_name("id").unwrap();
        assert_eq!(id_field.data_type(), &DataType::LargeUtf8);

        let name_field = schema.field_with_name("name").unwrap();
        assert_eq!(name_field.data_type(), &DataType::LargeUtf8);

        let age_field = schema.field_with_name("age").unwrap();
        assert_eq!(age_field.data_type(), &DataType::Int32);
    }

    #[test]
    fn test_repeated_field_becomes_list() {
        let descriptor = load_test_user_descriptor();
        let schema = proto_descriptor_to_arrow_schema(&descriptor).unwrap();

        let emails_field = schema.field_with_name("emails").unwrap();
        match emails_field.data_type() {
            DataType::List(inner) => {
                assert_eq!(inner.data_type(), &DataType::LargeUtf8);
            }
            other => panic!("Expected List, got {:?}", other),
        }
    }

    #[test]
    fn test_proto3_fields_are_nullable() {
        let descriptor = load_test_user_descriptor();
        let schema = proto_descriptor_to_arrow_schema(&descriptor).unwrap();

        for field in schema.fields() {
            assert!(
                field.is_nullable(),
                "Field '{}' should be nullable in proto3",
                field.name()
            );
        }
    }

    #[test]
    fn test_nested_message_from_unity_catalog() {
        use super::super::unity_catalog_schema::{
            UnityCatalogColumn, UnityCatalogTableSchema, generate_descriptor_from_schema,
        };

        let schema = UnityCatalogTableSchema {
            name: "test_table".to_string(),
            catalog_name: "test_catalog".to_string(),
            schema_name: "test_schema".to_string(),
            columns: vec![
                UnityCatalogColumn {
                    name: "id".to_string(),
                    type_text: "LONG".to_string(),
                    type_name: "LONG".to_string(),
                    position: 0,
                    nullable: false,
                    type_json: String::new(),
                },
                UnityCatalogColumn {
                    name: "name".to_string(),
                    type_text: "STRING".to_string(),
                    type_name: "STRING".to_string(),
                    position: 1,
                    nullable: true,
                    type_json: String::new(),
                },
                UnityCatalogColumn {
                    name: "score".to_string(),
                    type_text: "DOUBLE".to_string(),
                    type_name: "DOUBLE".to_string(),
                    position: 2,
                    nullable: true,
                    type_json: String::new(),
                },
                UnityCatalogColumn {
                    name: "active".to_string(),
                    type_text: "BOOLEAN".to_string(),
                    type_name: "BOOLEAN".to_string(),
                    position: 3,
                    nullable: false,
                    type_json: String::new(),
                },
            ],
        };

        let descriptor =
            generate_descriptor_from_schema(&schema).expect("Failed to generate descriptor");
        let arrow_schema = proto_descriptor_to_arrow_schema(&descriptor).unwrap();

        assert_eq!(arrow_schema.fields().len(), 4);

        let id_field = arrow_schema.field_with_name("id").unwrap();
        assert_eq!(id_field.data_type(), &DataType::Int64);

        let name_field = arrow_schema.field_with_name("name").unwrap();
        assert_eq!(name_field.data_type(), &DataType::LargeUtf8);

        let score_field = arrow_schema.field_with_name("score").unwrap();
        assert_eq!(score_field.data_type(), &DataType::Float64);

        let active_field = arrow_schema.field_with_name("active").unwrap();
        assert_eq!(active_field.data_type(), &DataType::Boolean);
    }
}
