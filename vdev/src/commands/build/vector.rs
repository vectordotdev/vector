use std::process::Command;

use anyhow::Result;
use clap::Args;

use crate::{app::CommandExt as _, utils::platform};

/// Build the `vector` executable.
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The build target e.g. x86_64-unknown-linux-musl
    target: Option<String>,

    /// Build with optimizations
    #[arg(short, long)]
    release: bool,

    /// Features to activate (comma-separated, or set FEATURES env var)
    #[arg(short = 'F', long, value_delimiter = ',', env = "FEATURES")]
    features: Vec<String>,

    #[arg(long)]
    no_default_features: bool,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let mut command = Command::new("cargo");
        command.in_repo();
        command.arg("build");

        if self.release {
            command.arg("--release");
        }

        if self.no_default_features {
            command.arg("--no-default-features");
        }
        let features: Vec<String> = self
            .features
            .into_iter()
            .filter(|f| !f.is_empty())
            .collect();
        if !features.is_empty() {
            command.args(["--features", &features.join(",")]);
        }

        let target = self.target.unwrap_or_else(platform::default_target);
        command.args(["--target", &target]);

        waiting!("Building Vector");
        command.check_run()?;

        Ok(())
    }
}
