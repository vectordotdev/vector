use anyhow::Result;
use clap::Args;

use crate::{app::VDevCommand, platform};

/// Build the `vector` executable.
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
        let mut command = VDevCommand::new("cargo").in_repo().arg("build");

        if self.release {
            command = command.arg("--release");
        }

        let target = self.target.unwrap_or_else(platform::default_target);
        command = command.features(&self.feature).args(["--target", &target]);

        waiting!("Building Vector");

        command.check_run()
    }
}
