#![allow(missing_docs)]
use std::collections::{HashMap, HashSet};

use enum_dispatch::enum_dispatch;
use vector_lib::configurable::{configurable_component, NamedComponent};

use crate::{config::SecretBackend, signal};

#[cfg(feature = "secrets-aws-secrets-manager")]
mod aws_secrets_manager;
mod exec;
mod file;
mod test;

/// Configurable secret backends in Vector.
#[allow(clippy::large_enum_variant)]
#[configurable_component]
#[derive(Clone, Debug)]
#[enum_dispatch(SecretBackend)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SecretBackends {
    /// File.
    File(file::FileBackend),

    /// Exec.
    Exec(exec::ExecBackend),

    /// AWS Secrets Manager.
    #[cfg(feature = "secrets-aws-secrets-manager")]
    AwsSecretsManager(aws_secrets_manager::AwsSecretsManagerBackend),

    /// Test.
    #[configurable(metadata(docs::hidden))]
    Test(test::TestBackend),
}

// TODO: Use `enum_dispatch` here.
impl NamedComponent for SecretBackends {
    fn get_component_name(&self) -> &'static str {
        match self {
            Self::File(config) => config.get_component_name(),
            Self::Exec(config) => config.get_component_name(),
            #[cfg(feature = "secrets-aws-secrets-manager")]
            Self::AwsSecretsManager(config) => config.get_component_name(),
            Self::Test(config) => config.get_component_name(),
        }
    }
}
