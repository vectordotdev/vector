use anyhow::Result;
use clap::{Args, Subcommand};

mod find;
mod set;

/// Manage the vdev config file
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Find(find::Cli),
    Set(set::Cli),
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        match self.command {
            Commands::Find(cli) => cli.exec(),
            Commands::Set(cli) => cli.exec(),
        }
    }
}
