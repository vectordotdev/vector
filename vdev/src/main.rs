#![deny(clippy::pedantic, warnings)]

#[macro_use]
mod utils;

mod app;
mod commands;
mod testing;

use anyhow::Result;
use clap::Parser;
use commands::Cli;

fn main() -> Result<()> {
    let cli = Cli::parse();

    app::set_global_verbosity(cli.verbose.log_level_filter());

    let path = utils::paths::find_repo_root()?.display().to_string();
    app::set_global_path(path);

    cli.exec()
}
