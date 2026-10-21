use std::{path::PathBuf, process::Command};

use anyhow::Result;
use clap::Args;

use crate::{app::CommandExt as _, utils::features};

/// Run `vector` with the minimum set of features required by the config file
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// Build and run `vector` in debug mode (default)
    #[arg(long)]
    debug: bool,

    /// Build and run `vector` in release mode
    #[arg(long, conflicts_with = "debug")]
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
        let mut features = features::load_and_extract(&self.config)?;
        features.extend(self.feature);
        let features = features.join(",");
        let mut command = Command::new("cargo");
        command.args([
            "run",
            "--package",
            "vector",
            "--no-default-features",
            "--features",
            &features,
        ]);
        if self.release {
            command.arg("--release");
        }
        command.args([
            "--",
            "--config",
            self.config.to_str().expect("Invalid config file name"),
        ]);
        command.args(self.args);
        command.check_run()
    }
}
