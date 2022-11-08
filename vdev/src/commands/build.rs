use anyhow::{bail, Result};
use clap::Args;

use crate::app;
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
    pub fn exec(&self) -> Result<()> {
        let mut command = app::construct_command("cargo");
        command.args(["build", "--no-default-features"]);

        if self.release {
            command.arg("--release");
        }

        command.arg("--features");
        if !self.feature.is_empty() {
            command.args([self.feature.join(",")]);
        } else {
            if platform::windows() {
                command.arg("default-msvc");
            } else {
                command.arg("default");
            }
        };

        if let Some(target) = self.target.as_deref() {
            command.args(["--target", target]);
        } else {
            command.args(["--target", &platform::default_target()]);
        };

        app::display_waiting("Building Vector");
        app::run_command(&mut command)?;

        Ok(())
    }
}
