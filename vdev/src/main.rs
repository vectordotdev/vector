#[macro_use]
mod macros;
mod app;
mod commands;
mod config;
mod git;
mod platform;
mod testing;

use anyhow::Result;
use clap::Parser;
use std::env;

use commands::Cli;

fn main() -> Result<()> {
    let cli = Cli::parse();

    app::set_global_verbosity(cli.verbose.log_level_filter());
    app::set_global_config(config::load()?);

    let path = if !app::config().repo.is_empty() {
        app::config().repo.to_string()
    } else {
        match env::current_dir() {
            Ok(p) => p.display().to_string(),
            Err(_) => ".".to_string(),
        }
    };
    app::set_global_path(path);

    cli.exec()
}
