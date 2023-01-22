use anyhow::Result;
use clap::Args;

use crate::config;

/// Locate the config file
#[derive(Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        display!("{}", config::path()?.display());

        Ok(())
    }
}
