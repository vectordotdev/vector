use crate::event::ValueKind;
use std::convert::TryFrom;

/// `Conversion` is a place-holder for a type conversion operation, to
/// convert from a plain (`String`) `ValueKind` into another type. Every
/// variant of `ValueKind` is represented here.
#[derive(Clone)]
pub enum Conversion {
    String,
    Integer,
    Float,
    Boolean,
    Timestamp(String),
}

impl TryFrom<&str> for Conversion {
    type Error = String;
    /// Convert the string into a type conversion.
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "string" => Ok(Conversion::String),
            "integer" | "int" => Ok(Conversion::Integer),
            "float" => Ok(Conversion::Float),
            "bool" | "boolean" => Ok(Conversion::Boolean),
            "timestamp" => Ok(Conversion::Timestamp("%d/%m/%Y:%H:%M:%S %z".into())),
            _ if s.starts_with("timestamp|") => Ok(Conversion::Timestamp(s[10..].into())),
            _ => Err(format!("Invalid type conversion specifier: {:?}", s)),
        }
    }
}

macro_rules! parse_simple {
    ($value:expr, $ty:ty, $tyname:literal, $vtype:ident) => {
        String::from_utf8_lossy(&$value)
            .parse::<$ty>()
            .map_err(|err| format!("Invalid {} {:?}: {}", $tyname, $value, err))
            .map(|value| ValueKind::$vtype(value))
    };
}

impl Conversion {
    /// Use this `Conversion` variant to turn the given `value` into a
    /// new `ValueKind`. This will fail in unexpected ways if the
    /// `value` is not currently a `ValueKind::String`.
    pub fn convert(&self, value: ValueKind) -> Result<ValueKind, String> {
        let value = value.into_bytes();
        match self {
            Conversion::String => Ok(value.into()),
            Conversion::Integer => parse_simple!(value, i64, "integer", Integer),
            Conversion::Float => parse_simple!(value, f64, "floating point number", Float),
            Conversion::Boolean => parse_bool(&String::from_utf8_lossy(&value))
                .map_err(|err| format!("Invalid boolean {:?}: {}", value, err))
                .map(|value| ValueKind::Boolean(value)),
            Conversion::Timestamp(_pattern) => Err("FIX#ME Timestamp".into()),
        }
    }
}

/// Parse a string into a native `bool`. The built in `bool::from_str`
/// only handles two cases, `"true"` and `"false"`. We want to be able
/// to convert from a more diverse set of strings. In particular, the
/// following set of source strings are allowed:
///
///  * `"true"`, `"t"`, `"yes"`, `"y"` (all case-insensitive), and
///  non-zero integers all convert to `true`.
///
///  * `"false"`, `"f"`, `"no"`, `"n"` (all case-insensitive), and `"0"`
///  all convert to `false`.
///
/// Anything else results in a parse error.
fn parse_bool(s: &str) -> Result<bool, &'static str> {
    match s {
        "true" | "t" | "yes" | "y" => Ok(true),
        "false" | "f" | "no" | "n" | "0" => Ok(false),
        _ => {
            if let Ok(n) = s.parse::<isize>() {
                Ok(n != 0)
            } else {
                // Do the case conversion only if simple matches fail,
                // since this operation can be expensive.
                match s.to_lowercase().as_str() {
                    "true" | "t" | "yes" | "y" => Ok(true),
                    "false" | "f" | "no" | "n" => Ok(false),
                    _ => Err("Invalid boolean"),
                }
            }
        }
    }
}

// These should perhaps each go into an individual test function to be
// able to determine what part failed, but that would end up really
// spamming the test logs.

#[test]
fn parse_bool_true() {
    assert_eq!(parse_bool("true"), Ok(true));
    assert_eq!(parse_bool("True"), Ok(true));
    assert_eq!(parse_bool("t"), Ok(true));
    assert_eq!(parse_bool("T"), Ok(true));
    assert_eq!(parse_bool("yes"), Ok(true));
    assert_eq!(parse_bool("YES"), Ok(true));
    assert_eq!(parse_bool("y"), Ok(true));
    assert_eq!(parse_bool("Y"), Ok(true));
    assert_eq!(parse_bool("1"), Ok(true));
    assert_eq!(parse_bool("23456"), Ok(true));
    assert_eq!(parse_bool("-8"), Ok(true));
}

#[test]
fn parse_bool_false() {
    assert_eq!(parse_bool("false"), Ok(false));
    assert_eq!(parse_bool("fAlSE"), Ok(false));
    assert_eq!(parse_bool("f"), Ok(false));
    assert_eq!(parse_bool("F"), Ok(false));
    assert_eq!(parse_bool("no"), Ok(false));
    assert_eq!(parse_bool("NO"), Ok(false));
    assert_eq!(parse_bool("n"), Ok(false));
    assert_eq!(parse_bool("N"), Ok(false));
    assert_eq!(parse_bool("0"), Ok(false));
    assert_eq!(parse_bool("000"), Ok(false));
}

#[test]
fn parse_bool_errors() {
    assert!(parse_bool("X").is_err());
    assert!(parse_bool("yes or no").is_err());
    assert!(parse_bool("123.4").is_err());
}
