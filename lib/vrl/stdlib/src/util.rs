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
    fun(num * multiplier) / multiplier
}

/// Takes a set of captures that have resulted from matching a regular expression
/// against some text and fills a `BTreeMap` with the result.
///
/// All captures are inserted with a key as the numeric index of that capture
/// "0" is the overall match.
/// Any named captures are also added to the Map with the key as the name.
///
#[cfg(any(feature = "parse_regex", feature = "parse_regex_all"))]
pub(crate) fn capture_regex_to_map(
    regex: &regex::Regex,
    capture: &regex::Captures,
    numeric_groups: bool,
) -> std::collections::BTreeMap<String, ::value::Value> {
    let names = regex.capture_names().flatten().map(|name| {
        (
            name.to_owned(),
            capture.name(name).map(|s| s.as_str()).into(),
        )
    });

    if numeric_groups {
        let indexed = capture
            .iter()
            .flatten()
            .enumerate()
            .map(|(idx, c)| (idx.to_string(), c.as_str().into()));

        indexed.chain(names).collect()
    } else {
        names.collect()
    }
}

#[cfg(any(feature = "parse_regex", feature = "parse_regex_all"))]
pub(crate) fn regex_kind(
    regex: &regex::Regex,
) -> std::collections::BTreeMap<vrl::value::kind::Field, vrl::value::Kind> {
    let mut inner_type = std::collections::BTreeMap::new();

    // Add typedefs for each capture by numerical index.
    for num in 0..regex.captures_len() {
        inner_type.insert(
            num.to_string().into(),
            vrl::value::Kind::bytes() | vrl::value::Kind::null(),
        );
    }

    // Add a typedef for each capture name.
    for name in regex.capture_names().flatten() {
        inner_type.insert(name.to_owned().into(), vrl::value::Kind::bytes());
    }

    inner_type
}

#[cfg(any(feature = "is_nullish", feature = "compact"))]
pub(crate) fn is_nullish(value: &::value::Value) -> bool {
    match value {
        ::value::Value::Bytes(v) => {
            let s = &String::from_utf8_lossy(v)[..];

            match s {
                "-" => true,
                _ => s.chars().all(char::is_whitespace),
            }
        }
        ::value::Value::Null => true,
        _ => false,
    }
}

#[cfg(any(feature = "decode_base64", feature = "encode_base64"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Base64Charset {
    Standard,
    UrlSafe,
}

#[cfg(any(feature = "decode_base64", feature = "encode_base64"))]
impl Default for Base64Charset {
    fn default() -> Self {
        Self::Standard
    }
}

#[cfg(any(feature = "decode_base64", feature = "encode_base64"))]
impl From<Base64Charset> for base64::alphabet::Alphabet {
    fn from(charset: Base64Charset) -> base64::alphabet::Alphabet {
        use Base64Charset::{Standard, UrlSafe};

        match charset {
            Standard => base64::alphabet::STANDARD,
            UrlSafe => base64::alphabet::URL_SAFE,
        }
    }
}

#[cfg(any(feature = "decode_base64", feature = "encode_base64"))]
impl std::str::FromStr for Base64Charset {
    type Err = &'static str;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        use Base64Charset::{Standard, UrlSafe};

        match s {
            "standard" => Ok(Standard),
            "url_safe" => Ok(UrlSafe),
            _ => Err("unknown charset"),
        }
    }
}
