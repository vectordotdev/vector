use std::collections::BTreeMap;

use remap::{Result, Value};

#[cfg(any(feature = "to_float", feature = "to_int", feature = "to_bool"))]
#[inline]
pub(crate) fn is_scalar_value(value: &Value) -> bool {
    use Value::*;

    match value {
        Integer(_) | Float(_) | Bytes(_) | Boolean(_) | Null => true,
        Timestamp(_) | Map(_) | Array(_) | Regex(_) => false,
    }
}

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

#[cfg(any(
    feature = "parse_json",
    feature = "parse_timestamp",
    feature = "to_timestamp",
    feature = "to_string",
    feature = "to_float",
    feature = "to_int",
    feature = "to_bool"
))]
#[inline]
pub(crate) fn convert_value_or_default(
    value: Result<Value>,
    default: Option<Result<Value>>,
    convert: impl Fn(Value) -> Result<Value> + Clone,
) -> Result<Value> {
    value
        .and_then(convert.clone())
        .or_else(|err| default.ok_or(err)?.and_then(|value| convert(value)))
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
) -> BTreeMap<String, Value> {
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

#[macro_export]
macro_rules! map {
    () => (
        ::std::collections::BTreeMap::new()
    );
    ($($k:tt: $v:expr),+ $(,)?) => {
        vec![$(($k.into(), $v.into())),+]
            .into_iter()
            .collect::<::std::collections::BTreeMap<_, _>>()
    };
}
