//! Unity Catalog schema fetching and protobuf descriptor generation.

use bytes::Buf;
use http::{Request, Uri};
use http_body::Body as HttpBody;
use hyper::Body;
use percent_encoding::{NON_ALPHANUMERIC, percent_encode};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use prost_reflect::prost_types;

use super::error::ZerobusSinkError;
use crate::config::ProxyConfig;
use crate::http::HttpClient;
use crate::tls::TlsSettings;

/// Unity Catalog table column information
#[derive(Debug, Deserialize, Clone)]
pub struct UnityCatalogColumn {
    pub name: String,
    #[allow(dead_code)] // Will be used for complex type parsing
    pub type_text: String,
    pub type_name: String,
    #[serde(default)]
    pub position: i32,
    pub nullable: bool,
    #[serde(default)]
    pub type_json: String,
}

/// Unity Catalog table schema response
#[derive(Debug, Deserialize)]
pub struct UnityCatalogTableSchema {
    pub name: String,
    pub catalog_name: String,
    pub schema_name: String,
    pub columns: Vec<UnityCatalogColumn>,
}

/// OAuth token response from Databricks
#[derive(Debug, Deserialize)]
struct OAuthTokenResponse {
    access_token: String,
}

/// Represents a parsed complex type from type_json
#[derive(Debug, Clone)]
enum ComplexType {
    Primitive(PrimitiveType),
    Struct(StructType),
    Array(Box<ComplexType>),
    Map {
        key_type: Box<ComplexType>,
        value_type: Box<ComplexType>,
    },
}

/// Primitive types
#[derive(Debug, Clone)]
enum PrimitiveType {
    String,
    Long,
    Integer,
    Short,
    Byte,
    Double,
    Float,
    Boolean,
    Binary,
    Timestamp,
    Date,
    Decimal { _precision: i32, _scale: i32 },
}

/// Struct field definition from type_json
#[derive(Debug, Clone)]
struct StructField {
    name: String,
    field_type: ComplexType,
    nullable: bool,
}

/// Struct type definition
#[derive(Debug, Clone)]
struct StructType {
    fields: Vec<StructField>,
}

/// Fetch table schema from Unity Catalog API
pub async fn fetch_table_schema(
    unity_catalog_endpoint: &str,
    table_name: &str,
    client_id: &str,
    client_secret: &str,
) -> Result<UnityCatalogTableSchema, ZerobusSinkError> {
    let http_client =
        HttpClient::new(TlsSettings::default(), &ProxyConfig::default()).map_err(|e| {
            ZerobusSinkError::ConfigError {
                message: format!("Failed to create HTTP client: {}", e),
            }
        })?;

    // First, get OAuth token
    let token =
        get_oauth_token(&http_client, unity_catalog_endpoint, client_id, client_secret).await?;

    // Fetch table schema
    let url = format!(
        "{}/api/2.0/unity-catalog/tables/{}",
        unity_catalog_endpoint.trim_end_matches('/'),
        table_name
    );

    let uri: Uri = url.parse().map_err(|e| ZerobusSinkError::ConfigError {
        message: format!("Invalid Unity Catalog endpoint URL: {}", e),
    })?;

    let request = Request::get(uri)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::empty())
        .map_err(|e| ZerobusSinkError::ConfigError {
            message: format!("Failed to build request: {}", e),
        })?;

    let response = http_client
        .send(request)
        .await
        .map_err(|e| ZerobusSinkError::ConfigError {
            message: format!("Failed to fetch table schema: {}", e),
        })?;

    let status = response.status();
    if !status.is_success() {
        let body_bytes = response
            .into_body()
            .collect()
            .await
            .map(|c| c.to_bytes())
            .unwrap_or_default();
        let error_text = String::from_utf8_lossy(&body_bytes);
        return Err(ZerobusSinkError::ConfigError {
            message: format!(
                "Unity Catalog API returned error {}: {}",
                status, error_text
            ),
        });
    }

    let body_bytes = response
        .into_body()
        .collect()
        .await
        .map(|c| c.to_bytes())
        .map_err(|e| ZerobusSinkError::ConfigError {
            message: format!("Failed to read response body: {}", e),
        })?;

    let schema: UnityCatalogTableSchema =
        serde_json::from_reader(body_bytes.reader()).map_err(|e| {
            ZerobusSinkError::ConfigError {
                message: format!("Failed to parse table schema response: {}", e),
            }
        })?;

    Ok(schema)
}

/// Get OAuth token from Databricks
async fn get_oauth_token(
    http_client: &HttpClient,
    unity_catalog_endpoint: &str,
    client_id: &str,
    client_secret: &str,
) -> Result<String, ZerobusSinkError> {
    let token_url = format!(
        "{}/oidc/v1/token",
        unity_catalog_endpoint.trim_end_matches('/')
    );

    let uri: Uri = token_url
        .parse()
        .map_err(|e| ZerobusSinkError::ConfigError {
            message: format!("Invalid token endpoint URL: {}", e),
        })?;

    // Build form-encoded body
    let form_body = format!(
        "grant_type=client_credentials&client_id={}&client_secret={}&scope=all-apis",
        percent_encode(client_id.as_bytes(), NON_ALPHANUMERIC),
        percent_encode(client_secret.as_bytes(), NON_ALPHANUMERIC)
    );

    let request = Request::post(uri)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(Body::from(form_body))
        .map_err(|e| ZerobusSinkError::ConfigError {
            message: format!("Failed to build OAuth request: {}", e),
        })?;

    let response = http_client
        .send(request)
        .await
        .map_err(|e| ZerobusSinkError::ConfigError {
            message: format!("Failed to get OAuth token: {}", e),
        })?;

    let status = response.status();
    if !status.is_success() {
        let body_bytes = response
            .into_body()
            .collect()
            .await
            .map(|c| c.to_bytes())
            .unwrap_or_default();
        let error_text = String::from_utf8_lossy(&body_bytes);
        return Err(ZerobusSinkError::ConfigError {
            message: format!("OAuth token request failed {}: {}", status, error_text),
        });
    }

    let body_bytes = response
        .into_body()
        .collect()
        .await
        .map(|c| c.to_bytes())
        .map_err(|e| ZerobusSinkError::ConfigError {
            message: format!("Failed to read OAuth response body: {}", e),
        })?;

    let token_response: OAuthTokenResponse =
        serde_json::from_reader(body_bytes.reader()).map_err(|e| {
            ZerobusSinkError::ConfigError {
                message: format!("Failed to parse OAuth token response: {}", e),
            }
        })?;

    Ok(token_response.access_token)
}

/// Parse type_json string into ComplexType
fn parse_type_json(type_json: &str) -> Result<ComplexType, ZerobusSinkError> {
    if type_json.is_empty() || type_json == "{}" {
        return Err(ZerobusSinkError::ConfigError {
            message: "Empty type_json".to_string(),
        });
    }

    let json: JsonValue =
        serde_json::from_str(type_json).map_err(|e| ZerobusSinkError::ConfigError {
            message: format!("Failed to parse type_json: {}", e),
        })?;

    // Unity Catalog wraps types in {"name": "field_name", "type": {...}}
    // Check if this is the wrapped format by looking for both "name" and "type" fields
    let type_json = if let Some(obj) = json.as_object() {
        if obj.contains_key("name") && obj.contains_key("type") {
            // This is Unity Catalog wrapped format - extract the inner "type"
            obj.get("type").unwrap()
        } else {
            // This is a direct type definition
            &json
        }
    } else {
        &json
    };

    parse_complex_type(type_json)
}

