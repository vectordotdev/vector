use std::{
    collections::{HashMap, HashSet},
    sync::LazyLock,
};

use futures::TryFutureExt;
use indexmap::IndexMap;
use regex::{Captures, Regex};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use vector_lib::config::ComponentKey;

use crate::{
    config::{
        SecretBackend,
        loading::{
            ComponentHint, Loader, deserialize_config_map, interpolate_config_map,
            process::Process, representation::ConfigMap,
        },
    },
    secrets::SecretBackends,
    signal,
};

// The following regex aims to extract a pair of strings, the first being the secret backend name
// and the second being the secret key. Here are some matching & non-matching examples:
// - "SECRET[backend.secret_name]" will match and capture "backend" and "secret_name"
// - "SECRET[my-backend.secret_name]" will match and capture "my-backend" and "secret_name"
// - "SECRET[backend.secret.name]" will match and capture "backend" and "secret.name"
// - "SECRET[backend..secret.name]" will match and capture "backend" and ".secret.name"
// - "SECRET[backend.path/to/secret]" will match and capture "backend" and "path/to/secret"
// - "SECRET[secret_name]" will not match
// - "SECRET[.secret.name]" will not match
pub static COLLECTOR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"SECRET\[([[:word:]\-]+)\.([[:word:].\-/]+)\]").unwrap());

const SECRET_KEY: &str = "secret";

/// Helper type for specifically deserializing secrets backends.
#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct SecretBackendOuter {
    #[serde(default)]
    pub(crate) secret: IndexMap<ComponentKey, SecretBackends>,
}

/// Loader for secrets backends.
#[derive(Debug, Deserialize, Serialize)]
pub struct SecretBackendLoader {
    backends: IndexMap<ComponentKey, SecretBackends>,
    pub(crate) secret_keys: HashMap<String, HashSet<String>>,
    #[serde(skip)]
    interpolate_env: bool,
}

impl SecretBackendLoader {
    /// Sets whether to interpolate environment variables in the config.
    pub const fn interpolate_env(mut self, interpolate: bool) -> Self {
        self.interpolate_env = interpolate;
        self
    }

