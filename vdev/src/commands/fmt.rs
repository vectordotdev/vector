use anyhow::Result;

use crate::app;

/// Apply format changes across the repository
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        app::exec_script("check-style.sh", ["--fix"])?;
        app::exec_in_repo("cargo", ["fmt"])
    }
}
