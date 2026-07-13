//! Schema fetching and Arrow schema construction for ClickHouse tables.

use std::str::FromStr;

use arrow::datatypes::{Field, Schema};
use async_trait::async_trait;
use bytes::Bytes;
use http::{Request, StatusCode};
use hyper::Body;
use serde::Deserialize;
use url::form_urlencoded;
use vector_lib::codecs::encoding::ArrowIpcCompression;
use vector_lib::codecs::encoding::format::{ArrowEncodingError, SchemaProvider};

use crate::http::{Auth, HttpClient};

use super::parser::ClickHouseType;

/// String constants for ClickHouse column `default_kind` values.
const COLUMN_KIND_REGULAR: &str = "";
const COLUMN_KIND_DEFAULT: &str = "DEFAULT";

/// Minimum ClickHouse `(major, minor)` that reliably decodes compressed Arrow IPC buffers
/// (see <https://github.com/ClickHouse/ClickHouse/pull/47114>).
pub const MIN_ARROW_IPC_COMPRESSION_VERSION: (u32, u32) = (23, 11);

#[derive(Debug, Deserialize)]
struct ColumnInfo {
    name: String,
    #[serde(rename = "type")]
    column_type: String,
    default_kind: String,
}

impl TryFrom<ColumnInfo> for Field {
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn try_from(column: ColumnInfo) -> Result<Self, Self::Error> {
        let ch_type = ClickHouseType::from_str(&column.column_type)?;
        let (dt, nullable) = (&ch_type)
            .try_into()
            .map_err(|e| format!("Failed to convert column '{}': {e}", column.name))?;
        // DEFAULT columns have server-side defaults, so users don't need to provide them.
        let nullable = nullable || column.default_kind == COLUMN_KIND_DEFAULT;
        Ok(Field::new(column.name, dt, nullable))
    }
}

/// Sends a GET query to the ClickHouse HTTP interface and returns the response body bytes.
///
/// `query_string` is the already URL-encoded query string (everything after `?`). Applies auth if
/// present and errors on any non-`200` status, using `context` to describe the failed operation.
async fn get_query_bytes(
    client: &HttpClient,
    endpoint: &str,
    query_string: &str,
    auth: Option<&Auth>,
    context: &str,
) -> crate::Result<Bytes> {
    let uri = format!("{endpoint}?{query_string}");
    let mut request = Request::get(&uri)
        .body(Body::empty())
        .map_err(|e| format!("Failed to build request: {e}"))?;

    if let Some(auth) = auth {
        auth.apply(&mut request);
    }

    let response = client.send(request).await?;

    if response.status() != StatusCode::OK {
        return Err(format!("{context}: HTTP {}", response.status()).into());
    }

    Ok(http_body::Body::collect(response.into_body())
        .await?
        .to_bytes())
}

/// Fetches the schema for a ClickHouse table and converts it to an Arrow schema.
pub async fn fetch_table_schema(
    client: &HttpClient,
    endpoint: &str,
    database: &str,
    table: &str,
    auth: Option<&Auth>,
) -> crate::Result<Schema> {
    let query = format!(
        "SELECT name, type, default_kind \
         FROM system.columns \
         WHERE database = {{db:String}} AND table = {{tbl:String}} \
         AND default_kind IN ('{COLUMN_KIND_REGULAR}', '{COLUMN_KIND_DEFAULT}') \
         ORDER BY position \
         FORMAT JSONEachRow"
    );

    let query_string = form_urlencoded::Serializer::new(String::new())
        .append_pair("query", &query)
        .append_pair("param_db", database)
        .append_pair("param_tbl", table)
        .finish();

    let body_bytes = get_query_bytes(
        client,
        endpoint,
        &query_string,
        auth,
        "Failed to fetch schema from ClickHouse",
    )
    .await?;

    // Pass bytes directly instead of converting to a UTF-8 String first
    parse_schema_from_response(&body_bytes)
}

/// Returns an error if `compression` is requested but the ClickHouse server is too old to decode
/// compressed Arrow IPC. Older servers accept the insert but fail to read it, so this is checked
/// once at startup to fail fast rather than silently dropping data at runtime.
pub async fn ensure_arrow_compression_supported(
    client: &HttpClient,
    endpoint: &str,
    auth: Option<&Auth>,
    compression: ArrowIpcCompression,
) -> crate::Result<()> {
    if compression == ArrowIpcCompression::None {
        return Ok(());
    }

    let (major, minor) = fetch_server_version(client, endpoint, auth).await?;
    let (min_major, min_minor) = MIN_ARROW_IPC_COMPRESSION_VERSION;
    if (major, minor) < (min_major, min_minor) {
        return Err(format!(
            "Arrow IPC compression requires ClickHouse >= {min_major}.{min_minor}, but the server \
             reported {major}.{minor}. Set the batch encoding 'compression' to 'none' or upgrade \
             ClickHouse."
        )
        .into());
    }

    Ok(())
}

