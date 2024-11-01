use std::collections::{HashMap, HashSet};

use vector_lib::configurable::configurable_component;

use crate::{config::SecretBackend, signal};

/// Configuration for the `test` secrets backend.
#[configurable_component(secrets("test"))]
#[derive(Clone, Debug, Default)]
pub struct TestBackend {
    /// Fixed value to replace all secrets with.
    pub replacement: String,
}

impl_generate_config_from_default!(TestBackend);

impl SecretBackend for TestBackend {
    async fn retrieve(
        &mut self,
        secret_keys: HashSet<String>,
        _: &mut signal::SignalRx,
    ) -> crate::Result<HashMap<String, String>> {
        Ok(secret_keys
            .into_iter()
            .map(|k| (k, self.replacement.clone()))
            .collect())
    }
}
