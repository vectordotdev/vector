use anyhow::Result;
use clap::Args;

use crate::git;

/// Show information about the current environment
#[derive(Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        println!("Branch: {}", git::current_branch()?);
        println!("Changed files: {}", git::changed_files()?.len());

        Ok(())
    }
}
