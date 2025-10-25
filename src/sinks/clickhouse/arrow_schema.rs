//! Schema fetching and Arrow type mapping for ClickHouse tables.

use std::sync::Arc;

use ::arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use http::{Request, StatusCode};
use hyper::Body;
use serde::Deserialize;

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
            let body_str = String::from_utf8(body_bytes.into())
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

/// Unwraps ClickHouse type modifiers like Nullable() and LowCardinality().
/// For example: "Nullable(LowCardinality(String))" -> "String"
fn unwrap_type_modifiers(ch_type: &str) -> &str {
    let mut base = ch_type;
    for prefix in ["Nullable(", "LowCardinality("] {
        if let Some(inner) = base.strip_prefix(prefix) {
            base = inner.strip_suffix(')').unwrap_or(inner);
        }
    }
    base
}

fn clickhouse_type_to_arrow(ch_type: &str) -> DataType {
    let base_type = unwrap_type_modifiers(ch_type);

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
        // Timezones are not currently handled, defaults to UTC
        "DateTime" => DataType::Timestamp(TimeUnit::Second, None),
        _ if base_type.starts_with("DateTime64") => parse_datetime64_precision(base_type),
        _ if base_type.starts_with("Decimal") => parse_decimal_type(base_type),
        _ if base_type.starts_with("Array") => DataType::Utf8,
        _ if base_type.starts_with("Map") => DataType::Utf8,
        _ if base_type.starts_with("Tuple") => DataType::Utf8,
        _ => {
            warn!("Unknown ClickHouse type '{}', defaulting to Utf8", ch_type);
            DataType::Utf8
        }
    }
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
fn parse_decimal_type(ch_type: &str) -> DataType {
    let (type_name, args_str) = extract_identifier(ch_type);

    let args = match parse_args(args_str) {
        Ok(args) => args,
        Err(_) => {
            warn!("Could not parse Decimal type '{}'", ch_type);
            return DataType::Null;
        }
    };

    match type_name {
        "Decimal" if args.len() == 2 => {
            // Decimal(P, S) format
            if let (Ok(precision), Ok(scale)) = (args[0].parse::<u8>(), args[1].parse::<i8>()) {
                return if precision <= 38 {
                    DataType::Decimal128(precision, scale)
                } else {
                    DataType::Decimal256(precision, scale)
                };
            }
        }
        "Decimal32" | "Decimal64" | "Decimal128" | "Decimal256" if args.len() == 1 => {
            if let Ok(scale) = args[0].parse::<i8>() {
                let precision = match type_name {
                    "Decimal32" => 9,
                    "Decimal64" => 18,
                    "Decimal128" => 38,
                    "Decimal256" => 76,
                    _ => unreachable!(),
                };
                return if precision <= 38 {
                    DataType::Decimal128(precision, scale)
                } else {
                    DataType::Decimal256(precision, scale)
                };
            }
        }
        _ => {}
    }

    warn!("Could not parse Decimal type '{}'", ch_type);
    DataType::Null
}

/// Parses DateTime64 precision and returns the appropriate Arrow timestamp type.
/// DateTime64(0) -> Second
/// DateTime64(3) -> Millisecond
/// DateTime64(6) -> Microsecond
/// DateTime64(9) -> Nanosecond
fn parse_datetime64_precision(ch_type: &str) -> DataType {
    let (_type_name, args_str) = extract_identifier(ch_type);

    let args = match parse_args(args_str) {
        Ok(args) => args,
        Err(_) => {
            warn!("Could not parse DateTime64 precision from '{}'", ch_type);
            return DataType::Null;
        }
    };

    // DateTime64(precision) or DateTime64(precision, 'timezone')
    if args.is_empty() {
        warn!("Could not parse DateTime64 precision from '{}'", ch_type);
        return DataType::Null;
    }

    // Parse the precision (first argument)
    match args[0].parse::<u8>() {
        Ok(0) => DataType::Timestamp(TimeUnit::Second, None),
        Ok(1..=3) => DataType::Timestamp(TimeUnit::Millisecond, None),
        Ok(4..=6) => DataType::Timestamp(TimeUnit::Microsecond, None),
        Ok(7..=9) => DataType::Timestamp(TimeUnit::Nanosecond, None),
        _ => {
            warn!("Unsupported DateTime64 precision in '{}'", ch_type);
            DataType::Null
        }
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
    fn test_decimal_type_mapping() {
        // Generic Decimal(P, S)
        assert_eq!(
            clickhouse_type_to_arrow("Decimal(10, 2)"),
            DataType::Decimal128(10, 2)
        );
        assert_eq!(
            clickhouse_type_to_arrow("Decimal(38, 6)"),
            DataType::Decimal128(38, 6)
        );
        assert_eq!(
            clickhouse_type_to_arrow("Decimal(50, 10)"),
            DataType::Decimal256(50, 10)
        );

        // Decimal32(S) - precision up to 9
        assert_eq!(
            clickhouse_type_to_arrow("Decimal32(2)"),
            DataType::Decimal128(9, 2)
        );

        // Decimal64(S) - precision up to 18
        assert_eq!(
            clickhouse_type_to_arrow("Decimal64(4)"),
            DataType::Decimal128(18, 4)
        );

        // Decimal128(S) - precision up to 38
        assert_eq!(
            clickhouse_type_to_arrow("Decimal128(10)"),
            DataType::Decimal128(38, 10)
        );

        // Decimal256(S) - precision up to 76
        assert_eq!(
            clickhouse_type_to_arrow("Decimal256(20)"),
            DataType::Decimal256(76, 20)
        );

        // With Nullable wrapper
        assert_eq!(
            clickhouse_type_to_arrow("Nullable(Decimal(18, 6))"),
            DataType::Decimal128(18, 6)
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
    fn test_decimal_type_parsing_with_new_parser() {
        // Generic Decimal(P, S) - edge cases
        assert_eq!(
            clickhouse_type_to_arrow("Decimal(10,2)"),
            DataType::Decimal128(10, 2)
        );
        assert_eq!(
            clickhouse_type_to_arrow("Decimal( 18 , 6 )"),
            DataType::Decimal128(18, 6)
        );

        // Sized decimal types without spaces
        assert_eq!(
            clickhouse_type_to_arrow("Decimal32(4)"),
            DataType::Decimal128(9, 4)
        );
        assert_eq!(
            clickhouse_type_to_arrow("Decimal64(8)"),
            DataType::Decimal128(18, 8)
        );
    }

    #[test]
    fn test_datetime64_parsing_with_new_parser() {
        // Without timezone
        assert_eq!(
            clickhouse_type_to_arrow("DateTime64(3)"),
            DataType::Timestamp(TimeUnit::Millisecond, None)
        );

        // With timezone (should ignore timezone for now)
        assert_eq!(
            clickhouse_type_to_arrow("DateTime64(6, 'UTC')"),
            DataType::Timestamp(TimeUnit::Microsecond, None)
        );
        assert_eq!(
            clickhouse_type_to_arrow("DateTime64(9, 'America/New_York')"),
            DataType::Timestamp(TimeUnit::Nanosecond, None)
        );

        // With spaces
        assert_eq!(
            clickhouse_type_to_arrow("DateTime64( 3 )"),
            DataType::Timestamp(TimeUnit::Millisecond, None)
        );
    }
}
