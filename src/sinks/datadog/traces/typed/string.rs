//! Datadog `map<string, string>` coercion for [`AttrValue`].
//!
//! Strings and bytes are copied (bytes as lossy UTF-8). A top-level non-finite
//! float uses the unquoted spelling from [`AttrValue::non_finite_spelling`].
//! Every other value is [`AttrValue::write_json`].

use crate::event::typed_trace::AttrValue;

/// Renders `value` for a Datadog `map<string, string>` entry.
///
/// Top-level `String` and `Bytes` are copied (bytes as lossy UTF-8). `Bool`,
/// `Integer`, and finite `Float` use JSON scalar spelling. Non-finite floats are
/// `NaN`, `Infinity`, and `-Infinity`. `Array` and `Map` are compact JSON with
/// map keys in order; nested non-finite floats are those same spellings as JSON
/// strings. `Null` is `null`.
#[must_use]
pub(crate) fn dd_value_to_string(value: &AttrValue) -> String {
    match value {
        AttrValue::String(value) => value.clone(),
        AttrValue::Bytes(value) => String::from_utf8_lossy(value).into_owned(),
        AttrValue::Float(number) => AttrValue::non_finite_spelling(*number)
            .map_or_else(|| json_fragment(value), str::to_owned),
        AttrValue::Null
        | AttrValue::Bool(_)
        | AttrValue::Integer(_)
        | AttrValue::Array(_)
        | AttrValue::Map(_) => json_fragment(value),
    }
}

fn json_fragment(value: &AttrValue) -> String {
    let mut out = String::new();
    value.write_json(&mut out);
    out
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::dd_value_to_string;
    use crate::event::typed_trace::{AttrMap, AttrValue};

    fn float(value: f64) -> AttrValue {
        AttrValue::Float(value)
    }

    #[test]
    fn scalars_use_json_spelling_and_strings_stay_raw() {
        assert_eq!(
            dd_value_to_string(&AttrValue::String("a\"b".into())),
            "a\"b"
        );
        assert_eq!(
            dd_value_to_string(&AttrValue::Bytes(bytes::Bytes::from_static(&[0xff]))),
            "\u{FFFD}"
        );
        assert_eq!(dd_value_to_string(&AttrValue::Null), "null");
        assert_eq!(dd_value_to_string(&AttrValue::Bool(true)), "true");
        assert_eq!(dd_value_to_string(&AttrValue::Bool(false)), "false");
        assert_eq!(dd_value_to_string(&AttrValue::Integer(-12)), "-12");
        assert_eq!(dd_value_to_string(&float(1.5)), "1.5");
        assert_eq!(dd_value_to_string(&float(0.0)), "0.0");
        assert_eq!(dd_value_to_string(&float(-0.0)), "-0.0");
        assert_ne!(
            dd_value_to_string(&float(0.0)),
            dd_value_to_string(&float(-0.0))
        );
    }

    #[test]
    fn non_finite_doubles_are_fixed_words() {
        assert_eq!(dd_value_to_string(&float(f64::NAN)), "NaN");
        assert_eq!(dd_value_to_string(&float(f64::INFINITY)), "Infinity");
        assert_eq!(dd_value_to_string(&float(f64::NEG_INFINITY)), "-Infinity");
        assert_eq!(
            dd_value_to_string(&float(f64::from_bits(0x7ff8_0000_0000_0001))),
            "NaN"
        );
    }

    #[test]
    fn arrays_and_maps_are_deterministic_json() {
        let array = AttrValue::Array(vec![
            AttrValue::String("a".into()),
            AttrValue::Integer(1),
            float(f64::NAN),
            AttrValue::Null,
            AttrValue::Bool(true),
            AttrValue::Bytes(bytes::Bytes::from_static(b"ok")),
        ]);
        assert_eq!(
            dd_value_to_string(&array),
            r#"["a",1,"NaN",null,true,"ok"]"#
        );

        let mut inner = AttrMap::new();
        inner.insert("z".into(), AttrValue::Integer(1));
        inner.insert("a".into(), float(f64::NEG_INFINITY));
        let mut outer = AttrMap::new();
        outer.insert("m".into(), AttrValue::Map(inner));
        outer.insert("b".into(), AttrValue::Integer(2));
        assert_eq!(
            dd_value_to_string(&AttrValue::Map(outer)),
            r#"{"b":2,"m":{"a":"-Infinity","z":1}}"#
        );
    }

    proptest! {
        #[test]
        fn object_keys_render_in_sorted_order(keys in prop::collection::hash_set("[a-z]{1,3}", 1..5)) {
            let mut map = AttrMap::new();
            for key in &keys {
                map.insert(key.as_str().into(), AttrValue::Integer(1));
            }
            let rendered = dd_value_to_string(&AttrValue::Map(map));
            let mut sorted: Vec<_> = keys.into_iter().collect();
            sorted.sort();
            let pairs: Vec<_> = sorted.iter().map(|key| format!("\"{key}\":1")).collect();
            let expected = format!("{{{}}}", pairs.join(","));
            assert_eq!(rendered, expected);
        }

        #[test]
        fn every_non_finite_payload_has_one_spelling(
            negative in any::<bool>(),
            payload in any::<u64>(),
        ) {
            let mantissa = payload & ((1u64 << 52) - 1);
            let value =
                f64::from_bits((u64::from(negative) << 63) | (0x7ffu64 << 52) | mantissa);
            let expected = if value.is_nan() {
                "NaN"
            } else if value.is_sign_negative() {
                "-Infinity"
            } else {
                "Infinity"
            };
            assert_eq!(dd_value_to_string(&float(value)), expected);
        }
    }
}
