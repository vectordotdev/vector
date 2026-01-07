//! ClickHouse type parsing and conversion to Arrow types.

use arrow::datatypes::{DataType, TimeUnit};

const DECIMAL32_PRECISION: u8 = 9;
const DECIMAL64_PRECISION: u8 = 18;
const DECIMAL128_PRECISION: u8 = 38;
const DECIMAL256_PRECISION: u8 = 76;

/// Represents a ClickHouse type with its modifiers and nested structure.
#[derive(Debug, PartialEq, Clone)]
pub enum ClickHouseType<'a> {
    /// A primitive type like String, Int64, DateTime, etc.
    Primitive(&'a str),
    /// Nullable(T)
    Nullable(Box<ClickHouseType<'a>>),
    /// LowCardinality(T)
    LowCardinality(Box<ClickHouseType<'a>>),
}

impl<'a> ClickHouseType<'a> {
    /// Returns true if this type or any of its nested types is Nullable.
    pub fn is_nullable(&self) -> bool {
        match self {
            ClickHouseType::Nullable(_) => true,
            ClickHouseType::LowCardinality(inner) => inner.is_nullable(),
            _ => false,
        }
    }

    /// Returns the innermost base type, unwrapping all modifiers.
    /// For example: LowCardinality(Nullable(String)) -> Primitive("String")
    pub fn base_type(&self) -> &ClickHouseType<'a> {
        match self {
            ClickHouseType::Nullable(inner) | ClickHouseType::LowCardinality(inner) => {
                inner.base_type()
            }
            _ => self,
        }
    }
}

/// Parses a ClickHouse type string into a structured representation.
pub fn parse_ch_type(ty: &str) -> ClickHouseType<'_> {
    let ty = ty.trim();

    // Recursively strip and parse type modifiers
    if let Some(inner) = strip_wrapper(ty, "Nullable") {
        return ClickHouseType::Nullable(Box::new(parse_ch_type(inner)));
    }
    if let Some(inner) = strip_wrapper(ty, "LowCardinality") {
        return ClickHouseType::LowCardinality(Box::new(parse_ch_type(inner)));
    }

    // Base case: return primitive type for anything without modifiers
    ClickHouseType::Primitive(ty)
}

/// Helper function to strip a wrapper from a type string.
/// Returns the inner content if the type matches the wrapper pattern.
fn strip_wrapper<'a>(ty: &'a str, wrapper_name: &str) -> Option<&'a str> {
    ty.strip_prefix(wrapper_name)?
        .trim_start()
        .strip_prefix('(')?
        .strip_suffix(')')
}

/// Unwraps ClickHouse type modifiers like Nullable() and LowCardinality().
/// Returns a tuple of (base_type, is_nullable).
/// For example: "LowCardinality(Nullable(String))" -> ("String", true)
pub fn unwrap_type_modifiers(ch_type: &str) -> (&str, bool) {
    let parsed = parse_ch_type(ch_type);
    let is_nullable = parsed.is_nullable();

    match parsed.base_type() {
        ClickHouseType::Primitive(base) => (base, is_nullable),
        _ => (ch_type, is_nullable),
    }
}

fn unsupported(ch_type: &str, kind: &str) -> String {
    format!(
        "{kind} type '{ch_type}' is not supported. \
         ClickHouse {kind} types cannot be automatically converted to Arrow format."
    )
}

/// Converts a ClickHouse type string to an Arrow DataType.
/// Returns a tuple of (DataType, is_nullable).
pub fn clickhouse_type_to_arrow(ch_type: &str) -> Result<(DataType, bool), String> {
    let (base_type, is_nullable) = unwrap_type_modifiers(ch_type);
    let (type_name, _) = extract_identifier(base_type);

    let data_type = match type_name {
        // Numeric
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
        "Decimal" | "Decimal32" | "Decimal64" | "Decimal128" | "Decimal256" => {
            parse_decimal_type(base_type)?
        }

        // Strings
        "String" | "FixedString" => DataType::Utf8,

        // Date and time types (timezones not currently handled, defaults to UTC)
        "Date" | "Date32" => DataType::Date32,
        "DateTime" => DataType::Timestamp(TimeUnit::Second, None),
        "DateTime64" => parse_datetime64_precision(base_type)?,

        // Unsupported
        "Array" => return Err(unsupported(ch_type, "Array")),
        "Tuple" => return Err(unsupported(ch_type, "Tuple")),
        "Map" => return Err(unsupported(ch_type, "Map")),

        // Unknown
        _ => {
            return Err(format!(
                "Unknown ClickHouse type '{}'. This type cannot be automatically converted.",
                type_name
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
        .ok_or_else(|| format!("Could not parse Decimal type '{}'.", ch_type))
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
            convert_type_no_metadata("LowCardinality(Nullable(String))")
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

    #[test]
    fn test_parse_ch_type_primitives() {
        assert_eq!(parse_ch_type("String"), ClickHouseType::Primitive("String"));
        assert_eq!(parse_ch_type("Int64"), ClickHouseType::Primitive("Int64"));
        assert_eq!(
            parse_ch_type("DateTime64(3)"),
            ClickHouseType::Primitive("DateTime64(3)")
        );
    }

    #[test]
    fn test_parse_ch_type_nullable() {
        assert_eq!(
            parse_ch_type("Nullable(String)"),
            ClickHouseType::Nullable(Box::new(ClickHouseType::Primitive("String")))
        );
        assert_eq!(
            parse_ch_type("Nullable(Int64)"),
            ClickHouseType::Nullable(Box::new(ClickHouseType::Primitive("Int64")))
        );
    }

    #[test]
    fn test_parse_ch_type_lowcardinality() {
        assert_eq!(
            parse_ch_type("LowCardinality(String)"),
            ClickHouseType::LowCardinality(Box::new(ClickHouseType::Primitive("String")))
        );
        assert_eq!(
            parse_ch_type("LowCardinality(Nullable(String))"),
            ClickHouseType::LowCardinality(Box::new(ClickHouseType::Nullable(Box::new(
                ClickHouseType::Primitive("String")
            ))))
        );
    }

    #[test]
    fn test_parse_ch_type_is_nullable() {
        assert!(!parse_ch_type("String").is_nullable());
        assert!(parse_ch_type("Nullable(String)").is_nullable());
        assert!(parse_ch_type("LowCardinality(Nullable(String))").is_nullable());
        assert!(!parse_ch_type("LowCardinality(String)").is_nullable());
    }

    #[test]
    fn test_parse_ch_type_base_type() {
        let parsed = parse_ch_type("LowCardinality(Nullable(String))");
        assert_eq!(parsed.base_type(), &ClickHouseType::Primitive("String"));

        let parsed = parse_ch_type("Nullable(Int64)");
        assert_eq!(parsed.base_type(), &ClickHouseType::Primitive("Int64"));

        let parsed = parse_ch_type("String");
        assert_eq!(parsed.base_type(), &ClickHouseType::Primitive("String"));
    }
}
