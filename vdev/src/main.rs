#![deny(clippy::pedantic, warnings)]

#[macro_use]
mod macros;
mod app;
mod commands;
mod config;
mod environment;
mod features;
mod git;
mod platform;
mod testing;
mod util;

use std::env;

use anyhow::Result;
use clap::Parser;
use commands::Cli;

fn main() -> Result<()> {
    let cli = Cli::parse();

    app::set_global_verbosity(cli.verbose.log_level_filter());
    app::set_global_config(config::load()?);

    let path = if app::config().repo.is_empty() {
        env::current_dir()
            .expect("Could not determine current directory")
            .display()
            .to_string()
    } else {
        app::config().repo.clone()
    };
    app::set_global_path(path);

    cli.exec()
}
