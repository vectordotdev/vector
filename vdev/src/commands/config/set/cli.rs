use anyhow::Result;
use clap::{Args, Subcommand};

/// Modify the config file
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Org(super::org::Cli),
    Repo(super::repo::Cli),
}

impl Cli {
    pub fn exec(&self) -> Result<()> {
        match &self.command {
            Commands::Org(cli) => cli.exec(),
            Commands::Repo(cli) => cli.exec(),
        }
    }
}
