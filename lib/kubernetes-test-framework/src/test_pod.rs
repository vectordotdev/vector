use super::{resource_file::ResourceFile, Result};
use crate::up_down;
use k8s_openapi::api::core::v1::Pod;
use std::process::{Command, Stdio};

#[derive(Debug)]
pub struct Config {
    custom_resource_file: ResourceFile,
}

impl Config {
    pub fn from_pod(pod: &Pod) -> Result<Self> {
        Self::from_resource_string(serde_json::to_string(pod)?.as_str())
    }

    pub fn from_resource_string(resource: &str) -> Result<Self> {
        let custom_resource_file = ResourceFile::new(resource)?;
        Ok(Self {
            custom_resource_file,
        })
    }
}

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
            .arg(self.config.custom_resource_file.path())
            .stdin(Stdio::null());
        command
    }
}

pub fn manager(kubectl_command: &str, config: Config) -> up_down::Manager<CommandBuilder> {
    up_down::Manager::new(CommandBuilder {
        kubectl_command: kubectl_command.to_owned(),
        config,
    })
}
