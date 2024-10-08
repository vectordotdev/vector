use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use vector_lib::configurable::{component::GenerateConfig, configurable_component};

use crate::{config::SecretBackend, signal};

/// Configuration for the `directory` secrets backend.
#[configurable_component(secrets("directory"))]
#[derive(Clone, Debug)]
pub struct DirectoryBackend {
    /// Directory path to read secrets from.
    pub path: PathBuf,

    /// Remove trailing whitespace from file contents.
    #[serde(default)]
    pub remove_trailing_whitespace: bool,
}

impl GenerateConfig for DirectoryBackend {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(DirectoryBackend {
            path: PathBuf::from("/path/to/secrets"),
            remove_trailing_whitespace: false,
        })
        .unwrap()
    }
}

impl SecretBackend for DirectoryBackend {
    async fn retrieve(
        &mut self,
        secret_keys: HashSet<String>,
        _: &mut signal::SignalRx,
    ) -> crate::Result<HashMap<String, String>> {
        let mut secrets = HashMap::new();
        for k in secret_keys.into_iter() {
            let file_path = self.path.join(&k);
            let contents = tokio::fs::read_to_string(&file_path).await?;
            let secret = if self.remove_trailing_whitespace {
                contents.trim_end()
            } else {
                &contents
            };
            if secret.is_empty() {
                return Err(format!("secret in file '{}' was empty", k).into());
            }
            secrets.insert(k, secret.to_string());
        }
        Ok(secrets)
    }
}
