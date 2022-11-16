mod core;

use anyhow::Result;
use clap::{Parser, Subcommand};

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

fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Start { json } => core::start(serde_json::from_str(&json)?),
        Commands::Stop { json } => core::stop(serde_json::from_str(&json)?),
    }
}
