use std::process::Command;

use anyhow::Result;
use clap::Args;

use crate::app::CommandExt as _;
use crate::platform;

/// Build Vector
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The build target e.g. x86_64-unknown-linux-musl
    target: Option<String>,

    /// Build with optimizations
    #[arg(short, long)]
    release: bool,

    /// The feature to activate (multiple allowed)
    #[arg(short = 'F', long)]
    feature: Vec<String>,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let mut command = Command::new("cargo");
        command.in_repo();
        command.args(["build", "--no-default-features"]);

        if self.release {
            command.arg("--release");
        }

        command.arg("--features");
        if self.feature.is_empty() {
            command.arg(platform::default_features());
        } else {
            command.arg(self.feature.join(","));
        }

        let target = self.target.unwrap_or_else(platform::default_target);
        command.args(["--target", &target]);

        waiting!("Building Vector");
        command.check_run()?;

        Ok(())
    }
}
