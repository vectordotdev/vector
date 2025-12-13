//! Schema fetching and Arrow type mapping for ClickHouse tables.

use std::sync::Arc;

use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use async_trait::async_trait;
use http::{Request, StatusCode};
use hyper::Body;
use serde::Deserialize;
use vector_lib::codecs::encoding::format::{ArrowEncodingError, SchemaProvider};

use crate::http::{Auth, HttpClient};

const DECIMAL32_PRECISION: u8 = 9;
const DECIMAL64_PRECISION: u8 = 18;
const DECIMAL128_PRECISION: u8 = 38;
const DECIMAL256_PRECISION: u8 = 76;

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
) -> crate::Result<Arc<Schema>> {
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
fn parse_schema_from_response(response: &str) -> crate::Result<Arc<Schema>> {
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

    Ok(Arc::new(Schema::new(fields)))
}

/// Unwraps ClickHouse type modifiers like Nullable() and LowCardinality().
/// Returns a tuple of (base_type, is_nullable).
/// For example: "Nullable(LowCardinality(String))" -> ("String", true)
fn unwrap_type_modifiers(ch_type: &str) -> (&str, bool) {
    let mut base = ch_type;
    let mut is_nullable = false;

    // Check for Nullable wrapper
    if let Some(inner) = base.strip_prefix("Nullable(") {
        is_nullable = true;
        base = inner.strip_suffix(')').unwrap_or(inner);
    }

    // Check for LowCardinality wrapper
    if let Some(inner) = base.strip_prefix("LowCardinality(") {
        base = inner.strip_suffix(')').unwrap_or(inner);
    }

    (base, is_nullable)
}

fn unsupported(ch_type: &str, kind: &str) -> String {
    format!(
        "{kind} type '{ch_type}' is not supported. \
         ClickHouse {kind} types cannot be automatically converted to Arrow format."
    )
}

fn clickhouse_type_to_arrow(ch_type: &str) -> Result<(DataType, bool), String> {
    let (base_type, is_nullable) = unwrap_type_modifiers(ch_type);

    let data_type = match base_type {
        // String types
        "String" => DataType::Utf8,
        _ if base_type.starts_with("FixedString") => DataType::Utf8,

        // Integer types
        "Int8" => DataType::Int8,
        "Int16" => DataType::Int16,
        "Int32" => DataType::Int32,
        "Int64" => DataType::Int64,
        "UInt8" => DataType::UInt8,
        "UInt16" => DataType::UInt16,
        "UInt32" => DataType::UInt32,
        "UInt64" => DataType::UInt64,

        // Floating point types
        "Float32" => DataType::Float32,
        "Float64" => DataType::Float64,

        // Boolean
        "Bool" => DataType::Boolean,

        // Date and time types (timezones not currently handled, defaults to UTC)
        "Date" | "Date32" => DataType::Date32,
        "DateTime" => DataType::Timestamp(TimeUnit::Second, None),
        _ if base_type.starts_with("DateTime64") => parse_datetime64_precision(base_type)?,

        // Decimal types
        _ if base_type.starts_with("Decimal") => parse_decimal_type(base_type)?,

        // Complex types
        _ if base_type.starts_with("Array") => {
            return Err(unsupported(ch_type, "Array"));
        }
        _ if base_type.starts_with("Tuple") => {
            return Err(unsupported(ch_type, "Tuple"));
        }
        _ if base_type.starts_with("Map") => {
            return Err(unsupported(ch_type, "Map"));
        }

        // Unknown types
        _ => {
            return Err(format!(
                "Unknown ClickHouse type '{}'. This type cannot be automatically converted to Arrow format.",
                ch_type
            ));
        }
    };

    Ok((data_type, is_nullable))
}

/// Extracts an identifier from the start of a string.
/// Returns (identifier, remaining_string).
fn extract_identifier(input: &str) -> (&str, &str) {
    for (i, c) in input.char_indices() {
        if c.is_alphabetic() || c == '_' || (i > 0 && c.is_numeric()) {
            continue;
        }
        return (&input[..i], &input[i..]);
    }
    (input, "")
}

/// Parses comma-separated arguments from a parenthesized string.
/// Input: "(arg1, arg2, arg3)" -> Output: Ok(vec!["arg1".to_string(), "arg2".to_string(), "arg3".to_string()])
/// Returns an error if parentheses are malformed.
fn parse_args(input: &str) -> Result<Vec<String>, String> {
    let trimmed = input.trim();
    if !trimmed.starts_with('(') || !trimmed.ends_with(')') {
        return Err(format!(
            "Expected parentheses around arguments in '{}'",
            input
        ));
    }

    let inner = trimmed[1..trimmed.len() - 1].trim();
    if inner.is_empty() {
        return Ok(vec![]);
    }

    // Split by comma, handling nested parentheses and quotes
    let mut args = Vec::new();
    let mut current_arg = String::new();
    let mut depth = 0;
    let mut in_quotes = false;

    for c in inner.chars() {
        match c {
            '\'' if !in_quotes => in_quotes = true,
            '\'' if in_quotes => in_quotes = false,
            '(' if !in_quotes => depth += 1,
            ')' if !in_quotes => depth -= 1,
            ',' if depth == 0 && !in_quotes => {
                args.push(current_arg.trim().to_string());
                current_arg = String::new();
                continue;
            }
            _ => {}
        }
        current_arg.push(c);
    }

    if !current_arg.trim().is_empty() {
        args.push(current_arg.trim().to_string());
    }

    Ok(args)
}

/// Parses ClickHouse Decimal types and returns the appropriate Arrow decimal type.
/// ClickHouse formats:
/// - Decimal(P, S) -> generic decimal with precision P and scale S
/// - Decimal32(S) -> precision up to 9, scale S
/// - Decimal64(S) -> precision up to 18, scale S
/// - Decimal128(S) -> precision up to 38, scale S
/// - Decimal256(S) -> precision up to 76, scale S
///
/// Uses metadata from ClickHouse's system.columns when available, otherwise falls back to parsing the type string.
fn parse_decimal_type(ch_type: &str) -> Result<DataType, String> {
    // Parse from type string
    let (type_name, args_str) = extract_identifier(ch_type);

    let result = parse_args(args_str).ok().and_then(|args| match type_name {
        "Decimal" if args.len() == 2 => args[0].parse::<u8>().ok().zip(args[1].parse::<i8>().ok()),
        "Decimal32" | "Decimal64" | "Decimal128" | "Decimal256" if args.len() == 1 => {
            args[0].parse::<i8>().ok().map(|scale| {
                let precision = match type_name {
                    "Decimal32" => DECIMAL32_PRECISION,
                    "Decimal64" => DECIMAL64_PRECISION,
                    "Decimal128" => DECIMAL128_PRECISION,
                    "Decimal256" => DECIMAL256_PRECISION,
                    _ => unreachable!(),
                };
                (precision, scale)
            })
        }
        _ => None,
    });

    result
        .map(|(precision, scale)| {
            if precision <= DECIMAL128_PRECISION {
                DataType::Decimal128(precision, scale)
            } else {
                DataType::Decimal256(precision, scale)
            }
        })
        .ok_or_else(|| {
            format!(
                "Could not parse Decimal type '{}'. Valid formats: Decimal(P,S), Decimal32(S), Decimal64(S), Decimal128(S), Decimal256(S)",
                ch_type
            )
        })
}

/// Parses DateTime64 precision and returns the appropriate Arrow timestamp type.
/// DateTime64(0) -> Second
/// DateTime64(3) -> Millisecond
/// DateTime64(6) -> Microsecond
/// DateTime64(9) -> Nanosecond
///
fn parse_datetime64_precision(ch_type: &str) -> Result<DataType, String> {
    // Parse from type string
    let (_type_name, args_str) = extract_identifier(ch_type);

    let args = parse_args(args_str).map_err(|e| {
        format!(
            "Could not parse DateTime64 arguments from '{}': {}. Expected format: DateTime64(0-9) or DateTime64(0-9, 'timezone')",
            ch_type, e
        )
    })?;

    // DateTime64(precision) or DateTime64(precision, 'timezone')
    if args.is_empty() {
        return Err(format!(
            "DateTime64 type '{}' has no precision argument. Expected format: DateTime64(0-9) or DateTime64(0-9, 'timezone')",
            ch_type
        ));
    }

    // Parse the precision (first argument)
    match args[0].parse::<u8>() {
        Ok(0) => Ok(DataType::Timestamp(TimeUnit::Second, None)),
        Ok(1..=3) => Ok(DataType::Timestamp(TimeUnit::Millisecond, None)),
        Ok(4..=6) => Ok(DataType::Timestamp(TimeUnit::Microsecond, None)),
        Ok(7..=9) => Ok(DataType::Timestamp(TimeUnit::Nanosecond, None)),
        _ => Err(format!(
            "Unsupported DateTime64 precision in '{}'. Precision must be 0-9",
            ch_type
        )),
    }
}

/// Schema provider implementation for ClickHouse tables.
///
/// Fetches the table schema from ClickHouse at runtime using the system.columns table.
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
    async fn get_schema(&self) -> Result<Arc<Schema>, ArrowEncodingError> {
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

    // Helper function for tests that don't need metadata
    fn convert_type_no_metadata(ch_type: &str) -> Result<(DataType, bool), String> {
        clickhouse_type_to_arrow(ch_type)
    }

    #[test]
    fn test_clickhouse_type_mapping() {
        assert_eq!(
            convert_type_no_metadata("String").expect("Failed to convert ClickHouse type to Arrow"),
            (DataType::Utf8, false)
        );
        assert_eq!(
            convert_type_no_metadata("Int64").expect("Failed to convert ClickHouse type to Arrow"),
            (DataType::Int64, false)
        );
        assert_eq!(
            convert_type_no_metadata("Float64")
                .expect("Failed to convert ClickHouse type to Arrow"),
            (DataType::Float64, false)
        );
        assert_eq!(
            convert_type_no_metadata("Bool").expect("Failed to convert ClickHouse type to Arrow"),
            (DataType::Boolean, false)
        );
        assert_eq!(
            convert_type_no_metadata("DateTime")
                .expect("Failed to convert ClickHouse type to Arrow"),
            (DataType::Timestamp(TimeUnit::Second, None), false)
        );
    }

    #[test]
    fn test_datetime64_precision_mapping() {
        assert_eq!(
            convert_type_no_metadata("DateTime64(0)")
                .expect("Failed to convert ClickHouse type to Arrow"),
            (DataType::Timestamp(TimeUnit::Second, None), false)
        );
        assert_eq!(
            convert_type_no_metadata("DateTime64(3)")
                .expect("Failed to convert ClickHouse type to Arrow"),
            (DataType::Timestamp(TimeUnit::Millisecond, None), false)
        );
        assert_eq!(
            convert_type_no_metadata("DateTime64(6)")
                .expect("Failed to convert ClickHouse type to Arrow"),
            (DataType::Timestamp(TimeUnit::Microsecond, None), false)
        );
        assert_eq!(
            convert_type_no_metadata("DateTime64(9)")
                .expect("Failed to convert ClickHouse type to Arrow"),
            (DataType::Timestamp(TimeUnit::Nanosecond, None), false)
        );
        // Test with timezones
        assert_eq!(
            convert_type_no_metadata("DateTime64(9, 'UTC')")
                .expect("Failed to convert ClickHouse type to Arrow"),
            (DataType::Timestamp(TimeUnit::Nanosecond, None), false)
        );
        assert_eq!(
            convert_type_no_metadata("DateTime64(6, 'UTC')")
                .expect("Failed to convert ClickHouse type to Arrow"),
            (DataType::Timestamp(TimeUnit::Microsecond, None), false)
        );
        assert_eq!(
            convert_type_no_metadata("DateTime64(9, 'America/New_York')")
                .expect("Failed to convert ClickHouse type to Arrow"),
            (DataType::Timestamp(TimeUnit::Nanosecond, None), false)
        );
        // Test edge cases for precision ranges
        assert_eq!(
            convert_type_no_metadata("DateTime64(1)")
                .expect("Failed to convert ClickHouse type to Arrow"),
            (DataType::Timestamp(TimeUnit::Millisecond, None), false)
        );
        assert_eq!(
            convert_type_no_metadata("DateTime64(4)")
                .expect("Failed to convert ClickHouse type to Arrow"),
            (DataType::Timestamp(TimeUnit::Microsecond, None), false)
        );
        assert_eq!(
            convert_type_no_metadata("DateTime64(7)")
                .expect("Failed to convert ClickHouse type to Arrow"),
            (DataType::Timestamp(TimeUnit::Nanosecond, None), false)
        );
    }

    #[test]
    fn test_nullable_type_mapping() {
        // Non-nullable types
        assert_eq!(
            convert_type_no_metadata("String").expect("Failed to convert ClickHouse type to Arrow"),
            (DataType::Utf8, false)
        );
        assert_eq!(
            convert_type_no_metadata("Int64").expect("Failed to convert ClickHouse type to Arrow"),
            (DataType::Int64, false)
        );

        // Nullable types
        assert_eq!(
            convert_type_no_metadata("Nullable(String)")
                .expect("Failed to convert ClickHouse type to Arrow"),
            (DataType::Utf8, true)
        );
        assert_eq!(
            convert_type_no_metadata("Nullable(Int64)")
                .expect("Failed to convert ClickHouse type to Arrow"),
            (DataType::Int64, true)
        );
        assert_eq!(
            convert_type_no_metadata("Nullable(Float64)")
                .expect("Failed to convert ClickHouse type to Arrow"),
            (DataType::Float64, true)
        );
    }

    #[test]
    fn test_lowcardinality_type_mapping() {
        assert_eq!(
            convert_type_no_metadata("LowCardinality(String)")
                .expect("Failed to convert ClickHouse type to Arrow"),
            (DataType::Utf8, false)
        );
        assert_eq!(
            convert_type_no_metadata("LowCardinality(FixedString(10))")
                .expect("Failed to convert ClickHouse type to Arrow"),
            (DataType::Utf8, false)
        );
        // Nullable + LowCardinality
        assert_eq!(
            convert_type_no_metadata("Nullable(LowCardinality(String))")
                .expect("Failed to convert ClickHouse type to Arrow"),
            (DataType::Utf8, true)
        );
    }

    #[test]
    fn test_decimal_type_mapping() {
        // Generic Decimal(P, S)
        assert_eq!(
            convert_type_no_metadata("Decimal(10, 2)")
                .expect("Failed to convert ClickHouse type to Arrow"),
            (DataType::Decimal128(10, 2), false)
        );
        assert_eq!(
            convert_type_no_metadata("Decimal(38, 6)")
                .expect("Failed to convert ClickHouse type to Arrow"),
            (DataType::Decimal128(38, 6), false)
        );
        assert_eq!(
            convert_type_no_metadata("Decimal(50, 10)")
                .expect("Failed to convert ClickHouse type to Arrow"),
            (DataType::Decimal256(50, 10), false)
        );

        // Generic Decimal without spaces and with spaces
        assert_eq!(
            convert_type_no_metadata("Decimal(10,2)")
                .expect("Failed to convert ClickHouse type to Arrow"),
            (DataType::Decimal128(10, 2), false)
        );
        assert_eq!(
            convert_type_no_metadata("Decimal( 18 , 6 )")
                .expect("Failed to convert ClickHouse type to Arrow"),
            (DataType::Decimal128(18, 6), false)
        );

        // Decimal32(S) - precision up to 9
        assert_eq!(
            convert_type_no_metadata("Decimal32(2)")
                .expect("Failed to convert ClickHouse type to Arrow"),
            (DataType::Decimal128(9, 2), false)
        );
        assert_eq!(
            convert_type_no_metadata("Decimal32(4)")
                .expect("Failed to convert ClickHouse type to Arrow"),
            (DataType::Decimal128(9, 4), false)
        );

        // Decimal64(S) - precision up to 18
        assert_eq!(
            convert_type_no_metadata("Decimal64(4)")
                .expect("Failed to convert ClickHouse type to Arrow"),
            (DataType::Decimal128(18, 4), false)
        );
        assert_eq!(
            convert_type_no_metadata("Decimal64(8)")
                .expect("Failed to convert ClickHouse type to Arrow"),
            (DataType::Decimal128(18, 8), false)
        );

        // Decimal128(S) - precision up to 38
        assert_eq!(
            convert_type_no_metadata("Decimal128(10)")
                .expect("Failed to convert ClickHouse type to Arrow"),
            (DataType::Decimal128(38, 10), false)
        );

        // Decimal256(S) - precision up to 76
        assert_eq!(
            convert_type_no_metadata("Decimal256(20)")
                .expect("Failed to convert ClickHouse type to Arrow"),
            (DataType::Decimal256(76, 20), false)
        );

        // With Nullable wrapper
        assert_eq!(
            convert_type_no_metadata("Nullable(Decimal(18, 6))")
                .expect("Failed to convert ClickHouse type to Arrow"),
            (DataType::Decimal128(18, 6), true)
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

    #[test]
    fn test_extract_identifier() {
        assert_eq!(extract_identifier("Decimal(10, 2)"), ("Decimal", "(10, 2)"));
        assert_eq!(extract_identifier("DateTime64(3)"), ("DateTime64", "(3)"));
        assert_eq!(extract_identifier("Int32"), ("Int32", ""));
        assert_eq!(
            extract_identifier("LowCardinality(String)"),
            ("LowCardinality", "(String)")
        );
        assert_eq!(extract_identifier("Decimal128(10)"), ("Decimal128", "(10)"));
    }

    #[test]
    fn test_parse_args() {
        // Simple cases
        assert_eq!(
            parse_args("(10, 2)").unwrap(),
            vec!["10".to_string(), "2".to_string()]
        );
        assert_eq!(parse_args("(3)").unwrap(), vec!["3".to_string()]);
        assert_eq!(parse_args("()").unwrap(), Vec::<String>::new());

        // With spaces
        assert_eq!(
            parse_args("( 10 , 2 )").unwrap(),
            vec!["10".to_string(), "2".to_string()]
        );

        // With nested parentheses
        assert_eq!(
            parse_args("(Nullable(String))").unwrap(),
            vec!["Nullable(String)".to_string()]
        );
        assert_eq!(
            parse_args("(Array(Int32), String)").unwrap(),
            vec!["Array(Int32)".to_string(), "String".to_string()]
        );

        // With quotes
        assert_eq!(
            parse_args("(3, 'UTC')").unwrap(),
            vec!["3".to_string(), "'UTC'".to_string()]
        );
        assert_eq!(
            parse_args("(9, 'America/New_York')").unwrap(),
            vec!["9".to_string(), "'America/New_York'".to_string()]
        );

        // Complex nested case
        assert_eq!(
            parse_args("(Tuple(Int32, String), Array(Float64))").unwrap(),
            vec![
                "Tuple(Int32, String)".to_string(),
                "Array(Float64)".to_string()
            ]
        );

        // Error cases
        assert!(parse_args("10, 2").is_err()); // Missing parentheses
        assert!(parse_args("(10, 2").is_err()); // Missing closing paren
    }

    #[test]
    fn test_array_type_not_supported() {
        // Array types should return an error
        let result = convert_type_no_metadata("Array(Int32)");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Array type"));
        assert!(err.contains("not supported"));
    }

    #[test]
    fn test_tuple_type_not_supported() {
        // Tuple types should return an error
        let result = convert_type_no_metadata("Tuple(String, Int64)");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Tuple type"));
        assert!(err.contains("not supported"));
    }

    #[test]
    fn test_map_type_not_supported() {
        // Map types should return an error
        let result = convert_type_no_metadata("Map(String, Int64)");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Map type"));
        assert!(err.contains("not supported"));
    }

    #[test]
    fn test_unknown_type_fails() {
        // Unknown types should return an error
        let result = convert_type_no_metadata("UnknownType");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Unknown ClickHouse type"));
    }
}
