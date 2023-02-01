//! Manage Vector.

use std::process::{Command, Stdio};

use crate::{helm_values_file::HelmValuesFile, resource_file::ResourceFile, up_down, Result};

/// Parameters required to build `kubectl` & `helm` commands to manage charts deployments in the
/// Kubernetes cluster.
#[derive(Debug)]
pub struct CommandBuilder {
    interface_command: String,
    namespace: String,
    helm_chart: String,
    release_name: String,
    custom_helm_values_files: Vec<HelmValuesFile>,
    custom_resource_file: Option<ResourceFile>,
    custom_env: Option<Vec<(String, String)>>,
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
            .arg(&self.helm_chart)
            .arg(&self.release_name)
            .stdin(Stdio::null());

        command.env(
            "CUSTOM_HELM_VALUES_FILES",
            self.custom_helm_values_files
                .iter()
                .map(|custom_helm_values_file| custom_helm_values_file.path().to_string_lossy())
                .collect::<Vec<_>>()
                .join(" "),
        );

        if let Some(ref custom_resource_file) = self.custom_resource_file {
            command.env("CUSTOM_RESOURCE_CONFIGS_FILE", custom_resource_file.path());
        }
        if let Some(env) = &self.custom_env {
            for envvar in env {
                command.env(envvar.0.clone(), envvar.1.clone());
            }
        }
        command
    }
}

/// Vector configuration to deploy.
#[derive(Debug, Default)]
pub struct Config<'a> {
    /// Custom Helm values to set, in the YAML format.
    /// Set to empty to opt-out of passing any custom values.
    pub custom_helm_values: Vec<&'a str>,

    /// Custom Kubernetes resource(s) to deploy together with Vector.
    /// Set to empty to opt-out of deploying custom resources.
    pub custom_resource: &'a str,
}

/// Takes care of deploying Vector into the Kubernetes cluster.
///
/// Manages the config file secret accordingly, accept additional env var
pub fn manager(
    interface_command: &str,
    namespace: &str,
    helm_chart: &str,
    release_name: &str,
    config: Config<'_>,
    custom_env: Option<Vec<(String, String)>>,
) -> Result<up_down::Manager<CommandBuilder>> {
    let Config {
        custom_helm_values,
        custom_resource,
    } = config;
    let custom_helm_values_files = custom_helm_values
        .into_iter()
        .map(HelmValuesFile::new)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let custom_resource_file = if custom_resource.is_empty() {
        None
    } else {
        Some(ResourceFile::new(custom_resource)?)
    };
    Ok(up_down::Manager::new(CommandBuilder {
        interface_command: interface_command.to_owned(),
        namespace: namespace.to_owned(),
        helm_chart: helm_chart.to_owned(),
        release_name: release_name.to_owned(),
        custom_helm_values_files,
        custom_resource_file,
        custom_env,
    }))
}