/// Recursively parse a complex type from JSON
fn parse_complex_type(json: &JsonValue) -> Result<ComplexType, ZerobusSinkError> {
    // Handle simple string types (for nested fields)
    if let Some(type_str) = json.as_str() {
        return parse_primitive_type(type_str);
    }

    // Handle type object
    let type_obj = json
        .as_object()
        .ok_or_else(|| ZerobusSinkError::ConfigError {
            message: format!("Expected type object, got: {:?}", json),
        })?;

    // Get the "type" field
    let type_field = type_obj
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ZerobusSinkError::ConfigError {
            message: format!("Missing 'type' field in type_json: {:?}", type_obj),
        })?;

    match type_field {
        "struct" => parse_struct_type(type_obj),
        "array" => parse_array_type(type_obj),
        "map" => parse_map_type(type_obj),
        primitive => parse_primitive_type(primitive),
    }
}

/// Parse primitive type string
fn parse_primitive_type(type_str: &str) -> Result<ComplexType, ZerobusSinkError> {
    let primitive = match type_str {
        "string" => PrimitiveType::String,
        "long" => PrimitiveType::Long,
        "integer" => PrimitiveType::Integer,
        "short" => PrimitiveType::Short,
        "byte" => PrimitiveType::Byte,
        "double" => PrimitiveType::Double,
        "float" => PrimitiveType::Float,
        "boolean" => PrimitiveType::Boolean,
        "binary" => PrimitiveType::Binary,
        "timestamp" => PrimitiveType::Timestamp,
        "date" => PrimitiveType::Date,
        other if other.starts_with("decimal") => {
            // Parse decimal(precision, scale)
            PrimitiveType::Decimal {
                _precision: 38,
                _scale: 10,
            } // Default values
        }
        unknown => {
            return Err(ZerobusSinkError::ConfigError {
                message: format!("Unknown primitive type: {}", unknown),
            });
        }
    };
    Ok(ComplexType::Primitive(primitive))
}

/// Parse STRUCT type
fn parse_struct_type(
    type_obj: &serde_json::Map<String, JsonValue>,
) -> Result<ComplexType, ZerobusSinkError> {
    let fields_json = type_obj
        .get("fields")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ZerobusSinkError::ConfigError {
            message: "STRUCT type missing 'fields' array".to_string(),
        })?;

    let mut fields = Vec::new();
    for field_json in fields_json {
        let field_obj = field_json
            .as_object()
            .ok_or_else(|| ZerobusSinkError::ConfigError {
                message: format!("Expected field object, got: {:?}", field_json),
            })?;

        let name = field_obj
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ZerobusSinkError::ConfigError {
                message: "Field missing 'name'".to_string(),
            })?
            .to_string();

        let nullable = field_obj
            .get("nullable")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        // Parse the field type (can be nested)
        let field_type_json =
            field_obj
                .get("type")
                .ok_or_else(|| ZerobusSinkError::ConfigError {
                    message: format!("Field '{}' missing 'type'", name),
                })?;

        let field_type = parse_complex_type(field_type_json)?;

        fields.push(StructField {
            name,
            field_type,
            nullable,
        });
    }

    Ok(ComplexType::Struct(StructType { fields }))
}

/// Parse ARRAY type
fn parse_array_type(
    type_obj: &serde_json::Map<String, JsonValue>,
) -> Result<ComplexType, ZerobusSinkError> {
    let element_type_json =
        type_obj
            .get("elementType")
            .ok_or_else(|| ZerobusSinkError::ConfigError {
                message: "ARRAY type missing 'elementType'".to_string(),
            })?;

    let element_type = parse_complex_type(element_type_json)?;
    Ok(ComplexType::Array(Box::new(element_type)))
}

/// Parse MAP type
fn parse_map_type(
    type_obj: &serde_json::Map<String, JsonValue>,
) -> Result<ComplexType, ZerobusSinkError> {
    let key_type_json = type_obj
        .get("keyType")
        .ok_or_else(|| ZerobusSinkError::ConfigError {
            message: "MAP type missing 'keyType'".to_string(),
        })?;

    let value_type_json =
        type_obj
            .get("valueType")
            .ok_or_else(|| ZerobusSinkError::ConfigError {
                message: "MAP type missing 'valueType'".to_string(),
            })?;

    let key_type = parse_complex_type(key_type_json)?;
    let value_type = parse_complex_type(value_type_json)?;

    Ok(ComplexType::Map {
        key_type: Box::new(key_type),
        value_type: Box::new(value_type),
    })
}

/// Helper structure to collect nested message types during generation
struct MessageCollector {
    /// All nested message definitions
    nested_messages: Vec<prost_types::DescriptorProto>,
}

impl MessageCollector {
    const fn new() -> Self {
        Self {
            nested_messages: Vec::new(),
        }
    }

    /// Add a nested message definition
    fn add_message(&mut self, message: prost_types::DescriptorProto) {
        self.nested_messages.push(message);
    }
}

/// Format a protobuf MessageDescriptor as a .proto file string for logging
fn format_descriptor_as_proto(descriptor: &prost_reflect::MessageDescriptor) -> String {
    let mut output = String::new();
    format_message_as_proto(descriptor, &mut output, 0);
    output
}

/// Recursively format a message and its nested types
fn format_message_as_proto(
    descriptor: &prost_reflect::MessageDescriptor,
    output: &mut String,
    indent_level: usize,
) {
    let indent = "  ".repeat(indent_level);

    // Write message header
    output.push_str(&format!("{}message {} {{\n", indent, descriptor.name()));

    // Write fields
    for field in descriptor.fields() {
        let field_indent = "  ".repeat(indent_level + 1);
        let field_type = format_field_type(&field);
        let field_number = field.number();
        output.push_str(&format!(
            "{}{}{} = {};\n",
            field_indent,
            field_type,
            field.name(),
            field_number
        ));
    }

    output.push_str(&format!("{}}}\n", indent));

    // Write nested message types
    for nested in descriptor.child_messages() {
        output.push('\n');
        format_message_as_proto(&nested, output, indent_level);
    }
}

/// Format a field's type declaration
fn format_field_type(field: &prost_reflect::FieldDescriptor) -> String {
    use prost_reflect::Kind;

    if field.is_map() {
        // Map fields: map<key_type, value_type> field_name
        if let Kind::Message(map_entry) = field.kind() {
            let key_field = map_entry.fields().find(|f| f.name() == "key").unwrap();
            let value_field = map_entry.fields().find(|f| f.name() == "value").unwrap();
            let key_type = format_scalar_type(&key_field);
            let value_type = format_scalar_type(&value_field);
            return format!("map<{}, {}> ", key_type, value_type);
        }
    }

    let base_type = match field.kind() {
        Kind::Message(msg) => msg.name().to_string(),
        kind => format_kind_type(&kind),
    };

    if field.is_list() {
        format!("repeated {} ", base_type)
    } else {
        format!("{} ", base_type)
    }
}

