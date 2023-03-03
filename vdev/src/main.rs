#![deny(clippy::pedantic, warnings)]
#![allow(
    clippy::module_name_repetitions,
    clippy::print_stdout,
    clippy::unused_self,
    clippy::unnecessary_wraps
)]

#[macro_use]
mod macros;
mod app;
mod commands;
mod config;
mod features;
mod git;
mod platform;
mod testing;
mod util;

use anyhow::Result;
use clap::Parser;
use std::env;

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
