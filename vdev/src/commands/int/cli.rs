use anyhow::Result;
use clap::{Args, Subcommand};

use crate::app::Application;
use crate::commands;

/// Manage integrations
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Show information about integrations
    Show(commands::int::show::Cli),
    /// Start an environment
    Start(commands::int::start::Cli),
    /// Stop an environment
    Stop(commands::int::stop::Cli),
}

impl Cli {
    pub fn exec(&self, app: &Application) -> Result<()> {
        match &self.command {
            Commands::Show(cli) => cli.exec(&app),
            Commands::Start(cli) => cli.exec(&app),
            Commands::Stop(cli) => cli.exec(&app),
        }
    }
}
