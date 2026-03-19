use anyhow::Result;
use clap::Args;

use crate::testing::integration::ComposeTestLocalConfig;

/// Run a complete end-to-end test workflow (CI-style)
///
/// This command orchestrates the full e2e test lifecycle:
/// 1. Clean up previous test output
/// 2. Start the environment
/// 3. Run tests with retries
/// 4. Upload results to Datadog (in CI)
/// 5. Stop the environment (always, as cleanup)
///
/// This is useful for CI workflows and local testing that mimics CI behavior.
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The e2e test name
    test_name: String,

    /// The desired environment name(s) to run. If omitted, all environments are run.
    /// Can be specified multiple times or comma-separated.
    #[arg(short = 'e', long = "environment", value_delimiter = ',')]
    environments: Vec<String>,

    /// Number of retries for the test phase
    #[arg(short = 'r', long, default_value = "2")]
    retries: u8,

    /// Print docker compose logs on success (logs are always printed on failure)
    #[arg(long)]
    show_logs: bool,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        crate::commands::compose_tests::run::exec(
            ComposeTestLocalConfig::e2e(),
            &self.test_name,
            &self.environments,
            self.retries,
            self.show_logs,
        )
    }
}
