use anyhow::Result;
use clap::{Parser, Subcommand};
use clap_verbosity_flag::{InfoLevel, Verbosity};

use crate::commands;

/// Vector's unified dev tool
#[derive(Parser, Debug)]
#[
    command(
        bin_name = "vdev",
        author,
        version,
        about,
        disable_help_subcommand = true,
        long_about = None,
    )
]
pub struct Cli {
    #[clap(flatten)]
    pub(crate) verbose: Verbosity<InfoLevel>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Build Vector
    Build(commands::build::Cli),
    /// Manage the vdev config file
    Config(commands::config::cli::Cli),
    /// Execute a command within the repository
    Exec(commands::exec::Cli),
    /// Manage integrations
    Int(commands::int::cli::Cli),
    /// Collection of useful utilities
    Meta(commands::meta::cli::Cli),
    /// Show information about the current environment
    Status(commands::status::Cli),
    /// Execute tests
    Test(commands::test::Cli),
}

impl Cli {
    pub fn exec(&self) -> Result<()> {
        match &self.command {
            Commands::Build(cli) => cli.exec(),
            Commands::Config(cli) => cli.exec(),
            Commands::Exec(cli) => cli.exec(),
            Commands::Int(cli) => cli.exec(),
            Commands::Meta(cli) => cli.exec(),
            Commands::Status(cli) => cli.exec(),
            Commands::Test(cli) => cli.exec(),
        }
    }
}
