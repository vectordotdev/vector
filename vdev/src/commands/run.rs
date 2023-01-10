use std::path::PathBuf;

use anyhow::{bail, Context as _, Result};
use clap::Args;
use exec::Command;

use crate::features;

/// Run `vector` with the minimum set of features required by the config file
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// Build and run `vector` in debug mode (default)
    #[arg(long, default_value_t = true)]
    debug: bool,

    /// Build and run `vector` in release mode
    #[arg(long)]
    release: bool,

    /// Path to configuration file
    config: PathBuf,

    /// Non-config arguments to `vector`
    args: Vec<String>,
}

impl Cli {
    pub(super) fn exec(self) -> Result<()> {
        if self.debug && self.release {
            bail!("Can only set one of `--debug` and `--release`");
        }
        let mode = if self.release { "--release" } else { "--debug" };

        let features = features::load_and_extract(&self.config)?;

        // exec cargo run "${options[@]}" --no-default-features --features "$features" -- --config "$config" "$@"
        let mut command = Command::new("cargo");
        command.args(&[
            "--no-default-features",
            "--features",
            &features,
            mode,
            "--",
            "--config",
        ]);
        command.arg(self.config);
        command.args(&self.args);

        Err(command.exec()).context("Could not execute `cargo`")
    }
}
