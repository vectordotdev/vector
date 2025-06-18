use std::{
    collections::{HashMap, HashSet},
    sync::LazyLock,
};

use futures::TryFutureExt;
use indexmap::IndexMap;
use regex::{Captures, Regex};
use serde::{Deserialize, Serialize};
use toml::value::Table;
use toml::Value;
use vector_lib::config::ComponentKey;

use crate::config::loading::{interpolate_toml_table, resolve_environment_variables};
use crate::{
    config::{
        loading::{deserialize_table, process::Process, ComponentHint, Loader},
        SecretBackend,
    },
    secrets::SecretBackends,
    signal,
};

// The following regex aims to extract a pair of strings, the first being the secret backend name
// and the second being the secret key. Here are some matching & non-matching examples:
// - "SECRET[backend.secret_name]" will match and capture "backend" and "secret_name"
// - "SECRET[backend.secret.name]" will match and capture "backend" and "secret.name"
// - "SECRET[backend..secret.name]" will match and capture "backend" and ".secret.name"
// - "SECRET[secret_name]" will not match
// - "SECRET[.secret.name]" will not match
pub static COLLECTOR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"SECRET\[([[:word:]]+)\.([[:word:].]+)\]").unwrap());

/// Helper type for specifically deserializing secrets backends.
#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct SecretBackendOuter {
    #[serde(default)]
    pub(crate) secret: IndexMap<ComponentKey, SecretBackends>,
}

/// Loader for secrets backends.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct SecretBackendLoader {
    backends: IndexMap<ComponentKey, SecretBackends>,
    pub(crate) secret_keys: HashMap<String, HashSet<String>>,
}

impl SecretBackendLoader {
    pub(crate) fn new() -> Self {
        Self {
            backends: IndexMap::new(),
            secret_keys: HashMap::new(),
        }
    }

    pub(crate) async fn retrieve(
        &mut self,
        signal_rx: &mut signal::SignalRx,
    ) -> Result<HashMap<String, String>, String> {
        let mut secrets: HashMap<String, String> = HashMap::new();

        for (backend_name, keys) in &self.secret_keys {
            let backend = self.backends
                .get_mut(&ComponentKey::from(backend_name.clone()))
                .ok_or_else(|| {
                    format!("Backend \"{backend_name}\" is required for secret retrieval but was not found in config.")
                })?;

            debug!(message = "Retrieving secrets from a backend.", backend = ?backend_name, keys = ?keys);
            let backend_secrets = backend
                .retrieve(keys.clone(), signal_rx)
                .map_err(|e| {
                    format!("Error while retrieving secret from backend \"{backend_name}\": {e}.",)
                })
                .await?;

            for (k, v) in backend_secrets {
                trace!(message = "Successfully retrieved a secret.", backend = ?backend_name, key = ?k);
                secrets.insert(format!("{backend_name}.{k}"), v);
            }
        }

        Ok(secrets)
    }

    pub(crate) fn has_secrets_to_retrieve(&self) -> bool {
        !self.secret_keys.is_empty()
    }
}

impl Process for SecretBackendLoader {
    fn postprocess(&mut self, table: Table) -> Result<Table, Vec<String>> {
        let table = resolve_environment_variables(table)?;

        // If there's no top-level `secret` section, nothing to do here.
        if !table.contains_key("secret") {
            return Ok(table);
        }

        collect_secret_keys_from_table(&table, &mut self.secret_keys);
        Ok(table)
    }

    fn merge(&mut self, table: Table, _: Option<ComponentHint>) -> Result<(), Vec<String>> {
        if table.contains_key("secret") {
            let additional = deserialize_table::<SecretBackendOuter>(table)?;
            self.backends.extend(additional.secret);
        }
        Ok(())
    }
}

impl Loader<SecretBackendLoader> for SecretBackendLoader {
    fn take(self) -> SecretBackendLoader {
        self
    }
}

fn collect_secret_keys(input: &str, keys: &mut HashMap<String, HashSet<String>>) {
    COLLECTOR.captures_iter(input).for_each(|cap| {
        if let (Some(backend), Some(key)) = (cap.get(1), cap.get(2)) {
            if let Some(keys) = keys.get_mut(backend.as_str()) {
                keys.insert(key.as_str().to_string());
            } else {
                keys.insert(
                    backend.as_str().to_string(),
                    HashSet::from_iter(std::iter::once(key.as_str().to_string())),
                );
            }
        }
    });
}

/// Recursively collects all secret references from keys and string values in a TOML table.
pub fn collect_secret_keys_from_table(table: &Table, keys: &mut HashMap<String, HashSet<String>>) {
    for (k, v) in table {
        // Check key for secrets like SECRET[backend.key]
        collect_secret_keys(k, keys);

        // Check the value
        match v {
            Value::String(s) => collect_secret_keys(s, keys),
            Value::Array(arr) => {
                for item in arr {
                    collect_secret_keys_from_value(item, keys);
                }
            }
            Value::Table(inner) => {
                collect_secret_keys_from_table(inner, keys);
            }
            _ => {}
        }
    }
}

fn collect_secret_keys_from_value(value: &Value, keys: &mut HashMap<String, HashSet<String>>) {
    match value {
        Value::String(s) => collect_secret_keys(s, keys),
        Value::Array(arr) => {
            for item in arr {
                collect_secret_keys_from_value(item, keys);
            }
        }
        Value::Table(t) => collect_secret_keys_from_table(t, keys),
        _ => {}
    }
}

pub fn interpolate_toml_table_with_secrets(
    table: &Table,
    vars: &HashMap<String, String>,
) -> Result<Table, Vec<String>> {
    interpolate_toml_table(table, vars, interpolate_secrets)
}

