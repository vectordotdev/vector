use anyhow::Result;
use clap::{Args, Subcommand};

mod show;
mod start;
mod stop;
mod test;

/// Manage integrations
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Show(show::Cli),
    Start(start::Cli),
    Stop(stop::Cli),
    Test(test::Cli),
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        match self.command {
            Commands::Show(cli) => cli.exec(),
            Commands::Start(cli) => cli.exec(),
            Commands::Stop(cli) => cli.exec(),
            Commands::Test(cli) => cli.exec(),
        }
    }
}