/// Fetches the ClickHouse server version, returning it as a `(major, minor)` tuple.
async fn fetch_server_version(
    client: &HttpClient,
    endpoint: &str,
    auth: Option<&Auth>,
) -> crate::Result<(u32, u32)> {
    let query_string = form_urlencoded::Serializer::new(String::new())
        .append_pair("query", "SELECT version() FORMAT TabSeparated")
        .finish();

    let body_bytes = get_query_bytes(
        client,
        endpoint,
        &query_string,
        auth,
        "Failed to query ClickHouse version",
    )
    .await?;

    parse_version(std::str::from_utf8(&body_bytes)?)
}

/// Parses a ClickHouse version string (e.g. `24.3.1.2823`) into a `(major, minor)` tuple.
pub(crate) fn parse_version(raw: &str) -> crate::Result<(u32, u32)> {
    let trimmed = raw.trim();
    let mut parts = trimmed.split('.');
    match (
        parts.next().and_then(|s| s.parse::<u32>().ok()),
        parts.next().and_then(|s| s.parse::<u32>().ok()),
    ) {
        (Some(major), Some(minor)) => Ok((major, minor)),
        _ => Err(format!("Could not parse ClickHouse version from '{trimmed}'").into()),
    }
}

/// Parses the JSONEachRow response from ClickHouse and builds an Arrow schema.
fn parse_schema_from_response(response: &[u8]) -> crate::Result<Schema> {
    let fields = serde_json::Deserializer::from_slice(response)
        .into_iter::<ColumnInfo>()
        .map(|res| -> crate::Result<Field> { res?.try_into() })
        .collect::<crate::Result<Vec<Field>>>()?;

    if fields.is_empty() {
        return Err("Table does not exist or has no columns".into());
    }

    Ok(Schema::new(fields))
}

/// Schema provider implementation for ClickHouse tables.
#[derive(Clone, Debug)]
pub struct ClickHouseSchemaProvider {
    client: HttpClient,
    endpoint: String,
    database: String,
    table: String,
    auth: Option<Auth>,
}

impl ClickHouseSchemaProvider {
    /// Create a new ClickHouse schema provider.
    pub const fn new(
        client: HttpClient,
        endpoint: String,
        database: String,
        table: String,
        auth: Option<Auth>,
    ) -> Self {
        Self {
            client,
            endpoint,
            database,
            table,
            auth,
        }
    }
}

