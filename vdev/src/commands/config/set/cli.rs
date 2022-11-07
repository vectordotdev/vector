use anyhow::Result;
use clap::{Args, Subcommand};

use crate::commands;

/// Modify the config file
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Set the target Datadog org
    Org(commands::config::set::org::Cli),
    /// Set the path to the Vector repository
    Repo(commands::config::set::repo::Cli),
}

impl Cli {
    pub fn exec(&self) -> Result<()> {
        match &self.command {
            Commands::Org(cli) => cli.exec(),
            Commands::Repo(cli) => cli.exec(),
        }
    }
}
