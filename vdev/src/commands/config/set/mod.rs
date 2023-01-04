use anyhow::Result;
use clap::{Args, Subcommand};

mod org;
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
    Org(org::Cli),
    Repo(repo::Cli),
}

impl Cli {
    pub fn exec(&self) -> Result<()> {
        match &self.command {
            Commands::Org(cli) => cli.exec(),
            Commands::Repo(cli) => cli.exec(),
        }
    }
}
