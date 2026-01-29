//! ClickHouse type parsing and conversion to Arrow types.

use arrow::datatypes::{DataType, Field, Fields, TimeUnit};
use itertools::Itertools;
use nom::{
    IResult, Parser,
    bytes::complete::{tag, take_till, take_while1},
    character::complete::{char, i8 as parse_i8, u8 as parse_u8, u32 as parse_u32},
    combinator::{all_consuming, cut, opt},
    multi::separated_list0,
    sequence::{delimited, preceded, separated_pair, terminated},
};

use nom::error::{Error, ErrorKind};

const DECIMAL32_PRECISION: u8 = 9;
const DECIMAL64_PRECISION: u8 = 18;
const DECIMAL128_PRECISION: u8 = 38;
const DECIMAL256_PRECISION: u8 = 76;

/// Represents a ClickHouse type with its modifiers and nested structure.
#[derive(Debug, PartialEq, Clone)]
pub enum ClickHouseType<'a> {
    // Numeric types
    Int8,
    Int16,
    Int32,
    Int64,
    UInt8,
    UInt16,
    UInt32,
    UInt64,
    Float32,
    Float64,
    Bool,

    // Decimal with precision and scale
    Decimal { precision: u8, scale: i8 },

    // String types
    String,
    FixedString(u32),

    // Date/time types
    Date,
    DateTime,
    DateTime64 { precision: u8 },

    // Wrapper types
    Nullable(Box<ClickHouseType<'a>>),
    LowCardinality(Box<ClickHouseType<'a>>),
    Array(Box<ClickHouseType<'a>>),
    Tuple(Vec<(Option<&'a str>, ClickHouseType<'a>)>),
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
    /// For example: LowCardinality(Nullable(String)) -> String
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

        let data_type = match self.base_type() {
            // Numeric types
            ClickHouseType::Int8 => DataType::Int8,
            ClickHouseType::Int16 => DataType::Int16,
            ClickHouseType::Int32 => DataType::Int32,
            ClickHouseType::Int64 => DataType::Int64,
            ClickHouseType::UInt8 => DataType::UInt8,
            ClickHouseType::UInt16 => DataType::UInt16,
            ClickHouseType::UInt32 => DataType::UInt32,
            ClickHouseType::UInt64 => DataType::UInt64,
            ClickHouseType::Float32 => DataType::Float32,
            ClickHouseType::Float64 => DataType::Float64,
            ClickHouseType::Bool => DataType::Boolean,

            // Decimal
            ClickHouseType::Decimal { precision, scale } => {
                if *precision <= DECIMAL128_PRECISION {
                    DataType::Decimal128(*precision, *scale)
                } else {
                    DataType::Decimal256(*precision, *scale)
                }
            }

            // String types
            ClickHouseType::String | ClickHouseType::FixedString(_) => DataType::Utf8,

            // Date/time types
            ClickHouseType::Date => DataType::Date32,
            ClickHouseType::DateTime => DataType::Timestamp(TimeUnit::Second, None),
            ClickHouseType::DateTime64 { precision } => match precision {
                0 => DataType::Timestamp(TimeUnit::Second, None),
                1..=3 => DataType::Timestamp(TimeUnit::Millisecond, None),
                4..=6 => DataType::Timestamp(TimeUnit::Microsecond, None),
                7..=9 => DataType::Timestamp(TimeUnit::Nanosecond, None),
                _ => {
                    return Err(format!(
                        "Unsupported DateTime64 precision {}. Must be 0-9",
                        precision
                    ));
                }
            },

            // Container types
            ClickHouseType::Array(inner) => {
                let (inner_arrow, inner_nullable) = inner.to_arrow()?;
                DataType::List(Field::new("item", inner_arrow, inner_nullable).into())
            }
            ClickHouseType::Tuple(elements) => {
                let fields: Vec<Field> = elements
                    .iter()
                    .enumerate()
                    .map(|(i, (name_opt, elem))| {
                        let (dt, nullable) = elem.to_arrow()?;
                        let name = name_opt.map_or_else(|| format!("f{i}"), str::to_owned);
                        Ok::<_, String>(Field::new(name, dt, nullable))
                    })
                    .try_collect()?;
                DataType::Struct(Fields::from(fields))
            }
            ClickHouseType::Map(key_type, value_type) => {
                let (key_arrow, _) = key_type.to_arrow()?;
                if !matches!(key_arrow, DataType::Utf8) {
                    return Err("Map keys must be String type.".to_string());
                }
                let (value_arrow, value_nullable) = value_type.to_arrow()?;
                let entries = DataType::Struct(Fields::from(vec![
                    Field::new("keys", DataType::Utf8, false),
                    Field::new("values", value_arrow, value_nullable),
                ]));
                DataType::Map(Field::new("entries", entries, false).into(), false)
            }

            // base_type() always unwraps Nullable/LowCardinality
            ClickHouseType::Nullable(_) | ClickHouseType::LowCardinality(_) => unreachable!(),
        };

        Ok((data_type, is_nullable))
    }
}

/// Wraps a parser in parentheses with cut (no backtracking after open paren).
fn parens<'a, O>(
    inner: impl Parser<&'a str, Output = O, Error = nom::error::Error<&'a str>>,
) -> impl Parser<&'a str, Output = O, Error = nom::error::Error<&'a str>> {
    delimited(char('('), cut(inner), char(')'))
}

