use anyhow::Result;
use clap::Args;

use crate::testing::config::INTEGRATION_TESTS_DIR;

/// Stop an integration test environment
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The integration name to stop
    integration: String,

    /// If true, remove the runner container compiled with all integration test features
    #[arg(short = 'a', long)]
    all_features: bool,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        crate::commands::compose_tests::stop::exec(
            &self.integration,
            INTEGRATION_TESTS_DIR,
            self.all_features,
        )
    }
}
