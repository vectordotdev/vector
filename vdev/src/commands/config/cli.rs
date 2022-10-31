use anyhow::Result;
use clap::{Args, Subcommand};

use crate::app::Application;
use crate::commands;

/// Manage the config file
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Locate the config file
    Find(commands::config::find::Cli),
    /// Modify the config file
    Set(commands::config::set::cli::Cli),
}

impl Cli {
    pub fn exec(&self, app: &Application) -> Result<()> {
        match &self.command {
            Commands::Find(cli) => cli.exec(&app),
            Commands::Set(cli) => cli.exec(&app),
        }
    }
}
