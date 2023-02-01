use std::process::Command;

use super::Result;
use crate::util::{run_command, run_command_blocking};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum CommandToBuild {
    Up,
    Down,
}

pub trait CommandBuilder {
    fn build(&self, command_to_build: CommandToBuild) -> Command;
}

/// Manages commands for bringing up and shutting down resources on the cluster.
#[derive(Debug)]
pub struct Manager<B>
where
    B: CommandBuilder,
{
    command_builder: B,
    needs_drop: bool,
}

impl<B> Manager<B>
where
    B: CommandBuilder,
{
    /// Create a new Manager.
    pub fn new(command_builder: B) -> Self {
        Self {
            command_builder,
            needs_drop: false,
        }
    }

    /// Bring up the resource.
    pub async fn up(&mut self) -> Result<()> {
        self.needs_drop = true;
        self.exec(CommandToBuild::Up).await
    }

    /// Shut down the resource.
    pub async fn down(&mut self) -> Result<()> {
        self.needs_drop = false;
        self.exec(CommandToBuild::Down).await
    }

    /// Bring up the resource, blocking execution.
    pub fn up_blocking(&mut self) -> Result<()> {
        self.needs_drop = true;
        self.exec_blocking(CommandToBuild::Up)
    }

    /// Shut down the resource, blocking execution.
    pub fn down_blocking(&mut self) -> Result<()> {
        self.needs_drop = false;
        self.exec_blocking(CommandToBuild::Down)
    }

    fn build(&self, command_to_build: CommandToBuild) -> Command {
        self.command_builder.build(command_to_build)
    }

    async fn exec(&self, command_to_build: CommandToBuild) -> Result<()> {
        let command = self.build(command_to_build);
        run_command(tokio::process::Command::from(command)).await
    }

    fn exec_blocking(&self, command_to_build: CommandToBuild) -> Result<()> {
        let command = self.build(command_to_build);
        run_command_blocking(command)
    }
}

impl<B> Drop for Manager<B>
where
    B: CommandBuilder,
{
    fn drop(&mut self) {
        if self.needs_drop {
            self.down_blocking().expect("turndown failed");
        }
    }
}
