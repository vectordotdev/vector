//! Manage test pods.

use std::process::{Command, Stdio};

use k8s_openapi::api::core::v1::Pod;

use super::{resource_file::ResourceFile, Result};
use crate::up_down;

/// A config that holds a test `Pod` resource file.
#[derive(Debug)]
pub struct Config {
    test_pod_resource_file: ResourceFile,
}

impl Config {
    /// Create a [`Config`] using a structured [`Pod`] object.
    pub fn from_pod(pod: &Pod) -> Result<Self> {
        Self::from_resource_string(serde_json::to_string(pod)?.as_str())
    }

    /// Create a [`Config`] using an unstructured resource string.
    pub fn from_resource_string(resource: &str) -> Result<Self> {
        let test_pod_resource_file = ResourceFile::new(resource)?;
        Ok(Self {
            test_pod_resource_file,
        })
    }
}

/// Parameters required to build a `kubectl` command to manage the test `Pod`.
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
                up_down::CommandToBuild::Up => "create",
                up_down::CommandToBuild::Down => "delete",
            })
            .arg("-f")
            .arg(self.config.test_pod_resource_file.path());

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

/// Create a new [`up_down::Manager`] with the specified `config` and using
/// the specified `kubectl_command`.
pub fn manager(kubectl_command: &str, config: Config) -> up_down::Manager<CommandBuilder> {
    up_down::Manager::new(CommandBuilder {
        kubectl_command: kubectl_command.to_owned(),
        config,
    })
}
