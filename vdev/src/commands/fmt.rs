use anyhow::Result;

use crate::app;

/// Apply format changes across the repository
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        app::exec("scripts/check-style.sh", ["--fix"], true)?;
        app::exec("cargo", ["fmt"], true)
    }
}
