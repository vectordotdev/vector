use std::sync::LazyLock;

use regex::Regex;

use crate::event::Value;

static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^0-9A-Za-z_]").unwrap());
fn slugify_text(input: &str) -> String {
    let result = RE.replace_all(input, "_");
    result.to_lowercase()
}

/// Whether a pair was supplied explicitly or produced by wildcard expansion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PairOrigin {
    Explicit,
    Wildcard,
}

/// A failed wildcard expansion, retaining the rendered inputs for diagnostics.
#[derive(Debug)]
pub(crate) struct ExpansionError {
    pub key: String,
    pub value: String,
    pub error: serde_json::Error,
}

/// Emits an owned pair, or expands a trailing wildcard using a JSON object's fields.
///
/// Explicit pairs are moved directly into the callback. Wildcard keys are
/// normalized and null values are omitted. The caller owns collision handling:
/// multiple object fields can normalize to the same key and are emitted in the
/// JSON map's iteration order. Invalid input fails before any pairs are emitted.
pub(crate) fn expand_pair(
    key: String,
    value: String,
    mut emit: impl FnMut(PairOrigin, String, String),
) -> Result<(), ExpansionError> {
    let Some(prefix) = key.strip_suffix('*') else {
        emit(PairOrigin::Explicit, key, value);
        return Ok(());
    };

    let output: serde_json::Map<String, serde_json::Value> = match serde_json::from_str(&value) {
        Ok(output) => output,
        Err(error) => return Err(ExpansionError { key, value, error }),
    };

    for (field, value) in output {
        let key = slugify_text(&format!("{prefix}{field}"));
        let value = Value::from(value).to_string_lossy().into_owned();
        if value == "<null>" {
            warn!(message = "Encountered \"null\" value for dynamic pair.", %key);
            continue;
        }
        emit(PairOrigin::Wildcard, key, value);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{PairOrigin, expand_pair};

    #[test]
    fn explicit_pair_preserves_strings_without_copying() {
        let key = String::from("Original*Key");
        let value = String::from("not JSON");
        let key_ptr = key.as_ptr();
        let value_ptr = value.as_ptr();
        let mut pairs = Vec::new();

        expand_pair(key, value, |origin, key, value| {
            assert_eq!(key.as_ptr(), key_ptr);
            assert_eq!(value.as_ptr(), value_ptr);
            pairs.push((origin, key, value));
        })
        .unwrap();

        assert_eq!(
            pairs,
            vec![(
                PairOrigin::Explicit,
                "Original*Key".into(),
                "not JSON".into()
            )]
        );
    }

    #[test]
    fn wildcard_normalizes_keys_and_formats_values() {
        let mut pairs = Vec::new();
        expand_pair(
            "Prefix_*".into(),
            r#"{"Field.Name":"text","boolean":true,"number":42,"array":["one","two"],"object":{"nested":"value"},"null":null,"null_string":"<null>"}"#.into(),
            |origin, key, value| pairs.push((origin, key, value)),
        )
        .unwrap();
        pairs.sort_by(|a, b| a.1.cmp(&b.1));

        assert_eq!(
            pairs,
            vec![
                (
                    PairOrigin::Wildcard,
                    "prefix_array".into(),
                    r#"["one","two"]"#.into()
                ),
                (PairOrigin::Wildcard, "prefix_boolean".into(), "true".into()),
                (
                    PairOrigin::Wildcard,
                    "prefix_field_name".into(),
                    "text".into()
                ),
                (PairOrigin::Wildcard, "prefix_number".into(), "42".into()),
                (
                    PairOrigin::Wildcard,
                    "prefix_object".into(),
                    r#"{"nested":"value"}"#.into()
                ),
            ]
        );
    }

    #[test]
    fn wildcard_leaves_collision_handling_to_caller() {
        let mut pairs = Vec::new();
        expand_pair(
            "*".into(),
            r#"{"key-one":"first","key_one":"second"}"#.into(),
            |origin, key, value| pairs.push((origin, key, value)),
        )
        .unwrap();

        assert_eq!(
            pairs,
            vec![
                (PairOrigin::Wildcard, "key_one".into(), "first".into()),
                (PairOrigin::Wildcard, "key_one".into(), "second".into()),
            ]
        );
    }

    #[test]
    fn empty_object_emits_no_pairs() {
        expand_pair("*".into(), "{}".into(), |_, _, _| {
            panic!("empty object should not emit any pairs");
        })
        .unwrap();
    }

    #[test]
    fn invalid_wildcard_value_preserves_inputs_and_emits_no_pairs() {
        for value in [r#"{"valid":1,"invalid":}"#, "[]", "null", "42", r#""text""#] {
            let error = expand_pair("prefix_*".into(), value.into(), |_, _, _| {
                panic!("invalid input should not emit any pairs");
            })
            .unwrap_err();

            assert_eq!(error.key, "prefix_*");
            assert_eq!(error.value, value);
            assert!(!error.error.to_string().is_empty());
        }
    }
}
