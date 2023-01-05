use anyhow::Result;
use clap::{Args, Subcommand};

mod repo;

/// Modify the config file
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Repo(repo::Cli),
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        match self.command {
            Commands::Repo(cli) => cli.exec(),
        }
    }
}
