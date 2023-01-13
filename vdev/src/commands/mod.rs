use anyhow::Result;
use clap::{Parser, Subcommand};
use clap_verbosity_flag::{InfoLevel, Verbosity};

mod build;
mod complete;
mod config;
mod exec;
mod features;
mod integration;
mod meta;
mod run;
mod status;
mod test;

/// Vector's unified dev tool
#[derive(Parser, Debug)]
#[command(
    version,
    bin_name = "vdev",
    infer_subcommands = true,
    disable_help_subcommand = true,
    after_help = r#"Environment variables:
  $CONTAINER_TOOL  Set the tool used to run containers (Defaults to autodetect)
                   Valid values are either "docker" or "podman".
"#
)]
pub struct Cli {
    #[clap(flatten)]
    pub verbose: Verbosity<InfoLevel>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Build(build::Cli),
    Complete(complete::Cli),
    Config(config::Cli),
    Exec(exec::Cli),
    Features(features::Cli),
    Integration(integration::Cli),
    Meta(meta::Cli),
    Run(run::Cli),
    Status(status::Cli),
    Test(test::Cli),
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        match self.command {
            Commands::Build(cli) => cli.exec(),
            Commands::Complete(cli) => cli.exec(),
            Commands::Config(cli) => cli.exec(),
            Commands::Exec(cli) => cli.exec(),
            Commands::Features(cli) => cli.exec(),
            Commands::Integration(cli) => cli.exec(),
            Commands::Meta(cli) => cli.exec(),
            Commands::Run(cli) => cli.exec(),
            Commands::Status(cli) => cli.exec(),
            Commands::Test(cli) => cli.exec(),
        }
    }
}
