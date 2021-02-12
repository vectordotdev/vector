#[cfg(any(feature = "parse_regex", feature = "parse_regex_all"))]
use std::collections::BTreeMap;
use std::str::FromStr;
use vrl::{value::Kind, Value};

/// Rounds the given number to the given precision.
/// Takes a function parameter so the exact rounding function (ceil, floor or round)
/// can be specified.
#[cfg(any(feature = "ceil", feature = "floor", feature = "round"))]
#[inline]
pub(crate) fn round_to_precision<F>(num: f64, precision: i64, fun: F) -> f64
where
    F: Fn(f64) -> f64,
{
    let multiplier = 10_f64.powf(precision as f64);
    fun(num * multiplier as f64) / multiplier
}

/// Takes a set of captures that have resulted from matching a regular expression
/// against some text and fills a BTreeMap with the result.
///
/// All captures are inserted with a key as the numeric index of that capture
/// "0" is the overall match.
/// Any named captures are also added to the Map with the key as the name.
///
#[cfg(any(feature = "parse_regex", feature = "parse_regex_all"))]
pub(crate) fn capture_regex_to_map(
    regex: &regex::Regex,
    capture: regex::Captures,
) -> std::collections::BTreeMap<String, Value> {
    let indexed = capture
        .iter()
        .filter_map(std::convert::identity)
        .enumerate()
        .map(|(idx, c)| (idx.to_string(), c.as_str().into()));

    let names = regex
        .capture_names()
        .filter_map(std::convert::identity)
        .map(|name| {
            (
                name.to_owned(),
                capture.name(name).map(|s| s.as_str()).into(),
            )
        });

    indexed.chain(names).collect()
}

#[cfg(any(feature = "parse_regex", feature = "parse_regex_all"))]
pub(crate) fn regex_type_def(regex: &regex::Regex) -> BTreeMap<String, Kind> {
    let mut inner_type = BTreeMap::new();

    // Add typedefs for each capture by numerical index.
    for num in 0..regex.captures_len() {
        inner_type.insert(num.to_string(), Kind::Bytes);
    }

    // Add a typedef for each capture name.
    for name in regex.capture_names().filter_map(std::convert::identity) {
        inner_type.insert(name.to_owned(), Kind::Bytes);
    }

    inner_type
}

#[cfg(any(feature = "is_nullish", feature = "compact"))]
pub(crate) fn is_nullish(value: &Value) -> bool {
    match value {
        Value::Bytes(v) => {
            let s = &String::from_utf8_lossy(&v)[..];

            match s {
                "-" => true,
                _ => s.chars().all(char::is_whitespace),
            }
        }
        Value::Null => true,
        _ => false,
    }
}

#[cfg(any(feature = "decode_base64", feature = "encode_base64"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Base64Charset {
    Standard,
    UrlSafe,
}

impl Default for Base64Charset {
    fn default() -> Self {
        Self::Standard
    }
}

impl Into<base64::CharacterSet> for Base64Charset {
    fn into(self) -> base64::CharacterSet {
        use Base64Charset::*;

        match self {
            Standard => base64::CharacterSet::Standard,
            UrlSafe => base64::CharacterSet::UrlSafe,
        }
    }
}

impl FromStr for Base64Charset {
    type Err = &'static str;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        use Base64Charset::*;

        match s {
            "standard" => Ok(Standard),
            "url_safe" => Ok(UrlSafe),
            _ => Err("unknown charset"),
        }
    }
}
