use anyhow::Result;
use clap::Args;

use crate::app;

/// Locate the config file
#[derive(Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(&self) -> Result<()> {
        display!("{}", app::config_file().path().display());

        Ok(())
    }
}
