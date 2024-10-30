use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use vector_lib::configurable::{component::GenerateConfig, configurable_component};

use crate::{config::SecretBackend, signal};

/// Configuration for the `file` secrets backend.
#[configurable_component(secrets("file"))]
#[derive(Clone, Debug)]
pub struct FileBackend {
    /// File path to read secrets from.
    pub path: PathBuf,
}

impl GenerateConfig for FileBackend {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(FileBackend {
            path: PathBuf::from("/path/to/secret"),
        })
        .unwrap()
    }
}

impl SecretBackend for FileBackend {
    async fn retrieve(
        &mut self,
        secret_keys: HashSet<String>,
        _: &mut signal::SignalRx,
    ) -> crate::Result<HashMap<String, String>> {
        let contents = tokio::fs::read_to_string(&self.path).await?;
        let output = serde_json::from_str::<HashMap<String, String>>(&contents)?;
        let mut secrets = HashMap::new();
        for k in secret_keys.into_iter() {
            if let Some(secret) = output.get(&k) {
                if secret.is_empty() {
                    return Err(format!("secret for key '{}' was empty", k).into());
                }
                secrets.insert(k, secret.to_string());
            } else {
                return Err(format!("secret for key '{}' was not retrieved", k).into());
            }
        }
        Ok(secrets)
    }
}
