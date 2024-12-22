use regex::Regex;
use std::{collections::HashMap, sync::LazyLock};

use crate::event::Value;

static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^0-9A-Za-z_]").unwrap());
fn slugify_text(input: String) -> String {
    let result = RE.replace_all(&input, "_");
    result.to_lowercase()
}

pub(crate) fn pair_expansion(
    key_s: String,
    value_s: String,
    static_pairs: &mut HashMap<String, String>,
    dynamic_pairs: &mut HashMap<String, String>,
) -> Result<HashMap<String, String>, serde_json::Error> {
    let mut expanded_pairs = HashMap::new();
    if let Some(opening_prefix) = key_s.strip_suffix('*') {
        let output: Result<serde_json::map::Map<String, serde_json::Value>, serde_json::Error> =
            serde_json::from_str(value_s.clone().as_str());

        if let Err(err) = output {
            warn!(
                "Failed to expand dynamic pair. value: {}, err: {}",
                value_s, err
            );
            return Err(err);
        }

        // key_* -> key_one, key_two, key_three
        // * -> one, two, three
        for (k, v) in output.unwrap() {
            let key = slugify_text(format!("{}{}", opening_prefix, k));
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
        static_pairs.insert(key_s.clone(), value_s.clone());
        expanded_pairs.insert(key_s, value_s);
    }
    Ok(expanded_pairs)
}
