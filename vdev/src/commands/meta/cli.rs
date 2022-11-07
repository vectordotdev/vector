use anyhow::Result;
use clap::{Args, Subcommand};

use crate::commands;

/// Collection of useful utilities
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Custom Starship prompt plugin
    Starship(commands::meta::starship::Cli),
}

impl Cli {
    pub fn exec(&self) -> Result<()> {
        match &self.command {
            Commands::Starship(cli) => cli.exec(),
        }
    }
}