/// Format a scalar field type (for map keys/values)
fn format_scalar_type(field: &prost_reflect::FieldDescriptor) -> String {
    match field.kind() {
        prost_reflect::Kind::Message(msg) => msg.name().to_string(),
        kind => format_kind_type(&kind),
    }
}

/// Map Kind enum to proto type string
fn format_kind_type(kind: &prost_reflect::Kind) -> String {
    use prost_reflect::Kind;
    match kind {
        Kind::Double => "double".into(),
        Kind::Float => "float".into(),
        Kind::Int32 => "int32".into(),
        Kind::Int64 => "int64".into(),
        Kind::Uint32 => "uint32".into(),
        Kind::Uint64 => "uint64".into(),
        Kind::Sint32 => "sint32".into(),
        Kind::Sint64 => "sint64".into(),
        Kind::Fixed32 => "fixed32".into(),
        Kind::Fixed64 => "fixed64".into(),
        Kind::Sfixed32 => "sfixed32".into(),
        Kind::Sfixed64 => "sfixed64".into(),
        Kind::Bool => "bool".into(),
        Kind::String => "string".into(),
        Kind::Bytes => "bytes".into(),
        Kind::Message(msg) => msg.name().to_string(),
        Kind::Enum(e) => e.name().to_string(),
    }
}

/// Generate protobuf descriptor from Unity Catalog table schema
pub fn generate_descriptor_from_schema(
    schema: &UnityCatalogTableSchema,
) -> Result<prost_reflect::MessageDescriptor, ZerobusSinkError> {
    let mut proto_fields = Vec::new();
    let mut collector = MessageCollector::new();

    // Sort columns by position to maintain stable field numbers
    let mut sorted_columns: Vec<&UnityCatalogColumn> = schema.columns.iter().collect();
    sorted_columns.sort_by_key(|c| c.position);

    for column in sorted_columns {
        // Skip columns with invalid positions (position should be >= 0)
        if column.position < 0 {
            continue;
        }

        // Try to parse complex types from type_json
        let (field_type, type_name, is_repeated) = if column.type_name == "STRUCT"
            || column.type_name == "ARRAY"
            || column.type_name == "MAP"
        {
            // Parse type_json for complex types - STRICT MODE: fail on parse errors
            let complex_type =
                parse_type_json(&column.type_json).map_err(|e| ZerobusSinkError::ConfigError {
                    message: format!(
                        "Failed to parse complex type for column '{}': {}. \
                         Vector requires all types to be supported. \
                         Options: 1) Update Vector to latest version, \
                                 2) Use explicit .proto schema file",
                        column.name, e
                    ),
                })?;

            let is_repeated = matches!(
                complex_type,
                ComplexType::Array(_) | ComplexType::Map { .. }
            );
            let path_prefix = column.name.clone();
            let (field_type, type_name) =
                map_complex_type_to_protobuf(&complex_type, &path_prefix, &mut collector)?;
            (field_type, type_name, is_repeated)
        } else {
            // Simple types
            let field_type = map_simple_databricks_type(&column.type_name)?;
            (field_type, None, false)
        };

        // Determine label based on type
        let label = if is_repeated {
            // ARRAYs and MAPs are represented as repeated fields
            prost_types::field_descriptor_proto::Label::Repeated as i32
        } else if column.nullable {
            prost_types::field_descriptor_proto::Label::Optional as i32
        } else {
            prost_types::field_descriptor_proto::Label::Required as i32
        };

        proto_fields.push(prost_types::FieldDescriptorProto {
            name: Some(column.name.clone()),
            number: Some(column.position + 1), // Protobuf field numbers start at 1, not 0
            label: Some(label),
            r#type: Some(field_type as i32),
            type_name,
            extendee: None,
            default_value: None,
            oneof_index: None,
            json_name: Some(column.name.clone()),
            options: None,
            proto3_optional: Some(column.nullable && !is_repeated),
        });
    }

    // Create the message descriptor
    let message_name = format!("{}_{}", schema.schema_name, schema.name);
    let message_proto = prost_types::DescriptorProto {
        name: Some(message_name.clone()),
        field: proto_fields,
        extension: vec![],
        nested_type: collector.nested_messages,
        enum_type: vec![],
        extension_range: vec![],
        oneof_decl: vec![],
        options: None,
        reserved_range: vec![],
        reserved_name: vec![],
    };

    let file_proto = prost_types::FileDescriptorProto {
        name: Some(format!("{}.proto", message_name)),
        package: Some(schema.catalog_name.clone()),
        message_type: vec![message_proto],
        ..Default::default()
    };

    let file_descriptor_set = prost_types::FileDescriptorSet {
        file: vec![file_proto],
    };

    // Build a FileDescriptor
    let pool = prost_reflect::DescriptorPool::from_file_descriptor_set(file_descriptor_set)
        .map_err(|e| ZerobusSinkError::ConfigError {
            message: format!("Failed to build descriptor pool: {}", e),
        })?;

    let full_message_name = format!("{}.{}", schema.catalog_name, message_name);
    let message_descriptor = pool
        .get_message_by_name(&full_message_name)
        .ok_or_else(|| ZerobusSinkError::ConfigError {
            message: format!("Failed to get message descriptor for {}", full_message_name),
        })?;

    // Log the inferred protobuf schema (only format when info logging is enabled)
    if tracing::enabled!(tracing::Level::INFO) {
        let proto_schema = format_descriptor_as_proto(&message_descriptor);
        info!(
            "Inferred protobuf schema from Unity Catalog table {}.{}.{}:\n{}",
            schema.catalog_name, schema.schema_name, schema.name, proto_schema
        );
    }

    Ok(message_descriptor)
}

/// Map simple Databricks type name to protobuf type
fn map_simple_databricks_type(
    type_name: &str,
) -> Result<prost_types::field_descriptor_proto::Type, ZerobusSinkError> {
    match type_name {
        "STRING" => Ok(prost_types::field_descriptor_proto::Type::String),
        "INT" => Ok(prost_types::field_descriptor_proto::Type::Int32),
        "LONG" | "BIGINT" => Ok(prost_types::field_descriptor_proto::Type::Int64),
        "BOOLEAN" | "BOOL" => Ok(prost_types::field_descriptor_proto::Type::Bool),
        "DOUBLE" => Ok(prost_types::field_descriptor_proto::Type::Double),
        "FLOAT" => Ok(prost_types::field_descriptor_proto::Type::Float),
        "TIMESTAMP" => Ok(prost_types::field_descriptor_proto::Type::Int64), // Unix timestamp in microseconds
        "DATE" => Ok(prost_types::field_descriptor_proto::Type::String),
        "BINARY" => Ok(prost_types::field_descriptor_proto::Type::Bytes),
        "DECIMAL" => Ok(prost_types::field_descriptor_proto::Type::String),

        unknown => Err(ZerobusSinkError::ConfigError {
            message: format!("Unsupported Databricks type: {}", unknown),
        }),
    }
}

