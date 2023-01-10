use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use clap::Args;

use crate::{app, features};

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

        app::exec(
            Path::new("cargo"),
            [
                "--no-default-features",
                "--features",
                &features,
                mode,
                "--",
                "--config",
                self.config.to_str().expect("Invalid config file name"),
            ]
            .into_iter()
            .chain(self.args.iter().map(String::as_str)),
        )
    }
}
