//! Manage Vector.

use crate::{helm_values_file::HelmValuesFile, resource_file::ResourceFile, up_down, Result};
use std::process::{Command, Stdio};

/// Parameters required to build a `kubectl` command to manage Vector in the
/// Kubernetes cluster.
#[derive(Debug)]
pub struct CommandBuilder {
    interface_command: String,
    namespace: String,
    custom_resource_file: ResourceFile,
    custom_helm_values_file: Option<HelmValuesFile>,
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
            .env(
                "CUSTOM_RESOURCE_CONFIGS_FILE",
                self.custom_resource_file.path(),
            )
            .stdin(Stdio::null());

        if let Some(ref custom_helm_values_file) = self.custom_helm_values_file {
            command.env("CUSTOM_HELM_VALUES_FILE", custom_helm_values_file.path());
        }

        command
    }
}

/// Takes care of deploying Vector into the Kubernetes cluster.
///
/// Manages the config file secret accordingly.
pub fn manager(
    interface_command: &str,
    namespace: &str,
    custom_resource: &str,
    custom_helm_values: &str,
) -> Result<up_down::Manager<CommandBuilder>> {
    let custom_resource_file = ResourceFile::new(custom_resource)?;
    let custom_helm_values_file = if custom_helm_values.is_empty() {
        None
    } else {
        Some(HelmValuesFile::new(custom_helm_values)?)
    };
    Ok(up_down::Manager::new(CommandBuilder {
        interface_command: interface_command.to_owned(),
        namespace: namespace.to_owned(),
        custom_resource_file,
        custom_helm_values_file,
    }))
}
