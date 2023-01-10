use anyhow::Result;
use clap::{Args, Subcommand};

mod starship;

/// Collection of useful utilities
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Starship(starship::Cli),
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        match self.command {
            Commands::Starship(cli) => cli.exec(),
        }
    }
}