/// Map primitive type to protobuf type
const fn map_primitive_to_protobuf(
    primitive: &PrimitiveType,
) -> prost_types::field_descriptor_proto::Type {
    match primitive {
        PrimitiveType::String => prost_types::field_descriptor_proto::Type::String,
        PrimitiveType::Long => prost_types::field_descriptor_proto::Type::Int64,
        PrimitiveType::Integer => prost_types::field_descriptor_proto::Type::Int32,
        PrimitiveType::Short => prost_types::field_descriptor_proto::Type::Int32,
        PrimitiveType::Byte => prost_types::field_descriptor_proto::Type::Int32,
        PrimitiveType::Double => prost_types::field_descriptor_proto::Type::Double,
        PrimitiveType::Float => prost_types::field_descriptor_proto::Type::Float,
        PrimitiveType::Boolean => prost_types::field_descriptor_proto::Type::Bool,
        PrimitiveType::Binary => prost_types::field_descriptor_proto::Type::Bytes,
        PrimitiveType::Timestamp => prost_types::field_descriptor_proto::Type::String,
        PrimitiveType::Date => prost_types::field_descriptor_proto::Type::String,
        PrimitiveType::Decimal { .. } => prost_types::field_descriptor_proto::Type::String,
    }
}

/// Map complex type to protobuf, generating nested messages as needed
/// Returns (field_type, optional_type_name)
fn map_complex_type_to_protobuf(
    complex_type: &ComplexType,
    path_prefix: &str,
    collector: &mut MessageCollector,
) -> Result<(prost_types::field_descriptor_proto::Type, Option<String>), ZerobusSinkError> {
    match complex_type {
        ComplexType::Primitive(primitive) => {
            let proto_type = map_primitive_to_protobuf(primitive);
            Ok((proto_type, None))
        }

        ComplexType::Struct(struct_type) => {
            // Generate a nested message for this struct
            let message_name = sanitize_message_name(path_prefix);
            let message_proto = generate_struct_message(&message_name, struct_type, collector)?;
            collector.add_message(message_proto);

            // Return MESSAGE type with the type name
            Ok((
                prost_types::field_descriptor_proto::Type::Message,
                Some(message_name),
            ))
        }

        ComplexType::Array(element_type) => {
            // Arrays become repeated fields
            // The element type determines the field type
            match element_type.as_ref() {
                ComplexType::Primitive(primitive) => {
                    let proto_type = map_primitive_to_protobuf(primitive);
                    Ok((proto_type, None))
                }
                ComplexType::Struct(_) => {
                    // Array of structs - need to generate the struct message
                    let element_message_name =
                        format!("{}_element", sanitize_message_name(path_prefix));
                    let (_, type_name) = map_complex_type_to_protobuf(
                        element_type,
                        &element_message_name,
                        collector,
                    )?;

                    Ok((
                        prost_types::field_descriptor_proto::Type::Message,
                        type_name,
                    ))
                }
                ComplexType::Array(_) => {
                    // Nested arrays not supported by Protobuf directly
                    Err(ZerobusSinkError::ConfigError {
                        message: format!("Nested arrays not supported for field: {}", path_prefix),
                    })
                }
                ComplexType::Map { .. } => {
                    // Array of maps - not directly supported
                    Err(ZerobusSinkError::ConfigError {
                        message: format!("Array of maps not supported for field: {}", path_prefix),
                    })
                }
            }
        }

        ComplexType::Map {
            key_type,
            value_type,
        } => {
            // Protobuf maps are represented as:
            // message MapFieldEntry { K key = 1; V value = 2; }
            // repeated MapFieldEntry map_field = N;

            // Protobuf maps support any scalar primitive key (int32, int64, bool, string, etc.)
            // but not complex types (struct, array, nested map).
            let key_primitive = match key_type.as_ref() {
                ComplexType::Primitive(p) => p,
                _ => {
                    return Err(ZerobusSinkError::ConfigError {
                        message: format!(
                            "MAP with non-scalar keys not supported for field '{}'. \
                             Protobuf maps require scalar (primitive) keys. Found key type: {:?}",
                            path_prefix, key_type
                        ),
                    });
                }
            };

            // Check if value is a primitive type
            match value_type.as_ref() {
                ComplexType::Primitive(value_primitive) => {
                    // Generate a map entry message for this field
                    let entry_message_name =
                        format!("{}_entry", sanitize_message_name(path_prefix));
                    let entry_message = generate_map_entry_message(
                        &entry_message_name,
                        key_primitive,
                        value_primitive,
                    )?;

                    collector.add_message(entry_message);

                    // Return repeated message type
                    Ok((
                        prost_types::field_descriptor_proto::Type::Message,
                        Some(entry_message_name),
                    ))
                }
                ComplexType::Struct(struct_type) => {
                    // Map with struct values: generate the value struct message, then a
                    // map-entry message that references it.
                    let value_message_name = format!("{}Value", sanitize_message_name(path_prefix));
                    let value_message =
                        generate_struct_message(&value_message_name, struct_type, collector)?;
                    collector.add_message(value_message);

                    let entry_message_name = format!("{}Entry", sanitize_message_name(path_prefix));
                    let entry_message = generate_map_entry_message_with_message_value(
                        &entry_message_name,
                        key_primitive,
                        &value_message_name,
                    )?;
                    collector.add_message(entry_message);

                    Ok((
                        prost_types::field_descriptor_proto::Type::Message,
                        Some(entry_message_name),
                    ))
                }
                ComplexType::Array(_) | ComplexType::Map { .. } => {
                    // Map with complex values
                    Err(ZerobusSinkError::ConfigError {
                        message: format!(
                            "MAP with complex values (ARRAY/MAP) not supported for field '{}'. \
                             Protobuf maps require simple value types.",
                            path_prefix
                        ),
                    })
                }
            }
        }
    }
}

/// Generate a protobuf message definition from a StructType
fn generate_struct_message(
    message_name: &str,
    struct_type: &StructType,
    collector: &mut MessageCollector,
) -> Result<prost_types::DescriptorProto, ZerobusSinkError> {
    let mut fields = Vec::new();

    for (index, field) in struct_type.fields.iter().enumerate() {
        // Field number starts at 1
        let field_number = (index + 1) as i32;

        // Recursively map the field type
        let path = format!("{}_{}", message_name, field.name);
        let (field_type, type_name) =
            map_complex_type_to_protobuf(&field.field_type, &path, collector)?;

        // Determine if this is a repeated field (for arrays)
        let (label, is_repeated) = if matches!(field.field_type, ComplexType::Array(_)) {
            (
                prost_types::field_descriptor_proto::Label::Repeated as i32,
                true,
            )
        } else if field.nullable {
            (
                prost_types::field_descriptor_proto::Label::Optional as i32,
                false,
            )
        } else {
            (
                prost_types::field_descriptor_proto::Label::Required as i32,
                false,
            )
        };

        fields.push(prost_types::FieldDescriptorProto {
            name: Some(field.name.clone()),
            number: Some(field_number),
            label: Some(label),
            r#type: Some(field_type as i32),
            type_name,
            extendee: None,
            default_value: None,
            oneof_index: None,
            json_name: Some(field.name.clone()),
            options: None,
            proto3_optional: Some(field.nullable && !is_repeated),
        });
    }

    Ok(prost_types::DescriptorProto {
        name: Some(message_name.to_string()),
        field: fields,
        extension: vec![],
        nested_type: vec![],
        enum_type: vec![],
        extension_range: vec![],
        oneof_decl: vec![],
        options: None,
        reserved_range: vec![],
        reserved_name: vec![],
    })
}

