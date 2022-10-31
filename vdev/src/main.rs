mod app;
mod commands;
mod config;
mod platform;
mod repo;

use crate::app::Application;
use crate::commands::cli::Cli;
use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    let cli = Cli::parse();
    env_logger::Builder::new()
        .filter_level(cli.verbose.log_level_filter())
        .init();

    let app = Application::new(cli.verbose.log_level_filter());

    cli.exec(&app)
}
