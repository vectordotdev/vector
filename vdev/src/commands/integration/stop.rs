use anyhow::Result;
use clap::Args;

use crate::testing::integration;

/// Stop an environment
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The desired integration
    integration: String,

    /// The desired environment
    environment: String,

    /// Use the currently defined configuration if the environment is not up
    #[arg(short, long)]
    force: bool,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        integration::stop(&self.integration, &self.environment, self.force)
    }
}
