use anyhow::Result;
use clap::{Parser, Subcommand};
use clap_verbosity_flag::{InfoLevel, Verbosity};

use crate::app::Application;
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
    /// Collection of useful utilities
    Meta(commands::meta::cli::Cli),
    /// Show information about the current environment
    Status(commands::status::Cli),
}

impl Cli {
    pub fn exec(&self, app: &Application) -> Result<()> {
        match &self.command {
            Commands::Build(cli) => cli.exec(&app),
            Commands::Config(cli) => cli.exec(&app),
            Commands::Exec(cli) => cli.exec(&app),
            Commands::Meta(cli) => cli.exec(&app),
            Commands::Status(cli) => cli.exec(&app),
        }
    }
}