#[async_trait]
impl SchemaProvider for ClickHouseSchemaProvider {
    async fn get_schema(&self) -> Result<Schema, ArrowEncodingError> {
        fetch_table_schema(
            &self.client,
            &self.endpoint,
            &self.database,
            &self.table,
            self.auth.as_ref(),
        )
        .await
        .map_err(|e| ArrowEncodingError::SchemaFetchError {
            message: e.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::datatypes::{DataType, TimeUnit};
    use indoc::indoc;

    #[test]
    fn test_parse_version() {
        assert_eq!(parse_version("24.3.1.2823").unwrap(), (24, 3));
        assert_eq!(parse_version("23.8.16.16\n").unwrap(), (23, 8));
        assert_eq!(parse_version("25.12.11.4").unwrap(), (25, 12));
        assert!(parse_version("not-a-version").is_err());
        assert!(parse_version("").is_err());
    }

    #[test]
    fn test_min_version_boundary() {
        // 23.10 must be rejected, 23.11 (the confirmed floor) must be accepted.
        assert!((23, 10) < MIN_ARROW_IPC_COMPRESSION_VERSION);
        assert!((23, 11) >= MIN_ARROW_IPC_COMPRESSION_VERSION);
        assert!((24, 1) >= MIN_ARROW_IPC_COMPRESSION_VERSION);
    }

    #[test]
    fn test_parse_schema() {
        let response = indoc! {r#"
            {"name":"id","type":"Int64","default_kind":""}
            {"name":"message","type":"String","default_kind":""}
            {"name":"timestamp","type":"DateTime","default_kind":""}
        "#};

        let schema = parse_schema_from_response(response.as_bytes()).unwrap();
        assert_eq!(schema.fields().len(), 3);
        assert_eq!(schema.field(0).name(), "id");
        assert_eq!(schema.field(0).data_type(), &DataType::Int64);
        assert_eq!(schema.field(1).name(), "message");
        assert_eq!(schema.field(1).data_type(), &DataType::Utf8);
        assert_eq!(schema.field(2).name(), "timestamp");
        assert_eq!(
            schema.field(2).data_type(),
            &DataType::Timestamp(TimeUnit::Second, None)
        );
    }

    #[test]
    fn test_parse_schema_with_type_parameters() {
        let response = indoc! {r#"
            {"name":"bytes_sent","type":"Decimal(18, 2)","default_kind":""}
            {"name":"timestamp","type":"DateTime64(6)","default_kind":""}
            {"name":"duration_ms","type":"Decimal32(4)","default_kind":""}
        "#};

        let schema = parse_schema_from_response(response.as_bytes()).unwrap();
        assert_eq!(schema.fields().len(), 3);

        assert_eq!(schema.field(0).name(), "bytes_sent");
        assert_eq!(schema.field(0).data_type(), &DataType::Decimal128(18, 2));

        assert_eq!(schema.field(1).name(), "timestamp");
        assert_eq!(
            schema.field(1).data_type(),
            &DataType::Timestamp(TimeUnit::Microsecond, None)
        );

        assert_eq!(schema.field(2).name(), "duration_ms");
        assert_eq!(schema.field(2).data_type(), &DataType::Decimal128(9, 4));
    }

    #[test]
    fn test_schema_field_ordering() {
        let response = indoc! {r#"
            {"name":"timestamp","type":"DateTime64(3)","default_kind":""}
            {"name":"host","type":"String","default_kind":""}
            {"name":"message","type":"String","default_kind":""}
            {"name":"id","type":"Int64","default_kind":""}
            {"name":"score","type":"Float64","default_kind":""}
            {"name":"active","type":"Bool","default_kind":""}
            {"name":"name","type":"String","default_kind":""}
        "#};

        let schema = parse_schema_from_response(response.as_bytes()).unwrap();
        assert_eq!(schema.fields().len(), 7);

        assert_eq!(schema.field(0).name(), "timestamp");
        assert_eq!(schema.field(1).name(), "host");
        assert_eq!(schema.field(2).name(), "message");
        assert_eq!(schema.field(3).name(), "id");
        assert_eq!(schema.field(4).name(), "score");
        assert_eq!(schema.field(5).name(), "active");
        assert_eq!(schema.field(6).name(), "name");

        assert_eq!(
            schema.field(0).data_type(),
            &DataType::Timestamp(TimeUnit::Millisecond, None)
        );
        assert_eq!(schema.field(1).data_type(), &DataType::Utf8);
        assert_eq!(schema.field(3).data_type(), &DataType::Int64);
        assert_eq!(schema.field(4).data_type(), &DataType::Float64);
        assert_eq!(schema.field(5).data_type(), &DataType::Boolean);
    }

    /// Tests that DEFAULT columns are marked nullable in the parsed schema,
    /// since ClickHouse fills them with server-side defaults when omitted.
    #[test]
    fn test_default_columns_marked_nullable() {
        // The SQL query filters out MATERIALIZED/ALIAS/EPHEMERAL, so
        // parse_schema_from_response only sees regular and DEFAULT columns.
        let response = indoc! {r#"
            {"name":"id","type":"Int64","default_kind":""}
            {"name":"status","type":"String","default_kind":"DEFAULT"}
            {"name":"message","type":"String","default_kind":""}
        "#};

        let schema = parse_schema_from_response(response.as_bytes()).unwrap();
        assert_eq!(schema.fields().len(), 3);

        // Regular column: non-nullable
        assert!(!schema.field(0).is_nullable());
        // DEFAULT column: forced nullable even though type is non-nullable
        assert_eq!(schema.field(1).name(), "status");
        assert!(schema.field(1).is_nullable());
        // Regular column: non-nullable
        assert!(!schema.field(2).is_nullable());
    }

    /// Simulates the response after the SQL query has filtered out
    /// MATERIALIZED/ALIAS/EPHEMERAL columns, leaving only regular and DEFAULT.
    #[test]
    fn test_post_filter_schema_with_default() {
        let response = r#"{"name":"id","type":"Int64","default_kind":""}
{"name":"created_at","type":"DateTime64(3)","default_kind":"DEFAULT"}
{"name":"message","type":"Nullable(String)","default_kind":""}
"#;

        let schema = parse_schema_from_response(response.as_bytes()).unwrap();
        assert_eq!(schema.fields().len(), 3);

        assert_eq!(schema.field(0).name(), "id");
        assert!(!schema.field(0).is_nullable());

        assert_eq!(schema.field(1).name(), "created_at");
        assert!(schema.field(1).is_nullable()); // DEFAULT → nullable

        assert_eq!(schema.field(2).name(), "message");
        assert!(schema.field(2).is_nullable()); // Nullable(String) → nullable
    }
}
