//! Unity Catalog schema fetching and protobuf descriptor generation.
//!
//! The UC-to-protobuf conversion is delegated to
//! [`databricks_zerobus_ingest_sdk::schema`]; this module only wraps it with
//! the HTTP fetching + descriptor-pool assembly that the sink needs.

use bytes::Buf;
use databricks_zerobus_ingest_sdk::schema::descriptor_from_uc_schema;
use http::{Request, Uri};
use http_body::Body as HttpBody;
use hyper::Body;
use percent_encoding::{NON_ALPHANUMERIC, percent_encode};
use prost_reflect::prost_types;
use serde::Deserialize;

use super::error::ZerobusSinkError;
use crate::config::ProxyConfig;
use crate::http::HttpClient;
use crate::tls::TlsSettings;

// Alias the SDK types under the names the rest of the sink already uses.
#[cfg(test)]
use databricks_zerobus_ingest_sdk::schema::UcColumn as UnityCatalogColumn;
pub use databricks_zerobus_ingest_sdk::schema::UcTableSchema as UnityCatalogTableSchema;

/// OAuth token response from Databricks
#[derive(Debug, Deserialize)]
struct OAuthTokenResponse {
    access_token: String,
}

/// Fetch table schema from Unity Catalog API
pub async fn fetch_table_schema(
    unity_catalog_endpoint: &str,
    table_name: &str,
    client_id: &str,
    client_secret: &str,
    proxy: &ProxyConfig,
) -> Result<UnityCatalogTableSchema, ZerobusSinkError> {
    let http_client = HttpClient::new(TlsSettings::default(), proxy).map_err(|e| {
        ZerobusSinkError::ConfigError {
            message: format!("Failed to create HTTP client: {}", e),
        }
    })?;

    // First, get OAuth token
    let token = get_oauth_token(
        &http_client,
        unity_catalog_endpoint,
        client_id,
        client_secret,
    )
    .await?;

    // Fetch table schema.
    // Encode each segment of the fully-qualified table name (catalog.schema.table)
    // so that reserved URI characters in quoted Unity Catalog identifiers (spaces,
    // #, /, etc.) don't break URI parsing or hit the wrong endpoint.
    let encoded_table_name: String = table_name
        .split('.')
        .map(|seg| percent_encode(seg.as_bytes(), NON_ALPHANUMERIC).to_string())
        .collect::<Vec<_>>()
        .join(".");
    let url = format!(
        "{}/api/2.1/unity-catalog/tables/{}",
        unity_catalog_endpoint.trim_end_matches('/'),
        encoded_table_name
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

/// Generate a protobuf message descriptor from a Unity Catalog table schema.
///
/// The core UC-type → protobuf conversion lives in
/// [`databricks_zerobus_ingest_sdk::schema::descriptor_from_uc_schema`]; this
/// wrapper adds the `FileDescriptorProto` / `DescriptorPool` plumbing that
/// Vector needs to get a `prost_reflect::MessageDescriptor` usable for
/// dynamic message encoding.
pub fn generate_descriptor_from_schema(
    schema: &UnityCatalogTableSchema,
) -> Result<prost_reflect::MessageDescriptor, ZerobusSinkError> {
    let message_proto =
        descriptor_from_uc_schema(schema).map_err(|e| ZerobusSinkError::ConfigError {
            message: format!("Failed to convert Unity Catalog schema to protobuf: {}", e),
        })?;

    let message_name = message_proto.name().to_string();
    let package_name = sanitize_package_name(&schema.catalog_name);

    let file_proto = prost_types::FileDescriptorProto {
        name: Some(format!("{}.proto", message_name)),
        package: Some(package_name.clone()),
        message_type: vec![message_proto],
        ..Default::default()
    };

    let file_descriptor_set = prost_types::FileDescriptorSet {
        file: vec![file_proto],
    };

    let pool = prost_reflect::DescriptorPool::from_file_descriptor_set(file_descriptor_set)
        .map_err(|e| ZerobusSinkError::ConfigError {
            message: format!("Failed to build descriptor pool: {}", e),
        })?;

    let full_message_name = format!("{}.{}", package_name, message_name);
    let message_descriptor = pool
        .get_message_by_name(&full_message_name)
        .ok_or_else(|| ZerobusSinkError::ConfigError {
            message: format!("Failed to get message descriptor for {}", full_message_name),
        })?;

    if tracing::enabled!(tracing::Level::INFO) {
        let proto_schema = format_descriptor_as_proto(&message_descriptor);
        info!(
            "Inferred protobuf schema from Unity Catalog table {}.{}.{}:\n{}",
            schema.catalog_name, schema.schema_name, schema.name, proto_schema
        );
    }

    Ok(message_descriptor)
}

/// Default prefix for package name segments that start with a non-letter.
const PACKAGE_SEGMENT_PREFIX: char = 'p';

/// Sanitize a string for use as a protobuf package name.
///
/// Package identifiers allow `[a-zA-Z][a-zA-Z0-9_]*` segments separated by `.`.
/// Invalid characters are replaced with `_` and each segment is ensured to start
/// with a letter.
fn sanitize_package_name(name: &str) -> String {
    name.split('.')
        .map(|segment| {
            let mut s: String = segment
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '_' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            if s.is_empty() || !s.starts_with(|c: char| c.is_ascii_alphabetic()) {
                s.insert(0, PACKAGE_SEGMENT_PREFIX);
            }
            s
        })
        .collect::<Vec<_>>()
        .join(".")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke test: the wrapper calls into the SDK and builds a usable
    /// `MessageDescriptor` via the descriptor pool.
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
                    name: "body".to_string(),
                    type_text: "string".to_string(),
                    type_name: "STRING".to_string(),
                    position: 2,
                    nullable: true,
                    type_json: "{}".to_string(),
                },
            ],
        };

        let descriptor =
            generate_descriptor_from_schema(&schema).expect("descriptor should be generated");
        assert_eq!(descriptor.fields().len(), 2);
        assert!(descriptor.get_field_by_name("id").is_some());
        assert!(descriptor.get_field_by_name("body").is_some());
    }

    /// Snapshot test for the proto-text formatter used in info logging.
    /// The UC→proto conversion itself is covered by the SDK's own tests;
    /// this guards the local `format_descriptor_as_proto` rendering.
    #[test]
    fn test_proto_schema_snapshot() {
        let json = include_str!("tests/fixtures/nested_structs_complete_schema.json");
        let schema: UnityCatalogTableSchema =
            serde_json::from_str(json).expect("Failed to parse nested_structs_complete schema");

        let descriptor =
            generate_descriptor_from_schema(&schema).expect("Failed to generate descriptor");

        let proto_text = format_descriptor_as_proto(&descriptor);

        assert!(
            proto_text.contains("message TestSchemaNestedStructsTable"),
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
    fn test_sanitize_package_name_non_ascii() {
        // Non-ASCII alphanumeric characters (e.g. accented letters, CJK) are
        // valid for `char::is_alphanumeric` but not for protobuf identifiers,
        // so they must be replaced with `_`.
        assert_eq!(sanitize_package_name("café"), "caf_");
        assert_eq!(sanitize_package_name("日本.tbl"), "p__.tbl");
        assert_eq!(sanitize_package_name("naïve.schema"), "na_ve.schema");
    }

    #[test]
    fn test_sanitize_package_name_ascii_preserved() {
        assert_eq!(sanitize_package_name("main.default_v2"), "main.default_v2");
        assert_eq!(sanitize_package_name("1abc"), "p1abc");
        assert_eq!(sanitize_package_name("_x"), "p_x");
    }
}
