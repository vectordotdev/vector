use anyhow::Result;
use clap::Args;

use crate::testing::integration::IntegrationTest;

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
        IntegrationTest::new(self.integration, self.environment)?.start()
    }
}
