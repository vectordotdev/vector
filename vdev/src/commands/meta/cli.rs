use clap::{Args, Subcommand};

use crate::app::Application;
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
    pub fn exec(&self, app: &Application) {
        match &self.command {
            Commands::Starship(cli) => cli.exec(&app),
        }
    }
}
