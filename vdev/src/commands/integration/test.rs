use anyhow::Result;
use clap::Args;

use crate::testing::integration::ComposeTestLocalConfig;

/// Execute integration tests
///
/// If an environment is named, it is used to run the test. If the environment was not previously started,
/// it is started before the test is run and stopped afterwards.
///
/// If no environment is named, but one has been started already, that environment is used for the test.
///
/// Otherwise, all environments are started, the test run, and then stopped, one by one.
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The desired integration
    integration: String,

    /// The desired environment (optional)
    environment: Option<String>,

    /// Use only the features defined in test.yaml (e.g., scripts/integration/<test-name>/test.yaml)
    /// instead of the shared 'all-integration-tests' feature. Defaults to false for better image reuse across tests.
    #[arg(long)]
    test_yaml_features: bool,

    /// Number of retries to allow on each integration test case.
    #[arg(short = 'r', long)]
    retries: Option<u8>,

    /// Extra test command arguments
    args: Vec<String>,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        crate::commands::compose_tests::test::exec(
            ComposeTestLocalConfig::integration(),
            &self.integration,
            self.environment.as_ref(),
            !self.test_yaml_features,
            self.retries.unwrap_or_default(),
            &self.args,
        )
    }
}
