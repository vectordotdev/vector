use anyhow::Result;

use crate::app;

/// Apply format changes across the repository
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        app::exec("scripts/check-style.sh", ["--fix"], true)?;
        // We are using nightly features in `.rustfmt.toml
        app::exec("cargo", ["+nightly", "fmt", "--all"], true)
    }
}
