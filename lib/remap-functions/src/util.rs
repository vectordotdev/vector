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
