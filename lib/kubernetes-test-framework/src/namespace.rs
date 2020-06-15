use crate::up_down;
use std::process::{Command, Stdio};

#[derive(Debug)]
pub struct CommandBuilder {
    kubectl_command: String,
    namespace: String,
}

impl up_down::CommandBuilder for CommandBuilder {
    fn build(&self, command_to_build: up_down::CommandToBuild) -> Command {
        let mut command = Command::new(&self.kubectl_command);
        command
            .arg(match command_to_build {
                up_down::CommandToBuild::Up => "create",
                up_down::CommandToBuild::Down => "delete",
            })
            .arg("namespace")
            .arg(&self.namespace)
            .stdin(Stdio::null());
        command
    }
}

pub fn manager(kubectl_command: &str, namespace: &str) -> up_down::Manager<CommandBuilder> {
    up_down::Manager::new(CommandBuilder {
        kubectl_command: kubectl_command.to_owned(),
        namespace: namespace.to_owned(),
    })
}
