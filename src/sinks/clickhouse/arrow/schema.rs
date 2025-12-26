//! Schema fetching and Arrow schema construction for ClickHouse tables.

use arrow::datatypes::{Field, Schema};
use async_trait::async_trait;
use http::{Request, StatusCode};
use hyper::Body;
use serde::Deserialize;
use vector_lib::codecs::encoding::format::{ArrowEncodingError, SchemaProvider};

use crate::http::{Auth, HttpClient};

use super::parser::clickhouse_type_to_arrow;

#[derive(Debug, Deserialize)]
struct ColumnInfo {
    name: String,
    #[serde(rename = "type")]
    column_type: String,
}

/// URL-encodes a string for use in HTTP query parameters.
fn url_encode(s: &str) -> String {
    percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC).to_string()
}

/// Fetches the schema for a ClickHouse table and converts it to an Arrow schema.
pub async fn fetch_table_schema(
    client: &HttpClient,
    endpoint: &str,
    database: &str,
    table: &str,
    auth: Option<&Auth>,
) -> crate::Result<Schema> {
    let query = "SELECT name, type \
                 FROM system.columns \
                 WHERE database = {db:String} AND table = {tbl:String} \
                 ORDER BY position \
                 FORMAT JSONEachRow";

    // Build URI with query and parameters
    let uri = format!(
        "{}?query={}&param_db={}&param_tbl={}",
        endpoint,
        url_encode(query),
        url_encode(database),
        url_encode(table)
    );
    let mut request = Request::get(&uri).body(Body::empty()).unwrap();

    if let Some(auth) = auth {
        auth.apply(&mut request);
    }

    let response = client.send(request).await?;

    match response.status() {
        StatusCode::OK => {
            let body_bytes = http_body::Body::collect(response.into_body())
                .await?
                .to_bytes();
            let body_str = String::from_utf8(body_bytes.into())
                .map_err(|e| format!("Failed to parse response as UTF-8: {}", e))?;

            parse_schema_from_response(&body_str)
        }
        status => Err(format!("Failed to fetch schema from ClickHouse: HTTP {}", status).into()),
    }
}

/// Parses the JSON response from ClickHouse and builds an Arrow schema.
fn parse_schema_from_response(response: &str) -> crate::Result<Schema> {
    let mut columns: Vec<ColumnInfo> = Vec::new();

    for line in response.lines() {
        if line.trim().is_empty() {
            continue;
        }

        let column: ColumnInfo = serde_json::from_str(line)
            .map_err(|e| format!("Failed to parse column info: {}", e))?;
        columns.push(column);
    }

    if columns.is_empty() {
        return Err("No columns found in table schema".into());
    }

    let mut fields = Vec::new();
    for column in columns {
        let (arrow_type, nullable) = clickhouse_type_to_arrow(&column.column_type)
            .map_err(|e| format!("Failed to convert column '{}': {}", column.name, e))?;
        fields.push(Field::new(&column.name, arrow_type, nullable));
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

    #[test]
    fn test_parse_schema() {
        let response = r#"{"name":"id","type":"Int64"}
{"name":"message","type":"String"}
{"name":"timestamp","type":"DateTime"}
"#;

        let schema = parse_schema_from_response(response).unwrap();
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
        // Test that type string parsing works for types with parameters
        let response = r#"{"name":"bytes_sent","type":"Decimal(18, 2)"}
{"name":"timestamp","type":"DateTime64(6)"}
{"name":"duration_ms","type":"Decimal32(4)"}
"#;

        let schema = parse_schema_from_response(response).unwrap();
        assert_eq!(schema.fields().len(), 3);

        // Check Decimal parsed from type string
        assert_eq!(schema.field(0).name(), "bytes_sent");
        assert_eq!(schema.field(0).data_type(), &DataType::Decimal128(18, 2));

        // Check DateTime64 parsed from type string
        assert_eq!(schema.field(1).name(), "timestamp");
        assert_eq!(
            schema.field(1).data_type(),
            &DataType::Timestamp(TimeUnit::Microsecond, None)
        );

        // Check Decimal32 parsed from type string
        assert_eq!(schema.field(2).name(), "duration_ms");
        assert_eq!(schema.field(2).data_type(), &DataType::Decimal128(9, 4));
    }

    #[test]
    fn test_schema_field_ordering() {
        let response = r#"{"name":"timestamp","type":"DateTime64(3)"}
{"name":"host","type":"String"}
{"name":"message","type":"String"}
{"name":"id","type":"Int64"}
{"name":"score","type":"Float64"}
{"name":"active","type":"Bool"}
{"name":"name","type":"String"}
"#;

        let schema = parse_schema_from_response(response).unwrap();
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
}
