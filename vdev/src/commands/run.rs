use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::Args;

use crate::{app::VDevCommand, features};

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

    /// Name an additional feature to add to the build
    #[arg(short = 'F', long)]
    feature: Vec<String>,

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

        let mut features = features::load_and_extract(&self.config)?;
        features.extend(self.feature);
        let features = features.join(",");
        let mut command = VDevCommand::new("cargo").args([
            "run",
            "--no-default-features",
            "--features",
            &features,
        ]);
        if self.release {
            command = command.arg("--release");
        }
        command
            .args([
                "--",
                "--config",
                self.config.to_str().expect("Invalid config file name"),
            ])
            .args(self.args)
            .check_run()
    }
}
