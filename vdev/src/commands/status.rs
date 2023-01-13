use anyhow::Result;
use clap::Args;

use crate::git;

/// Show information about the current environment
#[derive(Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        display!("Branch: {}", git::current_branch()?);
        display!("Changed files: {}", git::changed_files()?.len());

        Ok(())
    }
}
