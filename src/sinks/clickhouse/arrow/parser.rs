//! ClickHouse type parsing and conversion to Arrow types.

use arrow::datatypes::{DataType, Field, Fields, TimeUnit};
use std::sync::Arc;

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
    /// Array(T)
    Array(Box<ClickHouseType<'a>>),
    /// Tuple(T1, T2, ...)
    Tuple(Vec<ClickHouseType<'a>>),
    /// Map(K, V)
    Map(Box<ClickHouseType<'a>>, Box<ClickHouseType<'a>>),
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

    /// Converts this structured ClickHouseType to an Arrow DataType.
    /// Returns a tuple of (DataType, is_nullable).
    pub fn to_arrow(&self) -> Result<(DataType, bool), String> {
        let is_nullable = self.is_nullable();

        match self.base_type() {
            ClickHouseType::Primitive(name) => {
                let (type_name, _) = extract_identifier(name);
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
                        parse_decimal_type(name)?
                    }

                    // Strings
                    "String" | "FixedString" => DataType::Utf8,

                    // Date and time types (timezones not currently handled, defaults to UTC)
                    "Date" | "Date32" => DataType::Date32,
                    "DateTime" => DataType::Timestamp(TimeUnit::Second, None),
                    "DateTime64" => parse_datetime64_precision(name)?,

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
            ClickHouseType::Array(inner) => {
                let (inner_arrow, inner_nullable) = inner.to_arrow()?;
                let field = Field::new("item", inner_arrow, inner_nullable);
                Ok((DataType::List(Arc::new(field)), is_nullable))
            }
            ClickHouseType::Tuple(elements) => {
                let fields: Result<Vec<Field>, String> = elements
                    .iter()
                    .enumerate()
                    .map(|(i, elem)| {
                        let (elem_arrow, elem_nullable) = elem.to_arrow()?;
                        Ok(Field::new(format!("f{}", i), elem_arrow, elem_nullable))
                    })
                    .collect();
                Ok((DataType::Struct(Fields::from(fields?)), is_nullable))
            }
            ClickHouseType::Map(key_type, value_type) => {
                // Validate key is String
                let (key_arrow, _) = key_type.to_arrow()?;
                if !matches!(key_arrow, DataType::Utf8) {
                    return Err(
                        "Map keys must be String type. Vector's ObjectMap only supports String keys."
                            .to_string(),
                    );
                }

                // Recursively convert value type
                let (value_arrow, value_nullable) = value_type.to_arrow()?;

                // Arrow Map is represented as Map<String, T>
                let key_field = Field::new("keys", DataType::Utf8, false);
                let value_field = Field::new("values", value_arrow, value_nullable);
                let entries_struct = DataType::Struct(Fields::from(vec![key_field, value_field]));
                let entries_field = Field::new("entries", entries_struct, false);
                Ok((DataType::Map(Arc::new(entries_field), false), is_nullable))
            }
            _ => Err("Unsupported ClickHouse type".to_string()),
        }
    }
}

/// Parses a ClickHouse type string into a structured representation.
pub fn parse_ch_type(ty: &str) -> ClickHouseType<'_> {
    let ty = ty.trim();

    // Try to match type_name(args) pattern
    if let Some((type_name, args_str)) = try_parse_wrapper(ty) {
        match type_name {
            "Nullable" => {
                return ClickHouseType::Nullable(Box::new(parse_ch_type(args_str)));
            }
            "LowCardinality" => {
                return ClickHouseType::LowCardinality(Box::new(parse_ch_type(args_str)));
            }
            "Array" => {
                return ClickHouseType::Array(Box::new(parse_ch_type(args_str)));
            }
            "Tuple" => {
                let elements = parse_args(args_str)
                    .into_iter()
                    .map(|arg| parse_ch_type(arg))
                    .collect();
                return ClickHouseType::Tuple(elements);
            }
            "Map" => {
                let args = parse_args(args_str);
                if args.len() == 2 {
                    return ClickHouseType::Map(
                        Box::new(parse_ch_type(args[0])),
                        Box::new(parse_ch_type(args[1])),
                    );
                }
            }
            _ => {} // Fall through to primitive
        }
    }

    // Base case: return primitive type
    ClickHouseType::Primitive(ty)
}

