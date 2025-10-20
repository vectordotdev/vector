use anyhow::Result;
use clap::Args;

use crate::testing::integration::ComposeTestLocalConfig;

/// Stop an integration test environment
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The integration name to stop
    integration: String,

    /// Use only the features defined in test.yaml (e.g., scripts/integration/<test-name>/test.yaml)
    /// instead of the shared 'all-integration-tests' feature. Defaults to false for better image reuse across tests.
    #[arg(long)]
    test_yaml_features: bool,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        crate::commands::compose_tests::stop::exec(
            ComposeTestLocalConfig::integration(),
            &self.integration,
            !self.test_yaml_features,
        )
    }
}
