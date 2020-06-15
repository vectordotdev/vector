use super::{resource_file::ResourceFile, Result};
use crate::up_down;
use std::process::{Command, Stdio};

#[derive(Debug)]
pub struct CommandBuilder {
    interface_command: String,
    namespace: String,
    custom_resource_file: ResourceFile,
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
                "CUSTOM_RESOURCE_CONIFGS_FILE",
                self.custom_resource_file.path(),
            )
            .stdin(Stdio::null());
        command
    }
}

/// Takes care of deploying vector into the kubernetes cluster.
///
/// Manages the config file secret accordingly.
pub fn manager(
    interface_command: &str,
    namespace: &str,
    custom_resource: &str,
) -> Result<up_down::Manager<CommandBuilder>> {
    let custom_resource_file = ResourceFile::new(custom_resource)?;
    Ok(up_down::Manager::new(CommandBuilder {
        interface_command: interface_command.to_owned(),
        namespace: namespace.to_owned(),
        custom_resource_file,
    }))
}