fn interpolate_secrets(
    input: &str,
    secrets: &HashMap<String, String>,
) -> Result<String, Vec<String>> {
    let mut errors = Vec::<String>::new();
    let output = COLLECTOR
        .replace_all(input, |caps: &Captures<'_>| {
            caps.get(1)
                .and_then(|b| caps.get(2).map(|k| (b, k)))
                .and_then(|(b, k)| secrets.get(&format!("{}.{}", b.as_str(), k.as_str())))
                .cloned()
                .unwrap_or_else(|| {
                    errors.push(format!(
                        "Unable to find secret replacement for {}.",
                        caps.get(0).unwrap().as_str()
                    ));
                    "".to_string()
                })
        })
        .into_owned();
    if errors.is_empty() {
        Ok(output)
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{collect_secret_keys, interpolate_secrets};
    use crate::config::loading::interpolate_toml_table_with_secrets;
    use indoc::indoc;
    use toml::{Table, Value};

    #[test]
    fn replacement() {
        let secrets: HashMap<String, String> = vec![
            ("a.secret.key".into(), "value".into()),
            ("a...key".into(), "a...value".into()),
        ]
        .into_iter()
        .collect();

        assert_eq!(
            Ok("value".into()),
            interpolate_secrets("SECRET[a.secret.key]", &secrets)
        );
        assert_eq!(
            Ok("value value".into()),
            interpolate_secrets("SECRET[a.secret.key] SECRET[a.secret.key]", &secrets)
        );

        assert_eq!(
            Ok("xxxvalueyyy".into()),
            interpolate_secrets("xxxSECRET[a.secret.key]yyy", &secrets)
        );
        assert_eq!(
            Ok("a...value".into()),
            interpolate_secrets("SECRET[a...key]", &secrets)
        );
        assert_eq!(
            Ok("xxxSECRET[non_matching_syntax]yyy".into()),
            interpolate_secrets("xxxSECRET[non_matching_syntax]yyy", &secrets)
        );
        assert_eq!(
            Err(vec![
                "Unable to find secret replacement for SECRET[a.non.existing.key].".into()
            ]),
            interpolate_secrets("xxxSECRET[a.non.existing.key]yyy", &secrets)
        );
    }

    #[test]
    fn collection() {
        let mut keys = HashMap::new();
        collect_secret_keys(
            indoc! {r"
            SECRET[first_backend.secret_key]
            SECRET[first_backend.another_secret_key]
            SECRET[second_backend.secret_key]
            SECRET[second_backend.secret.key]
            SECRET[first_backend.a_third.secret_key]
            SECRET[first_backend...an_extra_secret_key]
            SECRET[non_matching_syntax]
            SECRET[.non.matching.syntax]
        "},
            &mut keys,
        );
        assert_eq!(keys.len(), 2);
        assert!(keys.contains_key("first_backend"));
        assert!(keys.contains_key("second_backend"));

        let first_backend_keys = keys.get("first_backend").unwrap();
        assert_eq!(first_backend_keys.len(), 4);
        assert!(first_backend_keys.contains("secret_key"));
        assert!(first_backend_keys.contains("another_secret_key"));
        assert!(first_backend_keys.contains("a_third.secret_key"));
        assert!(first_backend_keys.contains("..an_extra_secret_key"));

        let second_backend_keys = keys.get("second_backend").unwrap();
        assert_eq!(second_backend_keys.len(), 2);
        assert!(second_backend_keys.contains("secret_key"));
        assert!(second_backend_keys.contains("secret.key"));
    }

    #[test]
    fn collection_duplicates() {
        let mut keys = HashMap::new();
        collect_secret_keys(
            indoc! {r"
            SECRET[first_backend.secret_key]
            SECRET[first_backend.secret_key]
        "},
            &mut keys,
        );

        let first_backend_keys = keys.get("first_backend").unwrap();
        assert_eq!(first_backend_keys.len(), 1);
        assert!(first_backend_keys.contains("secret_key"));
    }

    #[test]
    fn interpolate_secrets_all_types() {
        // Step 1: create a TOML table with secret placeholders
        let mut log_fields = Table::new();
        log_fields.insert(
            "str".into(),
            Value::String("SECRET[backend.str_key]".into()),
        );
        log_fields.insert(
            "int".into(),
            Value::String("SECRET[backend.int_key]".into()),
        );
        log_fields.insert(
            "float".into(),
            Value::String("SECRET[backend.float_key]".into()),
        );
        log_fields.insert(
            "bool".into(),
            Value::String("SECRET[backend.bool_key]".into()),
        );

        let mut inputs = Table::new();
        inputs.insert("log_fields".into(), Value::Table(log_fields));
        let mut root_table = Table::new();
        root_table.insert("inputs".into(), Value::Table(inputs));

        // Step 2: define mock secrets with correct types as strings
        let secrets = HashMap::from([
            ("backend.str_key".to_string(), "hello".to_string()),
            ("backend.int_key".to_string(), "42".to_string()),
            ("backend.float_key".to_string(), "3.14".to_string()),
            ("backend.bool_key".to_string(), "true".to_string()),
        ]);

        // Step 3: interpolate secrets
        let result = interpolate_toml_table_with_secrets(&root_table, &secrets).unwrap();

        // Step 4: verify only the secret-related values, including type coercion
        let log_fields = &result["inputs"]["log_fields"];
        let log_fields = log_fields.as_table().expect("log_fields is a table");

        assert_eq!(log_fields["str"], Value::String("hello".into()));
        assert_eq!(log_fields["int"], Value::Integer(42));
        assert_eq!(log_fields["float"], Value::Float(3.14));
        assert_eq!(log_fields["bool"], Value::Boolean(true));
    }
}
