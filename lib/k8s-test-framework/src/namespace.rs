//! Manage namespaces.

use std::{
    collections::BTreeMap,
    process::{Command, Stdio},
};

use k8s_openapi::{api::core::v1::Namespace, apimachinery::pkg::apis::meta::v1::ObjectMeta};

use super::{resource_file::ResourceFile, Result};
use crate::up_down;

/// A config that holds a test `Namespace` resource file.
#[derive(Debug)]
pub struct Config {
    test_namespace_resource_file: ResourceFile,
}

impl Config {
    /// Create a [`Config`] using a structured [`Namespace`] object.
    pub fn from_namespace(namespace: &Namespace) -> Result<Self> {
        Self::from_resource_string(serde_json::to_string(namespace)?.as_str())
    }

    /// Create a [`Config`] using an unstructured resource string.
    pub fn from_resource_string(resource: &str) -> Result<Self> {
        let test_namespace_resource_file = ResourceFile::new(resource)?;
        Ok(Self {
            test_namespace_resource_file,
        })
    }
}

/// Parameters required to build a `kubectl` command to manage the namespace.
#[derive(Debug)]
pub struct CommandBuilder {
    kubectl_command: String,
    config: Config,
}

impl up_down::CommandBuilder for CommandBuilder {
    fn build(&self, command_to_build: up_down::CommandToBuild) -> Command {
        let mut command = Command::new(&self.kubectl_command);
        command
            .arg(match command_to_build {
                up_down::CommandToBuild::Up => "apply",
                up_down::CommandToBuild::Down => "delete",
            })
            .arg("--filename")
            .arg(self.config.test_namespace_resource_file.path());

        if matches!(command_to_build, up_down::CommandToBuild::Down) {
            // We don't need a graceful shutdown
            command.arg("--force=true");
            command.arg("--grace-period=0");
            command.arg("--wait=false");
        }

        command.stdin(Stdio::null());
        command
    }
}

/// Create a new [`up_down::Manager`] for the specified `namespace` and using
/// the specified `kubectl_command`.
pub fn manager(kubectl_command: &str, config: Config) -> up_down::Manager<CommandBuilder> {
    up_down::Manager::new(CommandBuilder {
        kubectl_command: kubectl_command.to_owned(),
        config,
    })
}

/// Helper to create a Namespace resource during tests
pub fn make_namespace(name: String, labels: Option<BTreeMap<String, String>>) -> Namespace {
    Namespace {
        metadata: ObjectMeta {
            name: Some(name),
            labels,
            ..Default::default()
        },
        spec: None,
        status: None,
    }
}