/// Tries to parse "TypeName(args)" into ("TypeName", "args").
fn try_parse_wrapper(ty: &str) -> Option<(&str, &str)> {
    let paren_pos = ty.find('(')?;
    if !ty.ends_with(')') {
        return None;
    }

    let type_name = ty[..paren_pos].trim();
    let args = &ty[paren_pos + 1..ty.len() - 1];

    Some((type_name, args))
}

/// Parses comma-separated arguments, respecting nesting and quotes.
/// Handles input with or without surrounding parentheses.
/// Examples: "Int32, String" or "(Int32, String)" both work.
/// Parses comma-separated arguments, respecting nesting and quotes.
/// Handles input with or without surrounding parentheses.
/// Examples: "Int32, String" or "(Int32, String)" both work.
fn parse_args(input: &str) -> Vec<&str> {
    let input = input.trim();

    // Strip parentheses if present
    let input = if input.starts_with('(') && input.ends_with(')') {
        &input[1..input.len() - 1]
    } else {
        input
    };

    if input.is_empty() {
        return vec![];
    }

    let mut args = Vec::new();
    let mut start = 0;
    let mut depth = 0;
    let mut in_quotes = false;

    for (i, c) in input.char_indices() {
        match c {
            '\'' => in_quotes = !in_quotes,
            '(' if !in_quotes => depth += 1,
            ')' if !in_quotes => depth -= 1,
            ',' if depth == 0 && !in_quotes => {
                args.push(input[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }

    args.push(input[start..].trim());
    args
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

    let args = parse_args(args_str);
    let result = match type_name {
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
    };

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

    let args = parse_args(args_str);

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
        parse_ch_type(ch_type).to_arrow()
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
        // Simple cases with parentheses
        assert_eq!(parse_args("(10, 2)"), vec!["10", "2"]);
        assert_eq!(parse_args("(3)"), vec!["3"]);
        assert_eq!(parse_args("()"), Vec::<&str>::new());

        // Simple cases without parentheses (now supported)
        assert_eq!(parse_args("10, 2"), vec!["10", "2"]);
        assert_eq!(parse_args("3"), vec!["3"]);

        // With spaces
        assert_eq!(parse_args("( 10 , 2 )"), vec!["10", "2"]);

        // With nested parentheses
        assert_eq!(parse_args("(Nullable(String))"), vec!["Nullable(String)"]);
        assert_eq!(
            parse_args("(Array(Int32), String)"),
            vec!["Array(Int32)", "String"]
        );

        // With quotes
        assert_eq!(parse_args("(3, 'UTC')"), vec!["3", "'UTC'"]);
        assert_eq!(
            parse_args("(9, 'America/New_York')"),
            vec!["9", "'America/New_York'"]
        );

        // Complex nested case with multiple levels, modifiers, named tuples, and quotes
        assert_eq!(
            parse_args(
                "(Array(Tuple(id Int64, tags Array(String))), Map(String, Tuple(Nullable(Float64), LowCardinality(String))), String, DateTime('America/New_York'))"
            ),
            vec![
                "Array(Tuple(id Int64, tags Array(String)))",
                "Map(String, Tuple(Nullable(Float64), LowCardinality(String)))",
                "String",
                "DateTime('America/New_York')"
            ]
        );
    }

    #[test]
    fn test_array_type() {
        let result = convert_type_no_metadata("Array(Int32)");
        assert!(result.is_ok());
        let (data_type, is_nullable) = result.unwrap();
        assert!(!is_nullable);
        match data_type {
            DataType::List(field) => {
                assert_eq!(field.data_type(), &DataType::Int32);
                assert!(!field.is_nullable());
            }
            _ => panic!("Expected List type"),
        }
    }

    #[test]
    fn test_tuple_type() {
        let result = convert_type_no_metadata("Tuple(String, Int64)");
        assert!(result.is_ok());
        let (data_type, is_nullable) = result.unwrap();
        assert!(!is_nullable);
        match data_type {
            DataType::Struct(fields) => {
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].data_type(), &DataType::Utf8);
                assert_eq!(fields[1].data_type(), &DataType::Int64);
            }
            _ => panic!("Expected Struct type"),
        }
    }

    #[test]
    fn test_map_type() {
        let result = convert_type_no_metadata("Map(String, Int64)");
        assert!(result.is_ok());
        let (data_type, is_nullable) = result.unwrap();
        assert!(!is_nullable);
        match data_type {
            DataType::Map(_, _) => {}
            _ => panic!("Expected Map type"),
        }
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

    #[test]
    fn test_array_type_parsing() {
        // Simple array
        let result = convert_type_no_metadata("Array(Int32)");
        assert!(result.is_ok());
        let (dtype, nullable) = result.unwrap();
        assert!(matches!(dtype, DataType::List(_)));
        assert!(!nullable);

        // Nested array
        let result = convert_type_no_metadata("Array(Array(String))");
        assert!(result.is_ok());
        let (dtype, _) = result.unwrap();
        if let DataType::List(inner) = dtype {
            assert!(matches!(inner.data_type(), DataType::List(_)));
        } else {
            panic!("Expected List type");
        }

        // Nullable array
        let result = convert_type_no_metadata("Nullable(Array(Int64))");
        assert!(result.is_ok());
        let (_, nullable) = result.unwrap();
        assert!(nullable);
    }

    #[test]
    fn test_tuple_type_parsing() {
        // Simple tuple
        let result = convert_type_no_metadata("Tuple(String, Int64)");
        assert!(result.is_ok());
        let (dtype, _) = result.unwrap();
        if let DataType::Struct(fields) = dtype {
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].name(), "f0");
            assert_eq!(fields[1].name(), "f1");
            assert!(matches!(fields[0].data_type(), DataType::Utf8));
            assert!(matches!(fields[1].data_type(), DataType::Int64));
        } else {
            panic!("Expected Struct type");
        }

        // Nested tuple
        let result = convert_type_no_metadata("Tuple(Int32, Tuple(String, Float64))");
        assert!(result.is_ok());
        let (dtype, _) = result.unwrap();
        if let DataType::Struct(fields) = dtype {
            assert_eq!(fields.len(), 2);
            assert!(matches!(fields[1].data_type(), DataType::Struct(_)));
        } else {
            panic!("Expected Struct type");
        }
    }

    #[test]
    fn test_map_type_parsing() {
        // Simple map
        let result = convert_type_no_metadata("Map(String, Int64)");
        assert!(result.is_ok());
        let (dtype, _) = result.unwrap();
        assert!(matches!(dtype, DataType::Map(_, _)));

        // Map with complex value
        let result = convert_type_no_metadata("Map(String, Array(Int32))");
        assert!(result.is_ok());
        let (dtype, _) = result.unwrap();
        if let DataType::Map(entries, _) = dtype
            && let DataType::Struct(fields) = entries.data_type()
        {
            let value_field = &fields[1];
            assert!(matches!(value_field.data_type(), DataType::List(_)));
        }

        // Non-string key should error
        let result = convert_type_no_metadata("Map(Int32, String)");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Map keys must be String"));
    }

    #[test]
    fn test_complex_nested_types() {
        // Array of tuples
        let result = convert_type_no_metadata("Array(Tuple(String, Int64))");
        assert!(result.is_ok());
        let (dtype, _) = result.unwrap();
        if let DataType::List(inner) = dtype {
            assert!(matches!(inner.data_type(), DataType::Struct(_)));
        } else {
            panic!("Expected List type");
        }

        // Tuple with array and map
        let result = convert_type_no_metadata("Tuple(Array(Int32), Map(String, Float64))");
        assert!(result.is_ok());
        let (dtype, _) = result.unwrap();
        if let DataType::Struct(fields) = dtype {
            assert_eq!(fields.len(), 2);
            assert!(matches!(fields[0].data_type(), DataType::List(_)));
            assert!(matches!(fields[1].data_type(), DataType::Map(_, _)));
        } else {
            panic!("Expected Struct type");
        }

        // Map with tuple values
        let result = convert_type_no_metadata("Map(String, Tuple(Int64, String))");
        assert!(result.is_ok());
        let (dtype, _) = result.unwrap();
        if let DataType::Map(entries, _) = dtype
            && let DataType::Struct(fields) = entries.data_type()
        {
            let value_field = &fields[1];
            assert!(matches!(value_field.data_type(), DataType::Struct(_)));
        }
    }
}