/// Parses an identifier (alphanumeric + underscore, at least one char).
fn identifier(input: &str) -> IResult<&str, &str> {
    take_while1(|c: char| c.is_alphanumeric() || c == '_')(input)
}

/// Parses a single tuple element (either "Type" or "name Type").
fn tuple_element(input: &str) -> IResult<&str, (Option<&str>, ClickHouseType<'_>)> {
    let (rest, name) = identifier(input)?;
    match rest.strip_prefix(' ') {
        Some(after_space) => {
            let (rest, ty) = ch_type(after_space)?;
            Ok((rest, (Some(name), ty)))
        }
        None => {
            // No space after identifier, so re-parse as a type
            let (rest, ty) = ch_type(input)?;
            Ok((rest, (None, ty)))
        }
    }
}

/// Parses a complete ClickHouse type.
///
/// Nom parsers return `(rest, output)` where `rest` is the remaining unparsed input.
/// For example, parsing `"Array(String)"`:
///   - `identifier` consumes `"Array"`, returns `rest = "(String)"`, `name = "Array"`
///   - The `"Array"` match arm then parses `rest` with `parens(ch_type)`
fn ch_type(input: &str) -> IResult<&str, ClickHouseType<'_>> {
    let (rest, name) = identifier(input)?;

    match name {
        // Wrapper types
        "Nullable" => parens(ch_type)
            .map(|t| ClickHouseType::Nullable(Box::new(t)))
            .parse(rest),
        "LowCardinality" => parens(ch_type)
            .map(|t| ClickHouseType::LowCardinality(Box::new(t)))
            .parse(rest),
        "Array" => parens(ch_type)
            .map(|t| ClickHouseType::Array(Box::new(t)))
            .parse(rest),
        "Map" => parens(separated_pair(ch_type, tag(", "), ch_type))
            .map(|(k, v)| ClickHouseType::Map(Box::new(k), Box::new(v)))
            .parse(rest),
        "Tuple" => parens(separated_list0(tag(", "), tuple_element))
            .map(ClickHouseType::Tuple)
            .parse(rest),

        // Numeric types
        "Int8" => Ok((rest, ClickHouseType::Int8)),
        "Int16" => Ok((rest, ClickHouseType::Int16)),
        "Int32" => Ok((rest, ClickHouseType::Int32)),
        "Int64" => Ok((rest, ClickHouseType::Int64)),
        "UInt8" => Ok((rest, ClickHouseType::UInt8)),
        "UInt16" => Ok((rest, ClickHouseType::UInt16)),
        "UInt32" => Ok((rest, ClickHouseType::UInt32)),
        "UInt64" => Ok((rest, ClickHouseType::UInt64)),
        "Float32" => Ok((rest, ClickHouseType::Float32)),
        "Float64" => Ok((rest, ClickHouseType::Float64)),
        "Bool" => Ok((rest, ClickHouseType::Bool)),

        // String types
        "String" => Ok((rest, ClickHouseType::String)),
        "FixedString" => parens(parse_u32)
            .map(ClickHouseType::FixedString)
            .parse(rest),

        // Date/time types
        "Date" | "Date32" => Ok((rest, ClickHouseType::Date)),
        "DateTime" => Ok((rest, ClickHouseType::DateTime)),
        "DateTime64" => {
            let tz = delimited(char('\''), take_till(|c| c == '\''), char('\''));
            parens(terminated(parse_u8, opt(preceded(tag(", "), tz))))
                .map(|p| ClickHouseType::DateTime64 { precision: p })
                .parse(rest)
        }

        // Decimal types
        "Decimal" => parens(separated_pair(parse_u8, tag(", "), parse_i8))
            .map(|(precision, scale)| ClickHouseType::Decimal { precision, scale })
            .parse(rest),
        "Decimal32" => parens(parse_i8)
            .map(|scale| ClickHouseType::Decimal {
                precision: DECIMAL32_PRECISION,
                scale,
            })
            .parse(rest),
        "Decimal64" => parens(parse_i8)
            .map(|scale| ClickHouseType::Decimal {
                precision: DECIMAL64_PRECISION,
                scale,
            })
            .parse(rest),
        "Decimal128" => parens(parse_i8)
            .map(|scale| ClickHouseType::Decimal {
                precision: DECIMAL128_PRECISION,
                scale,
            })
            .parse(rest),
        "Decimal256" => parens(parse_i8)
            .map(|scale| ClickHouseType::Decimal {
                precision: DECIMAL256_PRECISION,
                scale,
            })
            .parse(rest),

        _ => Err(nom::Err::Error(Error::new(input, ErrorKind::Tag))),
    }
}

