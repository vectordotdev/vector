use anyhow::Result;
use clap::{Args, Subcommand};

/// Manage the vdev config file
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Find(super::find::Cli),
    Set(super::set::cli::Cli),
}

impl Cli {
    pub fn exec(&self) -> Result<()> {
        match &self.command {
            Commands::Find(cli) => cli.exec(),
            Commands::Set(cli) => cli.exec(),
        }
    }
}
