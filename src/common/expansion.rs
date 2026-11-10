use std::{borrow::Cow, collections::HashMap, sync::LazyLock};

use regex::Regex;

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
    pair_expansion_inner(
        Cow::Borrowed(key_s),
        Cow::Borrowed(value_s),
        static_pairs,
        dynamic_pairs,
        Some(&mut expanded_pairs),
    )?;
    Ok(expanded_pairs)
}

/// Expands a rendered pair directly into the destination maps. Unlike
/// [`pair_expansion`], this consumes the rendered strings and does not build a
/// second map for callers that only need the merged destinations.
pub(crate) fn pair_expansion_into(
    key_s: String,
    value_s: String,
    static_pairs: &mut HashMap<String, String>,
    dynamic_pairs: &mut HashMap<String, String>,
) -> Result<(), serde_json::Error> {
    pair_expansion_inner(
        Cow::Owned(key_s),
        Cow::Owned(value_s),
        static_pairs,
        dynamic_pairs,
        None,
    )
}

fn pair_expansion_inner<'a>(
    key_s: Cow<'a, str>,
    value_s: Cow<'a, str>,
    static_pairs: &mut HashMap<String, String>,
    dynamic_pairs: &mut HashMap<String, String>,
    mut expanded_pairs: Option<&mut HashMap<String, String>>,
) -> Result<(), serde_json::Error> {
    if let Some(opening_prefix) = key_s.strip_suffix('*') {
        let output: serde_json::map::Map<String, serde_json::Value> =
            serde_json::from_str(value_s.as_ref())?;

        // key_* -> key_one, key_two, key_three
        // * -> one, two, three
        for (k, v) in output {
            let key = slugify_text(&format!("{opening_prefix}{k}"));
            let val = Value::from(v).to_string_lossy().into_owned();
            if val == "<null>" {
                warn!("Encountered \"null\" value for dynamic pair. key: {key}");
                continue;
            }
            if let Some(expanded_pairs) = expanded_pairs.as_deref_mut() {
                if let Some(prev) = dynamic_pairs.insert(key.clone(), val.clone()) {
                    warn!(
                        "Encountered duplicated dynamic pair. \
                                    key: {key}, value: {val:?}, discarded value: {prev:?}"
                    );
                }
                expanded_pairs.insert(key, val);
            } else {
                match dynamic_pairs.entry(key) {
                    std::collections::hash_map::Entry::Occupied(mut entry) => {
                        warn!(
                            "Encountered duplicated dynamic pair. \
                                        key: {}, value: {val:?}, discarded value: {:?}",
                            entry.key(),
                            entry.get(),
                        );
                        entry.insert(val);
                    }
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        entry.insert(val);
                    }
                }
            }
        }
    } else {
        let key_s = key_s.into_owned();
        let value_s = value_s.into_owned();
        if let Some(expanded_pairs) = expanded_pairs {
            static_pairs.insert(key_s.clone(), value_s.clone());
            expanded_pairs.insert(key_s, value_s);
        } else {
            static_pairs.insert(key_s, value_s);
        }
    }
    Ok(())
}
