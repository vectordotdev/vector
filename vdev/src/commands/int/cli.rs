use anyhow::Result;
use clap::{Args, Subcommand};

/// Manage integrations
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Show(super::show::Cli),
    Start(super::start::Cli),
    Stop(super::stop::Cli),
    Test(super::test::Cli),
}

impl Cli {
    pub fn exec(&self) -> Result<()> {
        match &self.command {
            Commands::Show(cli) => cli.exec(),
            Commands::Start(cli) => cli.exec(),
            Commands::Stop(cli) => cli.exec(),
            Commands::Test(cli) => cli.exec(),
        }
    }
}
