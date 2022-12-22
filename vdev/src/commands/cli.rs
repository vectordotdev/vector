use anyhow::Result;
use clap::{Parser, Subcommand};
use clap_verbosity_flag::{InfoLevel, Verbosity};

/// Vector's unified dev tool
#[derive(Parser, Debug)]
#[command(
    version,
    bin_name = "vdev",
    disable_help_subcommand = true,
    infer_subcommands = true
)]
pub struct Cli {
    #[clap(flatten)]
    pub verbose: Verbosity<InfoLevel>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Build(super::build::Cli),
    Complete(super::complete::Cli),
    Config(super::config::cli::Cli),
    Exec(super::exec::Cli),
    Integrations(super::integrations::cli::Cli),
    Meta(super::meta::cli::Cli),
    Status(super::status::Cli),
    Test(super::test::Cli),
}

impl Cli {
    pub fn exec(&self) -> Result<()> {
        match &self.command {
            Commands::Build(cli) => cli.exec(),
            Commands::Complete(cli) => cli.exec(),
            Commands::Config(cli) => cli.exec(),
            Commands::Exec(cli) => cli.exec(),
            Commands::Integrations(cli) => cli.exec(),
            Commands::Meta(cli) => cli.exec(),
            Commands::Status(cli) => cli.exec(),
            Commands::Test(cli) => cli.exec(),
        }
    }
}
