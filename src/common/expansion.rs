use regex::Regex;
use std::{collections::HashMap, sync::LazyLock};

use crate::event::Value;

static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^0-9A-Za-z_]").unwrap());
fn slugify_text(input: &str) -> String {
    let result = RE.replace_all(input, "_");
    result.to_lowercase()
}

/// Expands the given possibly template-able `key_s` and `value_s`, and return the expanded owned pairs
/// it would also insert the pairs into either `static_pairs` or `dynamic_pairs` depending on the template-ability of `key_s`.
pub(crate) fn pair_expansion(
    key_s: &str,
    value_s: &str,
    static_pairs: &mut HashMap<String, String>,
    dynamic_pairs: &mut HashMap<String, String>,
) -> Result<HashMap<String, String>, serde_json::Error> {
    let mut expanded_pairs = HashMap::new();
    if let Some(opening_prefix) = key_s.strip_suffix('*') {
        let output: serde_json::map::Map<String, serde_json::Value> =
            serde_json::from_str(value_s)?;

        // key_* -> key_one, key_two, key_three
        // * -> one, two, three
        for (k, v) in output {
            let key = slugify_text(&format!("{opening_prefix}{k}"));
            let val = Value::from(v).to_string_lossy().into_owned();
            if val == "<null>" {
                warn!("Encountered \"null\" value for dynamic pair. key: {}", key);
                continue;
            }
            if let Some(prev) = dynamic_pairs.insert(key.clone(), val.clone()) {
                warn!(
                    "Encountered duplicated dynamic pair. \
                                key: {}, value: {:?}, discarded value: {:?}",
                    key, val, prev
                );
            };
            expanded_pairs.insert(key, val);
        }
    } else {
        static_pairs.insert(key_s.to_string(), value_s.to_string());
        expanded_pairs.insert(key_s.to_string(), value_s.to_string());
    }
    Ok(expanded_pairs)
}