/// Generate a map entry message for protobuf map representation
/// Maps in protobuf are represented as: repeated MapEntry where MapEntry { key, value }
fn generate_map_entry_message(
    message_name: &str,
    key_type: &PrimitiveType,
    value_type: &PrimitiveType,
) -> Result<prost_types::DescriptorProto, ZerobusSinkError> {
    let key_proto_type = map_primitive_to_protobuf(key_type);
    let value_proto_type = map_primitive_to_protobuf(value_type);

    let fields = vec![
        // key field — any scalar primitive type supported by protobuf maps
        prost_types::FieldDescriptorProto {
            name: Some("key".to_string()),
            number: Some(1),
            label: Some(prost_types::field_descriptor_proto::Label::Optional as i32),
            r#type: Some(key_proto_type as i32),
            type_name: None,
            extendee: None,
            default_value: None,
            oneof_index: None,
            json_name: Some("key".to_string()),
            options: None,
            proto3_optional: Some(false),
        },
        // value field
        prost_types::FieldDescriptorProto {
            name: Some("value".to_string()),
            number: Some(2),
            label: Some(prost_types::field_descriptor_proto::Label::Optional as i32),
            r#type: Some(value_proto_type as i32),
            type_name: None,
            extendee: None,
            default_value: None,
            oneof_index: None,
            json_name: Some("value".to_string()),
            options: None,
            proto3_optional: Some(true),
        },
    ];

    Ok(prost_types::DescriptorProto {
        name: Some(message_name.to_string()),
        field: fields,
        extension: vec![],
        nested_type: vec![],
        enum_type: vec![],
        extension_range: vec![],
        oneof_decl: vec![],
        options: Some(prost_types::MessageOptions {
            map_entry: Some(true), // Mark this as a map entry
            ..Default::default()
        }),
        reserved_range: vec![],
        reserved_name: vec![],
    })
}

/// Generate a map entry message where the value is a message (struct) type
fn generate_map_entry_message_with_message_value(
    message_name: &str,
    key_type: &PrimitiveType,
    value_type_name: &str,
) -> Result<prost_types::DescriptorProto, ZerobusSinkError> {
    let key_proto_type = map_primitive_to_protobuf(key_type);

    let fields = vec![
        prost_types::FieldDescriptorProto {
            name: Some("key".to_string()),
            number: Some(1),
            label: Some(prost_types::field_descriptor_proto::Label::Optional as i32),
            r#type: Some(key_proto_type as i32),
            type_name: None,
            extendee: None,
            default_value: None,
            oneof_index: None,
            json_name: Some("key".to_string()),
            options: None,
            proto3_optional: Some(false),
        },
        prost_types::FieldDescriptorProto {
            name: Some("value".to_string()),
            number: Some(2),
            label: Some(prost_types::field_descriptor_proto::Label::Optional as i32),
            r#type: Some(prost_types::field_descriptor_proto::Type::Message as i32),
            type_name: Some(value_type_name.to_string()),
            extendee: None,
            default_value: None,
            oneof_index: None,
            json_name: Some("value".to_string()),
            options: None,
            proto3_optional: Some(true),
        },
    ];

    Ok(prost_types::DescriptorProto {
        name: Some(message_name.to_string()),
        field: fields,
        extension: vec![],
        nested_type: vec![],
        enum_type: vec![],
        extension_range: vec![],
        oneof_decl: vec![],
        options: Some(prost_types::MessageOptions {
            map_entry: Some(true),
            ..Default::default()
        }),
        reserved_range: vec![],
        reserved_name: vec![],
    })
}

