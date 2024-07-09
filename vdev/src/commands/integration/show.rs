use anyhow::Result;
use clap::Args;

use crate::testing::config::INTEGRATION_TESTS_DIR;

/// Show information about integrations
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The desired integration
    integration: Option<String>,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        crate::commands::compose_tests::show::exec(&self.integration, INTEGRATION_TESTS_DIR)
    }
}
