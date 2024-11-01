use anyhow::Result;
use clap::Args;

use crate::testing::integration::IntegrationTest;

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

    /// Whether to compile the test runner with all integration test features
    #[arg(short = 'a', long)]
    build_all: bool,

    /// Number of retries to allow on each integration test case.
    #[arg(short = 'r', long)]
    retries: Option<u8>,

    /// Extra test command arguments
    args: Vec<String>,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        crate::commands::compose_tests::test::exec::<IntegrationTest>(
            &self.integration,
            &self.environment,
            self.build_all,
            self.retries.unwrap_or_default(),
            &self.args,
        )
    }
}
