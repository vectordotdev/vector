use anyhow::Result;
use clap::Args;

use crate::testing::integration::ComposeTestLocalConfig;

/// Run a complete integration test workflow (CI-style)
///
/// This command orchestrates the full integration test lifecycle:
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
    /// The integration name
    test_name: String,

    /// The desired environment name(s) to run. If omitted, all environments are run.
    /// Can be specified multiple times or comma-separated.
    #[arg(short = 'e', long = "environment", value_delimiter = ',')]
    environments: Vec<String>,

    /// Whether to compile the test runner with all integration test features
    #[arg(short = 'a', long)]
    build_all: bool,

    /// Reuse existing test runner image instead of rebuilding (useful in CI)
    #[arg(long)]
    reuse_image: bool,

    /// Number of retries for the test phase
    #[arg(short = 'r', long, default_value = "2")]
    retries: u8,

    /// Print docker compose logs on failure or when in debug mode
    #[arg(long)]
    show_logs: bool,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        crate::commands::compose_tests::run::exec(
            ComposeTestLocalConfig::integration(),
            &self.test_name,
            &self.environments,
            self.build_all,
            self.reuse_image,
            self.retries,
            self.show_logs,
        )
    }
}
