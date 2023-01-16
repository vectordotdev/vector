use anyhow::Result;
use clap::Args;

use crate::testing::integration;

/// Start an environment
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The desired integration
    integration: String,

    /// The desired environment
    environment: String,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        integration::start(&self.integration, &self.environment)
    }
}
