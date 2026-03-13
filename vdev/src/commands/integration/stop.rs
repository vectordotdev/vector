use anyhow::Result;
use clap::Args;

use crate::testing::integration::ComposeTestLocalConfig;

/// Stop an integration test environment
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The integration name to stop
    integration: String,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        crate::commands::compose_tests::stop::exec(
            ComposeTestLocalConfig::integration(),
            &self.integration,
        )
    }
}
