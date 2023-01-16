use anyhow::Result;

use crate::app;

/// Check that all files are formatted properly
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        app::exec_in_repo("cargo", ["fmt", "--", "--check"])
    }
}