/// Parses a ClickHouse type string into a structured representation.
pub fn parse_ch_type(ty: &str) -> Result<ClickHouseType<'_>, String> {
    all_consuming(ch_type)
        .parse(ty)
        .map(|(_, parsed)| parsed)
        .map_err(|e| format!("Failed to parse ClickHouse type '{}': {}", ty, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper function for tests
    fn convert_type(ch_type: &str) -> Result<(DataType, bool), String> {
        parse_ch_type(ch_type)?.to_arrow()
    }

    #[test]
    fn test_clickhouse_type_mapping() {
        assert_eq!(convert_type("String").unwrap(), (DataType::Utf8, false));
        assert_eq!(convert_type("Int64").unwrap(), (DataType::Int64, false));
        assert_eq!(convert_type("Float64").unwrap(), (DataType::Float64, false));
        assert_eq!(convert_type("Bool").unwrap(), (DataType::Boolean, false));
        assert_eq!(
            convert_type("DateTime").unwrap(),
            (DataType::Timestamp(TimeUnit::Second, None), false)
        );
    }

    #[test]
    fn test_datetime64_precision_mapping() {
        assert_eq!(
            convert_type("DateTime64(0)").unwrap(),
            (DataType::Timestamp(TimeUnit::Second, None), false)
        );
        assert_eq!(
            convert_type("DateTime64(3)").unwrap(),
            (DataType::Timestamp(TimeUnit::Millisecond, None), false)
        );
        assert_eq!(
            convert_type("DateTime64(6)").unwrap(),
            (DataType::Timestamp(TimeUnit::Microsecond, None), false)
        );
        assert_eq!(
            convert_type("DateTime64(9)").unwrap(),
            (DataType::Timestamp(TimeUnit::Nanosecond, None), false)
        );
        // Test with timezones (ignored)
        assert_eq!(
            convert_type("DateTime64(9, 'UTC')").unwrap(),
            (DataType::Timestamp(TimeUnit::Nanosecond, None), false)
        );
        assert_eq!(
            convert_type("DateTime64(6, 'America/New_York')").unwrap(),
            (DataType::Timestamp(TimeUnit::Microsecond, None), false)
        );
        // Edge cases
        assert_eq!(
            convert_type("DateTime64(1)").unwrap(),
            (DataType::Timestamp(TimeUnit::Millisecond, None), false)
        );
        assert_eq!(
            convert_type("DateTime64(4)").unwrap(),
            (DataType::Timestamp(TimeUnit::Microsecond, None), false)
        );
        assert_eq!(
            convert_type("DateTime64(7)").unwrap(),
            (DataType::Timestamp(TimeUnit::Nanosecond, None), false)
        );
    }

    #[test]
    fn test_nullable_type_mapping() {
        assert_eq!(convert_type("String").unwrap(), (DataType::Utf8, false));
        assert_eq!(
            convert_type("Nullable(String)").unwrap(),
            (DataType::Utf8, true)
        );
        assert_eq!(
            convert_type("Nullable(Int64)").unwrap(),
            (DataType::Int64, true)
        );
        assert_eq!(
            convert_type("Nullable(Float64)").unwrap(),
            (DataType::Float64, true)
        );
    }

    #[test]
    fn test_lowcardinality_type_mapping() {
        assert_eq!(
            convert_type("LowCardinality(String)").unwrap(),
            (DataType::Utf8, false)
        );
        assert_eq!(
            convert_type("LowCardinality(FixedString(10))").unwrap(),
            (DataType::Utf8, false)
        );
        assert_eq!(
            convert_type("LowCardinality(Nullable(String))").unwrap(),
            (DataType::Utf8, true)
        );
    }

    #[test]
    fn test_decimal_type_mapping() {
        // Decimal(P, S)
        assert_eq!(
            convert_type("Decimal(10, 2)").unwrap(),
            (DataType::Decimal128(10, 2), false)
        );
        assert_eq!(
            convert_type("Decimal(38, 6)").unwrap(),
            (DataType::Decimal128(38, 6), false)
        );
        assert_eq!(
            convert_type("Decimal(50, 10)").unwrap(),
            (DataType::Decimal256(50, 10), false)
        );

        // Decimal32(S) - precision 9
        assert_eq!(
            convert_type("Decimal32(2)").unwrap(),
            (DataType::Decimal128(9, 2), false)
        );

        // Decimal64(S) - precision 18
        assert_eq!(
            convert_type("Decimal64(4)").unwrap(),
            (DataType::Decimal128(18, 4), false)
        );

        // Decimal128(S) - precision 38
        assert_eq!(
            convert_type("Decimal128(10)").unwrap(),
            (DataType::Decimal128(38, 10), false)
        );

        // Decimal256(S) - precision 76
        assert_eq!(
            convert_type("Decimal256(20)").unwrap(),
            (DataType::Decimal256(76, 20), false)
        );

        // Nullable
        assert_eq!(
            convert_type("Nullable(Decimal(18, 6))").unwrap(),
            (DataType::Decimal128(18, 6), true)
        );
    }

    #[test]
    fn test_array_type() {
        let (data_type, is_nullable) = convert_type("Array(Int32)").unwrap();
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
        let (data_type, is_nullable) = convert_type("Tuple(String, Int64)").unwrap();
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
        let (data_type, is_nullable) = convert_type("Map(String, Int64)").unwrap();
        assert!(!is_nullable);
        assert!(matches!(data_type, DataType::Map(_, _)));
    }

    #[test]
    fn test_unknown_type_fails() {
        let result = convert_type("UnknownType");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_ch_type_primitives() {
        assert_eq!(parse_ch_type("String").unwrap(), ClickHouseType::String);
        assert_eq!(parse_ch_type("Int64").unwrap(), ClickHouseType::Int64);
        assert_eq!(
            parse_ch_type("DateTime64(3)").unwrap(),
            ClickHouseType::DateTime64 { precision: 3 }
        );
    }

    #[test]
    fn test_parse_ch_type_nullable() {
        assert_eq!(
            parse_ch_type("Nullable(String)").unwrap(),
            ClickHouseType::Nullable(Box::new(ClickHouseType::String))
        );
        assert_eq!(
            parse_ch_type("Nullable(Int64)").unwrap(),
            ClickHouseType::Nullable(Box::new(ClickHouseType::Int64))
        );
    }

    #[test]
    fn test_parse_ch_type_lowcardinality() {
        assert_eq!(
            parse_ch_type("LowCardinality(String)").unwrap(),
            ClickHouseType::LowCardinality(Box::new(ClickHouseType::String))
        );
        assert_eq!(
            parse_ch_type("LowCardinality(Nullable(String))").unwrap(),
            ClickHouseType::LowCardinality(Box::new(ClickHouseType::Nullable(Box::new(
                ClickHouseType::String
            ))))
        );
    }

    #[test]
    fn test_parse_ch_type_is_nullable() {
        assert!(!parse_ch_type("String").unwrap().is_nullable());
        assert!(parse_ch_type("Nullable(String)").unwrap().is_nullable());
        assert!(
            parse_ch_type("LowCardinality(Nullable(String))")
                .unwrap()
                .is_nullable()
        );
        assert!(
            !parse_ch_type("LowCardinality(String)")
                .unwrap()
                .is_nullable()
        );
    }

    #[test]
    fn test_parse_ch_type_base_type() {
        let parsed = parse_ch_type("LowCardinality(Nullable(String))").unwrap();
        assert_eq!(parsed.base_type(), &ClickHouseType::String);

        let parsed = parse_ch_type("Nullable(Int64)").unwrap();
        assert_eq!(parsed.base_type(), &ClickHouseType::Int64);

        let parsed = parse_ch_type("String").unwrap();
        assert_eq!(parsed.base_type(), &ClickHouseType::String);
    }

    #[test]
    fn test_array_type_parsing() {
        let (dtype, nullable) = convert_type("Array(Int32)").unwrap();
        assert!(matches!(dtype, DataType::List(_)));
        assert!(!nullable);

        // Nested array
        let (dtype, _) = convert_type("Array(Array(String))").unwrap();
        if let DataType::List(inner) = dtype {
            assert!(matches!(inner.data_type(), DataType::List(_)));
        } else {
            panic!("Expected List type");
        }

        // Nullable array
        let (_, nullable) = convert_type("Nullable(Array(Int64))").unwrap();
        assert!(nullable);
    }

    #[test]
    fn test_tuple_type_parsing() {
        let (dtype, _) = convert_type("Tuple(String, Int64)").unwrap();
        if let DataType::Struct(fields) = dtype {
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].name(), "f0");
            assert_eq!(fields[1].name(), "f1");
        } else {
            panic!("Expected Struct type");
        }

        // Nested tuple
        let (dtype, _) = convert_type("Tuple(Int32, Tuple(String, Float64))").unwrap();
        if let DataType::Struct(fields) = dtype {
            assert_eq!(fields.len(), 2);
            assert!(matches!(fields[1].data_type(), DataType::Struct(_)));
        } else {
            panic!("Expected Struct type");
        }
    }

    #[test]
    fn test_map_type_parsing() {
        let (dtype, _) = convert_type("Map(String, Int64)").unwrap();
        assert!(matches!(dtype, DataType::Map(_, _)));

        // Map with complex value
        let (dtype, _) = convert_type("Map(String, Array(Int32))").unwrap();
        if let DataType::Map(entries, _) = dtype
            && let DataType::Struct(fields) = entries.data_type()
        {
            assert!(matches!(fields[1].data_type(), DataType::List(_)));
        }

        // Non-string key should error
        let result = convert_type("Map(Int32, String)");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Map keys must be String"));
    }

    #[test]
    fn test_complex_nested_types() {
        // Array of tuples
        let (dtype, _) = convert_type("Array(Tuple(String, Int64))").unwrap();
        if let DataType::List(inner) = dtype {
            assert!(matches!(inner.data_type(), DataType::Struct(_)));
        } else {
            panic!("Expected List type");
        }

        // Tuple with array and map
        let (dtype, _) = convert_type("Tuple(Array(Int32), Map(String, Float64))").unwrap();
        if let DataType::Struct(fields) = dtype {
            assert_eq!(fields.len(), 2);
            assert!(matches!(fields[0].data_type(), DataType::List(_)));
            assert!(matches!(fields[1].data_type(), DataType::Map(_, _)));
        } else {
            panic!("Expected Struct type");
        }

        // Map with tuple values
        let (dtype, _) = convert_type("Map(String, Tuple(Int64, String))").unwrap();
        if let DataType::Map(entries, _) = dtype
            && let DataType::Struct(fields) = entries.data_type()
        {
            assert!(matches!(fields[1].data_type(), DataType::Struct(_)));
        }
    }

    #[test]
    fn test_named_tuple_fields() {
        let (dtype, _) = convert_type("Tuple(category String, tag String)").unwrap();
        if let DataType::Struct(fields) = dtype {
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].name(), "category");
            assert_eq!(fields[1].name(), "tag");
        } else {
            panic!("Expected Struct type");
        }

        // Array of named tuples
        let (dtype, _) = convert_type("Array(Tuple(category String, tag String))").unwrap();
        if let DataType::List(inner) = dtype {
            if let DataType::Struct(fields) = inner.data_type() {
                assert_eq!(fields[0].name(), "category");
                assert_eq!(fields[1].name(), "tag");
            } else {
                panic!("Expected Struct type inside List");
            }
        } else {
            panic!("Expected List type");
        }

        // Named tuple with complex types
        let (dtype, _) =
            convert_type("Tuple(items Array(Int32), metadata Map(String, String))").unwrap();
        if let DataType::Struct(fields) = dtype {
            assert_eq!(fields[0].name(), "items");
            assert_eq!(fields[1].name(), "metadata");
            assert!(matches!(fields[0].data_type(), DataType::List(_)));
            assert!(matches!(fields[1].data_type(), DataType::Map(_, _)));
        } else {
            panic!("Expected Struct type");
        }
    }
}
