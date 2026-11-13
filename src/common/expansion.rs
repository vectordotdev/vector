use std::{collections::HashMap, sync::LazyLock};

use regex::Regex;

use crate::event::Value;

static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^0-9A-Za-z_]").unwrap());
fn slugify_text(input: &str, lowercase: bool) -> String {
    let result = RE.replace_all(input, "_");
    if lowercase {
        result.to_lowercase()
    } else {
        result.into_owned()
    }
}

/// Expands the given possibly template-able `key_s` and `value_s`, and return the expanded owned pairs
/// it would also insert the pairs into either `static_pairs` or `dynamic_pairs` depending on the template-ability of `key_s`.
///
/// Expanded keys have every character other than ASCII letters, digits and `_` replaced with `_`.
/// They are also lowercased when `lowercase_keys` is `true`.
pub(crate) fn pair_expansion(
    key_s: &str,
    value_s: &str,
    static_pairs: &mut HashMap<String, String>,
    dynamic_pairs: &mut HashMap<String, String>,
    lowercase_keys: bool,
) -> Result<HashMap<String, String>, serde_json::Error> {
    let mut expanded_pairs = HashMap::new();
    if let Some(opening_prefix) = key_s.strip_suffix('*') {
        let output: serde_json::map::Map<String, serde_json::Value> =
            serde_json::from_str(value_s)?;

        // key_* -> key_one, key_two, key_three
        // * -> one, two, three
        for (k, v) in output {
            let key = slugify_text(&format!("{opening_prefix}{k}"), lowercase_keys);
            let val = Value::from(v).to_string_lossy().into_owned();
            if val == "<null>" {
                warn!("Encountered \"null\" value for dynamic pair. key: {key}");
                continue;
            }
            if let Some(prev) = dynamic_pairs.insert(key.clone(), val.clone()) {
                warn!(
                    "Encountered duplicated dynamic pair. \
                                key: {key}, value: {val:?}, discarded value: {prev:?}"
                );
            }
            expanded_pairs.insert(key, val);
        }
    } else {
        static_pairs.insert(key_s.to_string(), value_s.to_string());
        expanded_pairs.insert(key_s.to_string(), value_s.to_string());
    }
    Ok(expanded_pairs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expand(key: &str, value: &str, lowercase_keys: bool) -> HashMap<String, String> {
        let mut static_pairs = HashMap::new();
        let mut dynamic_pairs = HashMap::new();
        pair_expansion(
            key,
            value,
            &mut static_pairs,
            &mut dynamic_pairs,
            lowercase_keys,
        )
        .unwrap()
    }

    #[test]
    fn expanded_keys_are_lowercased_by_default() {
        let pairs = expand("*", r#"{"userId": "1", "user-name": "a"}"#, true);
        assert_eq!(pairs.get("userid").map(String::as_str), Some("1"));
        assert_eq!(pairs.get("user_name").map(String::as_str), Some("a"));
        assert_eq!(pairs.len(), 2);
    }

    #[test]
    fn expanded_keys_keep_case_when_lowercasing_is_disabled() {
        let pairs = expand(
            "pod_*",
            r#"{"userId": "1", "userid": "2", "user-Name": "a"}"#,
            false,
        );
        assert_eq!(pairs.get("pod_userId").map(String::as_str), Some("1"));
        assert_eq!(pairs.get("pod_userid").map(String::as_str), Some("2"));
        assert_eq!(pairs.get("pod_user_Name").map(String::as_str), Some("a"));
        assert_eq!(pairs.len(), 3);
    }

    #[test]
    fn static_keys_are_never_changed() {
        let pairs = expand("ClusterName", "prod", true);
        assert_eq!(pairs.get("ClusterName").map(String::as_str), Some("prod"));
    }
}
