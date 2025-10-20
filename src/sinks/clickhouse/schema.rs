//! Schema fetching and Arrow type mapping for ClickHouse tables.

use std::sync::Arc;

use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use http::{Request, StatusCode};
use hyper::Body;
use serde::Deserialize;

use crate::http::{Auth, HttpClient};

#[derive(Debug, Deserialize)]
struct ColumnInfo {
    name: String,
    #[serde(rename = "type")]
    column_type: String,
}

/// Fetches the schema for a ClickHouse table and converts it to an Arrow schema.
pub async fn fetch_table_schema(
    client: &HttpClient,
    endpoint: &str,
    database: &str,
    table: &str,
    auth: Option<&Auth>,
) -> crate::Result<Arc<Schema>> {
    // Query to get table schema
    let query = format!(
        "SELECT name, type FROM system.columns WHERE database = '{}' AND table = '{}' FORMAT JSONEachRow",
        database, table
    );

    let encoded_query =
        percent_encoding::utf8_percent_encode(&query, percent_encoding::NON_ALPHANUMERIC)
            .to_string();
    let uri = format!("{}?query={}", endpoint, encoded_query);
    let mut request = Request::get(&uri).body(Body::empty()).unwrap();

    if let Some(auth) = auth {
        auth.apply(&mut request);
    }

    let response = client.send(request).await?;

    match response.status() {
        StatusCode::OK => {
            let body_bytes = hyper::body::to_bytes(response.into_body()).await?;
            let body_str = String::from_utf8(body_bytes.to_vec())
                .map_err(|e| format!("Failed to parse response as UTF-8: {}", e))?;

            parse_schema_from_response(&body_str)
        }
        status => Err(format!("Failed to fetch schema from ClickHouse: HTTP {}", status).into()),
    }
}

/// Parses the JSON response from ClickHouse and builds an Arrow schema.
fn parse_schema_from_response(response: &str) -> crate::Result<Arc<Schema>> {
    let mut fields = Vec::new();

    for line in response.lines() {
        if line.trim().is_empty() {
            continue;
        }

        let column: ColumnInfo = serde_json::from_str(line)
            .map_err(|e| format!("Failed to parse column info: {}", e))?;

        let arrow_type = clickhouse_type_to_arrow(&column.column_type);
        fields.push(Field::new(&column.name, arrow_type, true));
    }

    if fields.is_empty() {
        return Err("No columns found in table schema".into());
    }

    Ok(Arc::new(Schema::new(fields)))
}

fn clickhouse_type_to_arrow(ch_type: &str) -> DataType {
    let mut base_type = ch_type;
    if let Some(inner) = base_type.strip_prefix("Nullable(") {
        base_type = inner.strip_suffix(')').unwrap_or(inner);
    }
    if let Some(inner) = base_type.strip_prefix("LowCardinality(") {
        base_type = inner.strip_suffix(')').unwrap_or(inner);
    }

    match base_type {
        "String" | "FixedString" => DataType::Utf8,
        "Int8" => DataType::Int8,
        "Int16" => DataType::Int16,
        "Int32" => DataType::Int32,
        "Int64" => DataType::Int64,
        "UInt8" => DataType::UInt8,
        "UInt16" => DataType::UInt16,
        "UInt32" => DataType::UInt32,
        "UInt64" => DataType::UInt64,
        "Float32" => DataType::Float32,
        "Float64" => DataType::Float64,
        "Bool" => DataType::Boolean,
        "Date" => DataType::Date32,
        "Date32" => DataType::Date32,
        "DateTime" => DataType::Timestamp(TimeUnit::Second, None),
        _ if base_type.starts_with("DateTime64") => parse_datetime64_precision(base_type),
        _ if base_type.starts_with("Decimal") => DataType::Float64,
        _ if base_type.starts_with("Array") => DataType::Utf8, // Serialize as JSON
        _ if base_type.starts_with("Map") => DataType::Utf8,   // Serialize as JSON
        _ if base_type.starts_with("Tuple") => DataType::Utf8, // Serialize as JSON
        _ => {
            tracing::warn!("Unknown ClickHouse type '{}', defaulting to Utf8", ch_type);
            DataType::Utf8
        }
    }
}