    pub(crate) async fn retrieve(
        &mut self,
        signal_rx: &mut signal::SignalRx,
    ) -> Result<HashMap<String, String>, String> {
        let mut secrets: HashMap<String, String> = HashMap::new();

        for (backend_name, keys) in &self.secret_keys {
            let backend = self
                .backends
                .get_mut(&ComponentKey::from(backend_name.clone()))
                .ok_or_else(|| {
                    format!(
                        "Backend \"{backend_name}\" is required for secret retrieval but was not found in config."
                    )
                })?;

            debug!(message = "Retrieving secrets from a backend.", backend = ?backend_name, keys = ?keys);
            let backend_secrets = backend
                .retrieve(keys.clone(), signal_rx)
                .map_err(|e| {
                    format!("Error while retrieving secret from backend \"{backend_name}\": {e}.")
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

impl Default for SecretBackendLoader {
    fn default() -> Self {
        Self {
            backends: IndexMap::new(),
            secret_keys: HashMap::new(),
            interpolate_env: super::env_var_interpolation_enabled(),
        }
    }
}

impl Process for SecretBackendLoader {
    fn should_interpolate_env(&self) -> bool {
        self.interpolate_env
    }

    fn postprocess(&mut self, map: ConfigMap) -> Result<ConfigMap, Vec<String>> {
        collect_secret_keys_from_map(&map, &mut self.secret_keys);
        Ok(map)
    }

    fn merge(&mut self, map: ConfigMap, _: Option<ComponentHint>) -> Result<(), Vec<String>> {
        if let Some(secret_value) = map.get(SECRET_KEY) {
            let mut secret_map = ConfigMap::new();
            secret_map.insert(SECRET_KEY.to_string(), secret_value.clone());
            let additional = deserialize_config_map::<SecretBackendOuter>(secret_map)?;
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

/// Recursively collects secret references from object keys and string leaves.
pub fn collect_secret_keys_from_map(map: &ConfigMap, keys: &mut HashMap<String, HashSet<String>>) {
    for (key, value) in map {
        collect_secret_keys(key, keys);
        collect_secret_keys_from_value(value, keys);
    }
}

fn collect_secret_keys_from_value(value: &Value, keys: &mut HashMap<String, HashSet<String>>) {
    match value {
        Value::String(value) => collect_secret_keys(value, keys),
        Value::Array(values) => {
            for value in values {
                collect_secret_keys_from_value(value, keys);
            }
        }
        Value::Object(map) => collect_secret_keys_from_map(map, keys),
        _ => {}
    }
}

pub fn interpolate_config_map_with_secrets(
    map: &ConfigMap,
    secrets: &HashMap<String, String>,
) -> Result<ConfigMap, Vec<String>> {
    interpolate_config_map(map, secrets, interpolate_secrets)
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

    use indoc::indoc;

    use super::{collect_secret_keys, interpolate_secrets as interpolate};

    #[test]
    fn replacement() {
        let secrets: HashMap<String, String> = vec![
            ("a.secret.key".into(), "value".into()),
            ("a...key".into(), "a...value".into()),
            ("backend.path/to/secret".into(), "secret_value".into()),
            ("backend.nested/dir/file".into(), "nested_value".into()),
            ("my-backend.secret.key".into(), "hyphenated_value".into()),
        ]
        .into_iter()
        .collect();

        assert_eq!(
            Ok("value".into()),
            interpolate("SECRET[a.secret.key]", &secrets)
        );
        assert_eq!(
            Ok("value value".into()),
            interpolate("SECRET[a.secret.key] SECRET[a.secret.key]", &secrets)
        );

        assert_eq!(
            Ok("xxxvalueyyy".into()),
            interpolate("xxxSECRET[a.secret.key]yyy", &secrets)
        );
        assert_eq!(
            Ok("a...value".into()),
            interpolate("SECRET[a...key]", &secrets)
        );
        assert_eq!(
            Ok("secret_value".into()),
            interpolate("SECRET[backend.path/to/secret]", &secrets)
        );
        assert_eq!(
            Ok("nested_value".into()),
            interpolate("SECRET[backend.nested/dir/file]", &secrets)
        );
        assert_eq!(
            Ok("hyphenated_value".into()),
            interpolate("SECRET[my-backend.secret.key]", &secrets)
        );
        assert_eq!(
            Ok("xxxSECRET[non_matching_syntax]yyy".into()),
            interpolate("xxxSECRET[non_matching_syntax]yyy", &secrets)
        );
        assert_eq!(
            Err(vec![
                "Unable to find secret replacement for SECRET[a.non.existing.key].".into()
            ]),
            interpolate("xxxSECRET[a.non.existing.key]yyy", &secrets)
        );
    }

    #[test]
    fn collection() {
        let mut keys = HashMap::new();
        collect_secret_keys(
            indoc! {r"
            SECRET[first_backend.secret_key]
            SECRET[first_backend.secret-key]
            SECRET[first_backend.another_secret_key]
            SECRET[second_backend.secret_key]
            SECRET[second_backend.secret.key]
            SECRET[first_backend.a_third.secret_key]
            SECRET[first_backend...an_extra_secret_key]
            SECRET[third-backend.secret_key]
            SECRET[first_backend.path/to/secret]
            SECRET[second_backend.nested/dir/secret]
            SECRET[third-backend.another-secret]
            SECRET[non_matching_syntax]
            SECRET[.non.matching.syntax]
        "},
            &mut keys,
        );
        assert_eq!(keys.len(), 3);
        assert!(keys.contains_key("first_backend"));
        assert!(keys.contains_key("second_backend"));
        assert!(keys.contains_key("third-backend"));

        let first_backend_keys = keys.get("first_backend").unwrap();
        assert_eq!(first_backend_keys.len(), 6);
        assert!(first_backend_keys.contains("secret_key"));
        assert!(first_backend_keys.contains("secret-key"));
        assert!(first_backend_keys.contains("another_secret_key"));
        assert!(first_backend_keys.contains("a_third.secret_key"));
        assert!(first_backend_keys.contains("..an_extra_secret_key"));
        assert!(first_backend_keys.contains("path/to/secret"));

        let second_backend_keys = keys.get("second_backend").unwrap();
        assert_eq!(second_backend_keys.len(), 3);
        assert!(second_backend_keys.contains("secret_key"));
        assert!(second_backend_keys.contains("secret.key"));
        assert!(second_backend_keys.contains("nested/dir/secret"));

        let third_backend_keys = keys.get("third-backend").unwrap();
        assert_eq!(third_backend_keys.len(), 2);
        assert!(third_backend_keys.contains("secret_key"));
        assert!(third_backend_keys.contains("another-secret"));
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
}
