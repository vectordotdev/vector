//! Manage Vector.

use crate::{helm_values_file::HelmValuesFile, resource_file::ResourceFile, up_down, Result};
use std::process::{Command, Stdio};

/// Parameters required to build a `kubectl` command to manage Vector in the
/// Kubernetes cluster.
#[derive(Debug)]
pub struct CommandBuilder {
    interface_command: String,
    namespace: String,
    custom_helm_values_file: Option<HelmValuesFile>,
    custom_resource_file: Option<ResourceFile>,
}

impl up_down::CommandBuilder for CommandBuilder {
    fn build(&self, command_to_build: up_down::CommandToBuild) -> Command {
        let mut command = Command::new(&self.interface_command);
        command
            .arg(match command_to_build {
                up_down::CommandToBuild::Up => "up",
                up_down::CommandToBuild::Down => "down",
            })
            .arg(&self.namespace)
            .stdin(Stdio::null());

        if let Some(ref custom_helm_values_file) = self.custom_helm_values_file {
            command.env("CUSTOM_HELM_VALUES_FILE", custom_helm_values_file.path());
        }

        if let Some(ref custom_resource_file) = self.custom_resource_file {
            command.env("CUSTOM_RESOURCE_CONFIGS_FILE", custom_resource_file.path());
        }

        command
    }
}

/// Vector configuration to deploy.
#[derive(Debug, Default)]
pub struct Config<'a> {
    /// Custom Helm values to set, in the YAML format.
    /// Set to empty to opt-out of passing any custom values.
    pub custom_helm_values: &'a str,

    /// Custom Kubernestes resource(s) to deploy together with Vector.
    /// Set to empty to opt-out of deploying custom resources.
    pub custom_resource: &'a str,
}

/// Takes care of deploying Vector into the Kubernetes cluster.
///
/// Manages the config file secret accordingly.
pub fn manager(
    interface_command: &str,
    namespace: &str,
    config: Config<'_>,
) -> Result<up_down::Manager<CommandBuilder>> {
    let Config {
        custom_helm_values,
        custom_resource,
    } = config;
    let custom_helm_values_file = if custom_helm_values.is_empty() {
        None
    } else {
        Some(HelmValuesFile::new(custom_helm_values)?)
    };
    let custom_resource_file = if custom_resource.is_empty() {
        None
    } else {
        Some(ResourceFile::new(custom_resource)?)
    };
    Ok(up_down::Manager::new(CommandBuilder {
        interface_command: interface_command.to_owned(),
        namespace: namespace.to_owned(),
        custom_helm_values_file,
        custom_resource_file,
    }))
}