/// Parses DateTime64 precision and returns the appropriate Arrow timestamp type.
/// DateTime64(0) -> Second
/// DateTime64(3) -> Millisecond
/// DateTime64(6) -> Microsecond
/// DateTime64(9) -> Nanosecond
fn parse_datetime64_precision(ch_type: &str) -> DataType {
    // Extract precision from DateTime64(N)
    if let Some(precision_str) = ch_type
        .strip_prefix("DateTime64(")
        .and_then(|s| s.split(')').next())
        .and_then(|s| s.split(',').next())
    {
        match precision_str.trim().parse::<u8>() {
            Ok(0) => DataType::Timestamp(TimeUnit::Second, None),
            Ok(1..=3) => DataType::Timestamp(TimeUnit::Millisecond, None),
            Ok(4..=6) => DataType::Timestamp(TimeUnit::Microsecond, None),
            Ok(7..=9) => DataType::Timestamp(TimeUnit::Nanosecond, None),
            _ => {
                tracing::warn!(
                    "Unsupported DateTime64 precision in '{}', defaulting to Millisecond",
                    ch_type
                );
                DataType::Timestamp(TimeUnit::Millisecond, None)
            }
        }
    } else {
        // Default to millisecond if we can't parse
        tracing::warn!(
            "Could not parse DateTime64 precision from '{}', defaulting to Millisecond",
            ch_type
        );
        DataType::Timestamp(TimeUnit::Millisecond, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clickhouse_type_mapping() {
        assert_eq!(clickhouse_type_to_arrow("String"), DataType::Utf8);
        assert_eq!(clickhouse_type_to_arrow("Int64"), DataType::Int64);
        assert_eq!(clickhouse_type_to_arrow("Float64"), DataType::Float64);
        assert_eq!(clickhouse_type_to_arrow("Bool"), DataType::Boolean);
        assert_eq!(
            clickhouse_type_to_arrow("DateTime"),
            DataType::Timestamp(TimeUnit::Second, None)
        );
    }

    #[test]
    fn test_datetime64_precision_mapping() {
        assert_eq!(
            clickhouse_type_to_arrow("DateTime64(0)"),
            DataType::Timestamp(TimeUnit::Second, None)
        );
        assert_eq!(
            clickhouse_type_to_arrow("DateTime64(3)"),
            DataType::Timestamp(TimeUnit::Millisecond, None)
        );
        assert_eq!(
            clickhouse_type_to_arrow("DateTime64(6)"),
            DataType::Timestamp(TimeUnit::Microsecond, None)
        );
        assert_eq!(
            clickhouse_type_to_arrow("DateTime64(9)"),
            DataType::Timestamp(TimeUnit::Nanosecond, None)
        );
        // Test with timezone
        assert_eq!(
            clickhouse_type_to_arrow("DateTime64(9, 'UTC')"),
            DataType::Timestamp(TimeUnit::Nanosecond, None)
        );
        // Test edge cases for precision ranges
        assert_eq!(
            clickhouse_type_to_arrow("DateTime64(1)"),
            DataType::Timestamp(TimeUnit::Millisecond, None)
        );
        assert_eq!(
            clickhouse_type_to_arrow("DateTime64(4)"),
            DataType::Timestamp(TimeUnit::Microsecond, None)
        );
        assert_eq!(
            clickhouse_type_to_arrow("DateTime64(7)"),
            DataType::Timestamp(TimeUnit::Nanosecond, None)
        );
    }

    #[test]
    fn test_nullable_type_mapping() {
        assert_eq!(clickhouse_type_to_arrow("Nullable(String)"), DataType::Utf8);
        assert_eq!(clickhouse_type_to_arrow("Nullable(Int64)"), DataType::Int64);
    }

    #[test]
    fn test_lowcardinality_type_mapping() {
        assert_eq!(
            clickhouse_type_to_arrow("LowCardinality(String)"),
            DataType::Utf8
        );
        assert_eq!(
            clickhouse_type_to_arrow("LowCardinality(FixedString(10))"),
            DataType::Utf8
        );
        // Nullable + LowCardinality
        assert_eq!(
            clickhouse_type_to_arrow("Nullable(LowCardinality(String))"),
            DataType::Utf8
        );
    }

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
}
