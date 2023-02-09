use anyhow::Result;

use crate::app;

/// Check that all files are formatted properly
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        app::exec::<&str>("scripts/check-style.sh", [], true)?;
        app::exec("cargo", ["fmt", "--", "--check"], true)
    }
}