// The function converts Unity Catalog field names into valid protobuf message type names:
// When generating protobuf descriptors from Unity Catalog schemas, nested structures (structs, arrays of structs, maps)
// need to become protobuf message types. Protobuf message names must:
// 1. Start with a letter (not _ or digit)
// 2. Be alphanumeric (no special characters)
// 3. Follow PascalCase convention
fn sanitize_message_name(name: &str) -> String {
    // Convert to PascalCase and remove invalid characters
    let mut result = String::new();
    let mut capitalize_next = true;

    for c in name.chars() {
        if c.is_alphanumeric() {
            if capitalize_next {
                result.push(c.to_ascii_uppercase());
                capitalize_next = false;
            } else {
                result.push(c);
            }
        } else {
            capitalize_next = true;
        }
    }

    // Ensure it starts with a letter
    if result.is_empty() || !result.chars().next().unwrap().is_alphabetic() {
        result.insert(0, 'M');
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_simple_types() {
        let test_cases = vec![
            ("STRING", prost_types::field_descriptor_proto::Type::String),
            ("INT", prost_types::field_descriptor_proto::Type::Int32),
            ("BIGINT", prost_types::field_descriptor_proto::Type::Int64),
            ("BOOLEAN", prost_types::field_descriptor_proto::Type::Bool),
            ("DOUBLE", prost_types::field_descriptor_proto::Type::Double),
            (
                "TIMESTAMP",
                prost_types::field_descriptor_proto::Type::Int64, // Unix timestamp in microseconds
            ),
            ("BINARY", prost_types::field_descriptor_proto::Type::Bytes),
        ];

        for (databricks_type, expected_proto_type) in test_cases {
            let result = map_simple_databricks_type(databricks_type);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), expected_proto_type);
        }
    }

    #[test]
    fn test_generate_descriptor_simple_schema() {
        let schema = UnityCatalogTableSchema {
            name: "test_table".to_string(),
            catalog_name: "test_catalog".to_string(),
            schema_name: "test_schema".to_string(),
            columns: vec![
                UnityCatalogColumn {
                    name: "id".to_string(),
                    type_text: "bigint".to_string(),
                    type_name: "BIGINT".to_string(),
                    position: 1,
                    nullable: false,
                    type_json: "{}".to_string(),
                },
                UnityCatalogColumn {
                    name: "message".to_string(),
                    type_text: "string".to_string(),
                    type_name: "STRING".to_string(),
                    position: 2,
                    nullable: true,
                    type_json: "{}".to_string(),
                },
            ],
        };

        let result = generate_descriptor_from_schema(&schema);
        assert!(result.is_ok());

        let descriptor = result.unwrap();
        assert_eq!(descriptor.fields().len(), 2);

        let id_field = descriptor.get_field_by_name("id");
        assert!(id_field.is_some());

        let message_field = descriptor.get_field_by_name("message");
        assert!(message_field.is_some());
    }

    #[test]
    fn test_parse_struct_type_json() {
        let type_json = r#"{
            "type": "struct",
            "fields": [
                {
                    "name": "job_id",
                    "type": "long",
                    "nullable": true
                },
                {
                    "name": "task_run_id",
                    "type": "long",
                    "nullable": true
                }
            ]
        }"#;

        let result = parse_type_json(type_json);
        assert!(result.is_ok());

        match result.unwrap() {
            ComplexType::Struct(struct_type) => {
                assert_eq!(struct_type.fields.len(), 2);
                assert_eq!(struct_type.fields[0].name, "job_id");
                assert_eq!(struct_type.fields[1].name, "task_run_id");
            }
            _ => panic!("Expected struct type"),
        }
    }

    #[test]
    fn test_parse_array_type_json() {
        let type_json = r#"{
            "type": "array",
            "elementType": "string"
        }"#;

        let result = parse_type_json(type_json);
        assert!(result.is_ok());

        match result.unwrap() {
            ComplexType::Array(element_type) => match element_type.as_ref() {
                ComplexType::Primitive(PrimitiveType::String) => {}
                _ => panic!("Expected string element type"),
            },
            _ => panic!("Expected array type"),
        }
    }

    #[test]
    fn test_parse_map_type_json() {
        let type_json = r#"{
            "type": "map",
            "keyType": "string",
            "valueType": "string"
        }"#;

        let result = parse_type_json(type_json);
        assert!(result.is_ok());

        match result.unwrap() {
            ComplexType::Map {
                key_type,
                value_type,
            } => {
                assert!(matches!(
                    key_type.as_ref(),
                    ComplexType::Primitive(PrimitiveType::String)
                ));
                assert!(matches!(
                    value_type.as_ref(),
                    ComplexType::Primitive(PrimitiveType::String)
                ));
            }
            _ => panic!("Expected map type"),
        }
    }

    #[test]
    fn test_generate_descriptor_with_map() {
        let schema = UnityCatalogTableSchema {
            name: "test_table".to_string(),
            catalog_name: "test_catalog".to_string(),
            schema_name: "test_schema".to_string(),
            columns: vec![
                UnityCatalogColumn {
                    name: "id".to_string(),
                    type_text: "bigint".to_string(),
                    type_name: "BIGINT".to_string(),
                    position: 1,
                    nullable: false,
                    type_json: "{}".to_string(),
                },
                UnityCatalogColumn {
                    name: "attributes".to_string(),
                    type_text: "map<string,string>".to_string(),
                    type_name: "MAP".to_string(),
                    position: 2,
                    nullable: true,
                    type_json: r#"{"type":"map","keyType":"string","valueType":"string"}"#
                        .to_string(),
                },
            ],
        };

        let result = generate_descriptor_from_schema(&schema);
        assert!(
            result.is_ok(),
            "Failed to generate descriptor with MAP type: {:?}",
            result.err()
        );

        let descriptor = result.unwrap();
        assert_eq!(descriptor.fields().len(), 2);

        let id_field = descriptor.get_field_by_name("id");
        assert!(id_field.is_some());

        let attributes_field = descriptor.get_field_by_name("attributes");
        assert!(attributes_field.is_some());
    }

    #[test]
    fn test_strict_validation_fails_on_unsupported() {
        let schema = UnityCatalogTableSchema {
            name: "test_table".to_string(),
            catalog_name: "test_catalog".to_string(),
            schema_name: "test_schema".to_string(),
            columns: vec![UnityCatalogColumn {
                name: "unsupported_col".to_string(),
                type_text: "struct<...>".to_string(),
                type_name: "STRUCT".to_string(),
                position: 1,
                nullable: true,
                type_json: "invalid json".to_string(), // Malformed type_json
            }],
        };

        let result = generate_descriptor_from_schema(&schema);
        assert!(result.is_err(), "Expected error for malformed type_json");
    }

    // ========== Fixture-based Tests ==========
    // These tests use real Unity Catalog schema responses saved as JSON fixtures

    #[test]
    fn test_fixture_simple_table() {
        let json = include_str!("tests/fixtures/simple_table_schema.json");
        let schema: UnityCatalogTableSchema =
            serde_json::from_str(json).expect("Failed to parse simple_table fixture");

        let result = generate_descriptor_from_schema(&schema);
        assert!(
            result.is_ok(),
            "Failed to generate descriptor: {:?}",
            result.err()
        );

        let descriptor = result.unwrap();
        assert_eq!(descriptor.fields().len(), 4);

        // Verify all fields exist
        assert!(descriptor.get_field_by_name("id").is_some());
        assert!(descriptor.get_field_by_name("name").is_some());
        assert!(descriptor.get_field_by_name("created_at").is_some());
        assert!(descriptor.get_field_by_name("is_active").is_some());
    }

    #[test]
    fn test_fixture_all_primitive_types() {
        let json = include_str!("tests/fixtures/all_primitive_types_schema.json");
        let schema: UnityCatalogTableSchema =
            serde_json::from_str(json).expect("Failed to parse all_primitive_types fixture");

        let result = generate_descriptor_from_schema(&schema);
        assert!(
            result.is_ok(),
            "Failed to generate descriptor: {:?}",
            result.err()
        );

        let descriptor = result.unwrap();
        assert_eq!(
            descriptor.fields().len(),
            9,
            "Should have 9 primitive type columns"
        );

        // Verify specific types
        assert!(descriptor.get_field_by_name("col_string").is_some());
        assert!(descriptor.get_field_by_name("col_int").is_some());
        assert!(descriptor.get_field_by_name("col_long").is_some());
        assert!(descriptor.get_field_by_name("col_double").is_some());
        assert!(descriptor.get_field_by_name("col_float").is_some());
        assert!(descriptor.get_field_by_name("col_boolean").is_some());
        assert!(descriptor.get_field_by_name("col_binary").is_some());
        assert!(descriptor.get_field_by_name("col_timestamp").is_some());
        assert!(descriptor.get_field_by_name("col_date").is_some());
    }

    #[test]
    fn test_fixture_nested_struct() {
        let json = include_str!("tests/fixtures/nested_struct_schema.json");
        let schema: UnityCatalogTableSchema =
            serde_json::from_str(json).expect("Failed to parse nested_struct fixture");

        let result = generate_descriptor_from_schema(&schema);
        assert!(
            result.is_ok(),
            "Failed to generate descriptor for nested struct: {:?}",
            result.err()
        );

        let descriptor = result.unwrap();
        assert_eq!(descriptor.fields().len(), 1);

        // Verify nested struct field exists
        let user_info_field = descriptor.get_field_by_name("user_info");
        assert!(user_info_field.is_some(), "user_info field should exist");
    }

    #[test]
    fn test_fixture_array_of_structs() {
        let json = include_str!("tests/fixtures/array_of_structs_schema.json");
        let schema: UnityCatalogTableSchema =
            serde_json::from_str(json).expect("Failed to parse array_of_structs fixture");

        let result = generate_descriptor_from_schema(&schema);
        assert!(
            result.is_ok(),
            "Failed to generate descriptor for array of structs: {:?}",
            result.err()
        );

        let descriptor = result.unwrap();
        assert_eq!(descriptor.fields().len(), 1);

        // Verify array field exists
        let transactions_field = descriptor.get_field_by_name("transactions");
        assert!(
            transactions_field.is_some(),
            "transactions field should exist"
        );
    }

    #[test]
    fn test_fixture_mixed_types() {
        // Tests a schema with 5 columns covering: timestamp, bigint, nested struct,
        // array<bigint>, and map<string,string>.
        let json = include_str!("tests/fixtures/mixed_types_schema.json");
        let schema: UnityCatalogTableSchema =
            serde_json::from_str(json).expect("Failed to parse mixed_types fixture");

        let result = generate_descriptor_from_schema(&schema);
        assert!(
            result.is_ok(),
            "Failed to generate descriptor for mixed_types: {:?}",
            result.err()
        );

        let descriptor = result.unwrap();
        assert_eq!(descriptor.fields().len(), 5, "Should have 5 columns");

        // Verify all fields exist: timestamp, bigint, struct, array, map
        assert!(
            descriptor.get_field_by_name("field_001").is_some(),
            "Should have field_001 (timestamp)"
        );
        assert!(
            descriptor.get_field_by_name("field_002").is_some(),
            "Should have field_002 (bigint)"
        );
        assert!(
            descriptor.get_field_by_name("field_003").is_some(),
            "Should have field_003 (STRUCT)"
        );
        assert!(
            descriptor.get_field_by_name("field_007").is_some(),
            "Should have field_007 (ARRAY)"
        );
        assert!(
            descriptor.get_field_by_name("field_008").is_some(),
            "Should have field_008 (MAP)"
        );
    }

    #[test]
    fn test_fixture_mixed_types_struct_parsing() {
        // Test that the nested STRUCT column is properly parsed
        let json = include_str!("tests/fixtures/mixed_types_schema.json");
        let schema: UnityCatalogTableSchema =
            serde_json::from_str(json).expect("Failed to parse mixed_types fixture");

        // Find the struct column (field_003)
        let struct_col = schema
            .columns
            .iter()
            .find(|c| c.name == "field_003")
            .expect("Should have field_003 column");

        // Parse its type_json
        let result = parse_type_json(&struct_col.type_json);
        assert!(result.is_ok(), "Should parse field_003 type_json");

        match result.unwrap() {
            ComplexType::Struct(struct_type) => {
                assert_eq!(struct_type.fields.len(), 1, "Should have 1 field");
                assert_eq!(struct_type.fields[0].name, "field_004");

                // Verify nested struct
                match &struct_type.fields[0].field_type {
                    ComplexType::Struct(nested) => {
                        assert_eq!(nested.fields.len(), 2, "Nested struct should have 2 fields");
                        assert_eq!(nested.fields[0].name, "field_005");
                        assert_eq!(nested.fields[1].name, "field_006");
                    }
                    _ => panic!("Expected nested struct"),
                }
            }
            _ => panic!("Expected struct type for field_003"),
        }
    }

    #[test]
    fn test_fixture_error_handling_empty_type_json() {
        // Test that empty type_json is handled correctly
        let result = parse_type_json("");
        assert!(result.is_err(), "Should fail on empty type_json");

        let result = parse_type_json("{}");
        assert!(result.is_err(), "Should fail on empty object type_json");
    }

    #[test]
    fn test_fixture_error_handling_invalid_json() {
        // Test that invalid JSON is handled correctly
        let result = parse_type_json("not valid json");
        assert!(result.is_err(), "Should fail on invalid JSON");
    }

    #[test]
    fn test_fixture_error_handling_missing_type_field() {
        // Test that missing 'type' field is handled
        let result = parse_type_json(r#"{"fields": []}"#);
        assert!(result.is_err(), "Should fail when 'type' field is missing");
    }

    // ========== Comprehensive Proto Compatibility Test ==========

    /// Helper function to assert a field exists with expected type
    fn assert_field_exists_with_type(
        descriptor: &prost_reflect::MessageDescriptor,
        field_name: &str,
        expected_type: &str,
    ) {
        let field = descriptor
            .get_field_by_name(field_name)
            .unwrap_or_else(|| panic!("Field '{}' should exist", field_name));

        let actual_type = format_field_type_simple(&field);
        assert_eq!(
            actual_type, expected_type,
            "Field '{}' should have type '{}'",
            field_name, expected_type
        );
    }

    /// Helper function to assert a field is a message type with expected name
    fn assert_field_is_message(
        descriptor: &prost_reflect::MessageDescriptor,
        field_name: &str,
        message_type: &str,
    ) {
        let field = descriptor
            .get_field_by_name(field_name)
            .unwrap_or_else(|| panic!("Field '{}' should exist", field_name));

        match field.kind() {
            prost_reflect::Kind::Message(msg) => {
                assert_eq!(
                    msg.name(),
                    message_type,
                    "Field '{}' should be message type '{}'",
                    field_name,
                    message_type
                );
            }
            _ => panic!("Field '{}' should be a message type", field_name),
        }
    }

    /// Helper function to assert a field is repeated (array)
    fn assert_field_is_repeated(descriptor: &prost_reflect::MessageDescriptor, field_name: &str) {
        let field = descriptor
            .get_field_by_name(field_name)
            .unwrap_or_else(|| panic!("Field '{}' should exist", field_name));

        assert!(
            field.is_list(),
            "Field '{}' should be repeated/list",
            field_name
        );
    }

    /// Helper function to format field type as simple string
    fn format_field_type_simple(field: &prost_reflect::FieldDescriptor) -> String {
        use prost_reflect::Kind;
        match field.kind() {
            Kind::String => "string".to_string(),
            Kind::Int64 => "int64".to_string(),
            Kind::Int32 => "int32".to_string(),
            Kind::Bool => "bool".to_string(),
            Kind::Double => "double".to_string(),
            Kind::Float => "float".to_string(),
            Kind::Bytes => "bytes".to_string(),
            Kind::Message(msg) => msg.name().to_string(),
            _ => "unknown".to_string(),
        }
    }

    #[test]
    fn test_nested_structs_complete_schema() {
        // Verifies that a 15-column schema with nested structs, arrays, and
        // various primitive types generates a correct protobuf descriptor.

        let json = include_str!("tests/fixtures/nested_structs_complete_schema.json");
        let schema: UnityCatalogTableSchema =
            serde_json::from_str(json).expect("Failed to parse nested_structs_complete schema");

        let descriptor = generate_descriptor_from_schema(&schema)
            .expect("Failed to generate descriptor from complete schema");

        // === 1. VERIFY MAIN MESSAGE STRUCTURE ===

        assert_eq!(
            descriptor.name(),
            "test_schema_nested_structs_table",
            "Main message should have expected name"
        );

        assert_eq!(
            descriptor.fields().len(),
            15,
            "Should have exactly 15 fields"
        );

        // === 2. VERIFY PRIMITIVE FIELDS ===

        assert_field_exists_with_type(&descriptor, "field_001", "int64"); // bigint
        assert_field_exists_with_type(&descriptor, "field_002", "string");
        assert_field_exists_with_type(&descriptor, "field_003", "string");
        assert_field_exists_with_type(&descriptor, "field_004", "string");
        assert_field_exists_with_type(&descriptor, "field_005", "string");
        assert_field_exists_with_type(&descriptor, "field_006", "int64"); // bigint
        assert_field_exists_with_type(&descriptor, "field_007", "int64"); // bigint
        assert_field_exists_with_type(&descriptor, "field_028", "bool");
        assert_field_exists_with_type(&descriptor, "field_029", "string");
        assert_field_exists_with_type(&descriptor, "field_030", "string");
        assert_field_exists_with_type(&descriptor, "field_031", "bool");

        // === 3. VERIFY COMPLEX MESSAGE TYPES ===

        assert_field_is_message(&descriptor, "field_008", "Field008");
        assert_field_is_message(&descriptor, "field_018", "Field018");
        assert_field_is_message(&descriptor, "field_021", "Field021");

        // === 4. VERIFY ARRAY FIELD ===

        assert_field_is_repeated(&descriptor, "field_027");

        let array_field = descriptor
            .get_field_by_name("field_027")
            .expect("field_027 should exist");
        let element_type = format_field_type_simple(&array_field);
        assert_eq!(element_type, "int64", "field_027 should be repeated int64");

        // === 5. VERIFY NESTED MESSAGE: Field018 (2 string fields) ===

        let field_018 = descriptor
            .get_field_by_name("field_018")
            .expect("field_018 should exist");

        if let prost_reflect::Kind::Message(msg) = field_018.kind() {
            assert_eq!(msg.fields().len(), 2, "Field018 should have 2 fields");
            assert!(msg.get_field_by_name("field_019").is_some());
            assert!(msg.get_field_by_name("field_020").is_some());
        } else {
            panic!("field_018 should be a message type");
        }

        // === 6. VERIFY NESTED MESSAGE: Field021 (6 fields incl. int32) ===

        let field_021 = descriptor
            .get_field_by_name("field_021")
            .expect("field_021 should exist");

        if let prost_reflect::Kind::Message(msg) = field_021.kind() {
            assert_eq!(msg.fields().len(), 6, "Field021 should have 6 fields");

            let expected_fields = vec![
                "field_022",
                "field_023",
                "field_020",
                "field_024",
                "field_025",
                "field_026",
            ];
            for field_name in expected_fields {
                assert!(
                    msg.get_field_by_name(field_name).is_some(),
                    "Field021 should have {} field",
                    field_name
                );
            }

            // Verify field_026 is int32
            let f026 = msg
                .get_field_by_name("field_026")
                .expect("field_026 should exist");
            match f026.kind() {
                prost_reflect::Kind::Int32 => {}
                _ => panic!("field_026 should be int32"),
            }
        } else {
            panic!("field_021 should be a message type");
        }

        // === 7. VERIFY NESTED MESSAGE: Field008 (4 nested struct fields) ===

        let field_008 = descriptor
            .get_field_by_name("field_008")
            .expect("field_008 should exist");

        if let prost_reflect::Kind::Message(msg) = field_008.kind() {
            let expected_nested = vec!["field_009", "field_012", "field_014", "field_016"];
            for nested in expected_nested {
                assert!(
                    msg.get_field_by_name(nested).is_some(),
                    "Field008 should have {} field",
                    nested
                );
            }

            // Verify field_009 nested structure has 2 fields
            let f009 = msg
                .get_field_by_name("field_009")
                .expect("field_009 should exist");
            if let prost_reflect::Kind::Message(nested_msg) = f009.kind() {
                assert!(nested_msg.get_field_by_name("field_010").is_some());
                assert!(nested_msg.get_field_by_name("field_011").is_some());
            }
        } else {
            panic!("field_008 should be a message type");
        }
    }

    #[test]
    fn test_proto_schema_snapshot() {
        // Snapshot test: verify the generated proto text matches expected format
        let json = include_str!("tests/fixtures/nested_structs_complete_schema.json");
        let schema: UnityCatalogTableSchema =
            serde_json::from_str(json).expect("Failed to parse nested_structs_complete schema");

        let descriptor =
            generate_descriptor_from_schema(&schema).expect("Failed to generate descriptor");

        // Format as proto text
        let proto_text = format_descriptor_as_proto(&descriptor);

        // Verify key structures are present in the proto text
        assert!(
            proto_text.contains("message test_schema_nested_structs_table"),
            "Proto should have main message definition"
        );
        assert!(
            proto_text.contains("string field_003"),
            "Proto should have field_003 (string)"
        );
        assert!(
            proto_text.contains("int64 field_007"),
            "Proto should have field_007 (int64)"
        );
        assert!(
            proto_text.contains("repeated int64 field_027"),
            "Proto should have field_027 as repeated int64"
        );
        assert!(
            proto_text.contains("message Field018"),
            "Proto should have Field018 nested message"
        );
        assert!(
            proto_text.contains("message Field021"),
            "Proto should have Field021 nested message"
        );
        assert!(
            proto_text.contains("message Field008"),
            "Proto should have Field008 nested message"
        );
    }

    #[test]
    fn test_complex_schema_with_all_type_patterns() {
        // Regression test: verifies that a large 91-column schema exercising
        // all supported Unity Catalog type patterns generates a valid protobuf
        // descriptor without errors.
        //
        // Coverage:
        //   - map<bigint, bool>   (field_142) — non-string scalar key
        //   - map<bigint, double> (field_143) — non-string scalar key
        //   - map<string, STRUCT> (field_725) — MAP with struct value
        //   - ARRAY<STRUCT> with deeply nested fields
        //   - Deeply nested STRUCTs (4+ levels)

        let json = include_str!("tests/fixtures/complex_nested_types_schema.json");
        let schema: UnityCatalogTableSchema =
            serde_json::from_str(json).expect("Failed to parse schema fixture");

        assert_eq!(
            schema.columns.len(),
            91,
            "Fixture must contain all 91 columns"
        );

        let descriptor = generate_descriptor_from_schema(&schema)
            .expect("Should succeed: all column types must be supported");

        let proto_text = format_descriptor_as_proto(&descriptor);

        // --- non-string scalar map keys ---
        assert!(
            proto_text.contains("field_142"),
            "Should contain field_142 (map<int64, bool>)"
        );
        assert!(
            proto_text.contains("field_143"),
            "Should contain field_143 (map<int64, double>)"
        );

        // --- MAP with STRUCT value ---
        assert!(
            proto_text.contains("field_725"),
            "Should contain field_725 (map<string, STRUCT>)"
        );

        // --- key top-level fields ---
        assert!(
            descriptor.get_field_by_name("field_001").is_some(),
            "Should have field_001 (timestamp)"
        );
        assert!(
            descriptor.get_field_by_name("field_002").is_some(),
            "Should have field_002 (string)"
        );
        assert!(
            descriptor.get_field_by_name("field_007").is_some(),
            "Should have field_007 (deeply nested metadata struct)"
        );
        assert!(
            descriptor.get_field_by_name("field_117").is_some(),
            "Should have field_117 (struct with map fields)"
        );
        assert!(
            descriptor.get_field_by_name("field_721").is_some(),
            "Should have field_721 (struct containing MAP<string, STRUCT>)"
        );
        assert!(
            descriptor.get_field_by_name("field_149").is_some(),
            "Should have field_149 (ARRAY<complex STRUCT>)"
        );
        assert!(
            descriptor.get_field_by_name("field_278").is_some(),
            "Should have field_278 (deeply nested ARRAY<STRUCT>)"
        );

        // total field count must match all 91 columns
        assert_eq!(
            descriptor.fields().len(),
            91,
            "Descriptor should have exactly 91 fields"
        );
    }
}
