use anyhow::Result;
use clap::Args;

use crate::testing::integration::ComposeTestLocalConfig;

/// Start an environment
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The e2e test name
    test: String,

    /// Use only the features defined in test.yaml (e.g., scripts/e2e/<test-name>/test.yaml)
    /// instead of the shared 'all-e2e-tests' feature. Defaults to false for better image reuse across tests.
    #[arg(long)]
    test_yaml_features: bool,

    /// The desired environment name to start. If omitted, the first environment name is used.
    environment: Option<String>,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        crate::commands::compose_tests::start::exec(
            ComposeTestLocalConfig::e2e(),
            &self.test,
            self.environment.as_ref(),
            !self.test_yaml_features,
        )
    }
}
