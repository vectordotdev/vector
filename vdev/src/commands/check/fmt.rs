use anyhow::Result;

use crate::app;

/// Check that all files are formatted properly
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        info!("Checking style (trailing spaces, line endings)...");
        app::exec("scripts/check-style.sh", ["--all"], true)?;

        info!("Checking Rust formatting...");
        app::exec("cargo", ["fmt", "--", "--check"], true)
    }
}
