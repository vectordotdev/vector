use anyhow::Result;

use crate::app;

/// Rebuild the 3rd-party license file.
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        app::exec("dd-rust-license-tool", ["write"], true)
    }
}
