use anyhow::Result;
use clap::Args;

use crate::app::Application;

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
    pub fn exec(&self, app: &Application) -> Result<()> {
        let mut command = app.repo.command("cargo");
        command.args(["build", "--no-default-features"]);

        if self.release {
            command.arg("--release");
        }

        command.arg("--features");
        if !self.feature.is_empty() {
            command.args([self.feature.join(",")]);
        } else {
            if app.platform.windows() {
                command.arg("default-msvc");
            } else {
                command.arg("default");
            }
        };

        if let Some(target) = self.target.as_deref() {
            command.args(["--target", target]);
        } else {
            command.args(["--target", &app.platform.default_target()]);
        };

        let status = command.status()?;
        if !status.success() {
            app.abort(format!("failed with exit code: {status}"));
        }

        Ok(())
    }
}
