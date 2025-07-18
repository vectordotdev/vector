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

///	Configuration options to retrieve secrets from external backend in order to avoid storing secrets in plaintext
/// in Vector config. Multiple backends can be configured. Use `SECRET[<backend_name>.<secret_key>]` to tell Vector to retrieve the secret. This placeholder is replaced by the secret
/// retrieved from the relevant backend.
///
/// When `type` is `exec`, the provided command will be run and provided a list of
/// secrets to fetch, determined from the configuration file, on stdin as JSON in the format:
///
/// ```json
/// {"version": "1.0", "secrets": ["secret1", "secret2"]}
/// ```
///
/// The executable is expected to respond with the values of these secrets on stdout, also as JSON, in the format:
///
/// ```json
/// {
///     "secret1": {"value": "secret_value", "error": null},
///     "secret2": {"value": null, "error": "could not fetch the secret"}
/// }
/// ```
/// If an `error` is returned for any secrets, or if the command exits with a non-zero status code,
/// Vector will log the errors and exit.
///
/// Otherwise, the secret must be a JSON text string with key/value pairs. For example:
/// ```json
/// {
///     "username": "test",
///     "password": "example-password"
/// }
/// ```
///
/// If an error occurred while reading the file or retrieving the secrets, Vector logs the error and exits.
///
/// Secrets are loaded when Vector starts or if Vector receives a `SIGHUP` signal triggering its
/// configuration reload process.
#[allow(clippy::large_enum_variant)]
#[configurable_component(global_option("secret"))]
#[derive(Clone, Debug)]
#[enum_dispatch(SecretBackend)]
#[serde(tag = "type", rename_all = "snake_case")]
#[configurable(metadata(
    docs::enum_tag_description = "secret type",
    docs::common = false,
    docs::required = false,
))]
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
