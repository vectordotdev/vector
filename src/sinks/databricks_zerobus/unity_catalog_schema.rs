//! Unity Catalog schema fetching and Arrow schema generation.
//!
//! The UC-to-Arrow conversion is delegated to
//! [`databricks_zerobus_ingest_sdk::schema`]; this module only wraps it with
//! the HTTP fetching that the sink needs.

use bytes::Buf;
use databricks_zerobus_ingest_sdk::schema::arrow_schema_from_uc_schema;
use http::{Request, StatusCode, Uri};
use http_body::Body as HttpBody;
use hyper::Body;
use percent_encoding::{NON_ALPHANUMERIC, percent_encode};
use serde::Deserialize;

use super::error::ZerobusSinkError;
use crate::http::HttpClient;

/// Whether a Unity Catalog HTTP response status should be retried.
///
/// Delegates to the canonical Vector HTTP retry policy
/// [`crate::sinks::util::http::RetryStrategy::Default`] so this sink stays in
/// lock-step with other HTTP-based sinks: 5xx (except 501 Not Implemented),
/// 408 (Request Timeout), and 429 (Too Many Requests) are transient; 4xx
/// otherwise (404, 401, 403, ...) and 501 are permanent.
fn status_is_retryable(status: StatusCode) -> bool {
    use crate::sinks::util::{http::RetryStrategy, retries::RetryAction};
    matches!(
        RetryStrategy::Default.retry_action::<()>(status),
        RetryAction::Retry(_) | RetryAction::RetryPartial(_)
    )
}

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
    http_client: &HttpClient,
) -> Result<UnityCatalogTableSchema, ZerobusSinkError> {
    let token = get_oauth_token(
        http_client,
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
        .map_err(|e| ZerobusSinkError::SchemaError {
            message: format!("Failed to fetch table schema: {}", e),
            retryable: true,
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
        return Err(ZerobusSinkError::SchemaError {
            message: format!(
                "Unity Catalog API returned error {}: {}",
                status, error_text
            ),
            retryable: status_is_retryable(status),
        });
    }

    let body_bytes = response
        .into_body()
        .collect()
        .await
        .map(|c| c.to_bytes())
        .map_err(|e| ZerobusSinkError::SchemaError {
            message: format!("Failed to read response body: {}", e),
            retryable: true,
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
        .map_err(|e| ZerobusSinkError::SchemaError {
            message: format!("Failed to get OAuth token: {}", e),
            retryable: true,
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
        return Err(ZerobusSinkError::SchemaError {
            message: format!("OAuth token request failed {}: {}", status, error_text),
            retryable: status_is_retryable(status),
        });
    }

    let body_bytes = response
        .into_body()
        .collect()
        .await
        .map(|c| c.to_bytes())
        .map_err(|e| ZerobusSinkError::SchemaError {
            message: format!("Failed to read OAuth response body: {}", e),
            retryable: true,
        })?;

    let token_response: OAuthTokenResponse =
        serde_json::from_reader(body_bytes.reader()).map_err(|e| {
            ZerobusSinkError::ConfigError {
                message: format!("Failed to parse OAuth token response: {}", e),
            }
        })?;

    Ok(token_response.access_token)
}

/// Generate an Arrow schema from a Unity Catalog table schema.
///
/// The core UC-type → Arrow conversion lives in
/// [`databricks_zerobus_ingest_sdk::schema::arrow_schema_from_uc_schema`], which
/// produces the canonical Arrow schema the Databricks Arrow Flight server expects
/// (`STRING` → `LargeUtf8`, `TIMESTAMP` → `Timestamp(Microsecond, UTC)`, etc.).
///
/// The returned schema is used both to declare the Zerobus Arrow stream and to
/// drive the Arrow batch encoder, so a single source of truth keeps the encoded
/// `RecordBatch` schema in lock-step with the stream's declared schema.
pub fn generate_arrow_schema_from_schema(
    schema: &UnityCatalogTableSchema,
) -> Result<arrow::datatypes::Schema, ZerobusSinkError> {
    let arrow_schema =
        arrow_schema_from_uc_schema(schema).map_err(|e| ZerobusSinkError::ConfigError {
            message: format!("Failed to convert Unity Catalog schema to Arrow: {}", e),
        })?;

    if tracing::enabled!(tracing::Level::INFO) {
        info!(
            "Inferred Arrow schema from Unity Catalog table {}.{}.{}:\n{:#?}",
            schema.catalog_name, schema.schema_name, schema.name, arrow_schema
        );
    }

    Ok(arrow_schema)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_is_retryable_matches_canonical_policy() {
        // Transient — must retry.
        assert!(status_is_retryable(StatusCode::INTERNAL_SERVER_ERROR));
        assert!(status_is_retryable(StatusCode::BAD_GATEWAY));
        assert!(status_is_retryable(StatusCode::SERVICE_UNAVAILABLE));
        assert!(status_is_retryable(StatusCode::GATEWAY_TIMEOUT));
        assert!(status_is_retryable(StatusCode::REQUEST_TIMEOUT));
        assert!(status_is_retryable(StatusCode::TOO_MANY_REQUESTS));
        // Permanent — must not retry. 501 in particular: the server doesn't
        // support the requested functionality; retry won't change that.
        assert!(!status_is_retryable(StatusCode::NOT_IMPLEMENTED));
        assert!(!status_is_retryable(StatusCode::NOT_FOUND));
        assert!(!status_is_retryable(StatusCode::UNAUTHORIZED));
        assert!(!status_is_retryable(StatusCode::FORBIDDEN));
        assert!(!status_is_retryable(StatusCode::BAD_REQUEST));
    }

    /// Smoke test: the wrapper calls into the SDK and produces an Arrow schema
    /// with the expected fields and the Databricks-canonical type mapping
    /// (`STRING` → `LargeUtf8`, `BIGINT` → `Int64`).
    #[test]
    fn test_generate_arrow_schema_simple_schema() {
        use arrow::datatypes::DataType;

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

        let arrow_schema =
            generate_arrow_schema_from_schema(&schema).expect("arrow schema should be generated");
        assert_eq!(arrow_schema.fields().len(), 2);

        let id = arrow_schema.field_with_name("id").expect("id field");
        assert_eq!(id.data_type(), &DataType::Int64);
        assert!(!id.is_nullable());

        let body = arrow_schema.field_with_name("body").expect("body field");
        assert_eq!(body.data_type(), &DataType::LargeUtf8);
        assert!(body.is_nullable());
    }

    /// Exercises the UC→Arrow conversion on a schema with nested structs and
    /// arrays. The conversion itself is owned by the SDK; this guards that the
    /// wrapper feeds the fixture through and yields the expected complex fields.
    #[test]
    fn test_arrow_schema_nested_structs() {
        use arrow::datatypes::DataType;

        let json = include_str!("tests/fixtures/nested_structs_complete_schema.json");
        let schema: UnityCatalogTableSchema =
            serde_json::from_str(json).expect("Failed to parse nested_structs_complete schema");

        let arrow_schema =
            generate_arrow_schema_from_schema(&schema).expect("Failed to generate arrow schema");

        // Primitive fields map to their canonical Arrow types.
        assert_eq!(
            arrow_schema
                .field_with_name("field_003")
                .unwrap()
                .data_type(),
            &DataType::LargeUtf8,
        );
        assert_eq!(
            arrow_schema
                .field_with_name("field_007")
                .unwrap()
                .data_type(),
            &DataType::Int64,
        );

        // ARRAY<int64> becomes a List field.
        assert!(matches!(
            arrow_schema
                .field_with_name("field_027")
                .unwrap()
                .data_type(),
            DataType::List(_),
        ));

        // STRUCT columns become Struct fields.
        for struct_field in ["field_018", "field_021", "field_008"] {
            assert!(
                matches!(
                    arrow_schema
                        .field_with_name(struct_field)
                        .unwrap()
                        .data_type(),
                    DataType::Struct(_),
                ),
                "{struct_field} should be a Struct"
            );
        }
    }
}
