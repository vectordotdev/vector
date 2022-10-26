mod app;
mod commands;
mod config;
mod platform;

use crate::app::Application;
use crate::commands::cli::Cli;
use clap::Parser;

fn main() {
    let cli = Cli::parse();
    env_logger::Builder::new()
        .filter_level(cli.verbose.log_level_filter())
        .init();

    let app = Application::new(cli.verbose.log_level_filter());

    // TODO: some error handling
    cli.exec(&app);
}
