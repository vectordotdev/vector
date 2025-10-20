use anyhow::Result;
use clap::Args;

use crate::testing::integration::ComposeTestLocalConfig;

/// Stop an e2e-test environment
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The e2e test name to stop
    test: String,

    /// Use only the features defined in test.yaml (e.g., scripts/e2e/<test-name>/test.yaml)
    /// instead of the shared 'all-e2e-tests' feature. Defaults to false for better image
    /// reuse across tests.
    #[arg(long)]
    test_yaml_features: bool,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        crate::commands::compose_tests::stop::exec(
            ComposeTestLocalConfig::e2e(),
            &self.test,
            !self.test_yaml_features,
        )
    }
}
