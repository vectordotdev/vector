use anyhow::Result;

use crate::util;

/// Determine the appropriate release channel (nightly or latest) based on Git HEAD.
// This script is used across various release scripts to determine where distribute archives,
// packages, etc.
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        println!("{}", util::release_channel()?);
        Ok(())
    }
}
