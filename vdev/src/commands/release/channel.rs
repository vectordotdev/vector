use anyhow::Result;

use crate::util;

/// Determine the appropriate release channel (latest, nightly or custom) based on mode arg.
// This script is used across various release scripts to determine where distribute archives,
// packages, etc.
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {
    pub mode: Option<String>,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        debug!("mode {:?}", &self.mode);
        println!("{}", util::release_channel(self.mode.as_ref())?);
        Ok(())
    }
}
