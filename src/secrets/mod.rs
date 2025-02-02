#![allow(missing_docs)]
use std::collections::{HashMap, HashSet};

use enum_dispatch::enum_dispatch;
use vector_lib::configurable::configurable_component;

use crate::config::GenerateConfig;
use crate::{config::SecretBackend, signal};

#[cfg(feature = "secrets-aws-secrets-manager")]
mod aws_secrets_manager;
mod directory;
mod exec;
mod file;
mod test;

/// Configurable secret backends in Vector.
#[allow(clippy::large_enum_variant)]
#[configurable_component(global_option("secrets", "secret type"))]
#[derive(Clone, Debug)]
#[enum_dispatch(SecretBackend)]
#[serde(tag = "type", rename_all = "snake_case")]
#[configurable(metadata(docs::enum_tag_description = "secret type"))]
pub enum SecretBackends {
    /// File.
    File(file::FileBackend),

    /// Directory.
    Directory(directory::DirectoryBackend),

    /// Exec.
    Exec(exec::ExecBackend),

    /// AWS Secrets Manager.
    #[cfg(feature = "secrets-aws-secrets-manager")]
    AwsSecretsManager(aws_secrets_manager::AwsSecretsManagerBackend),

    /// Test.
    #[configurable(metadata(docs::hidden))]
    Test(test::TestBackend),
}

impl GenerateConfig for SecretBackends {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self::File(file::FileBackend {
            path: "path/to/file".into(),
        }))
        .unwrap()
    }
}
