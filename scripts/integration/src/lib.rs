pub use anyhow::Result;
use clap::{Parser, Subcommand};
use serde_json::Value;

#[derive(Parser, Debug)]
#[command(disable_help_subcommand = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Start { json: String },
    Stop { json: String },
}

pub fn docker_main(
    start: impl Fn(Value) -> Result<()>,
    stop: impl Fn(Value) -> Result<()>,
) -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Start { json } => start(serde_json::from_str(&json)?),
        Commands::Stop { json } => stop(serde_json::from_str(&json)?),
    }
}
