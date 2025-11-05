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
///
/// To pre-build a shared image with all integration features, use `vdev int build` first,
/// then use `--no-build` to skip the build step.
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The desired integration
    integration: String,

    /// The desired environment (optional)
    environment: Option<String>,

    /// Number of retries to allow on each integration test case.
    #[arg(short = 'r', long)]
    retries: Option<u8>,

    /// Skip building the test runner image (use pre-built image)
    #[arg(long)]
    no_build: bool,

    /// Extra test command arguments
    args: Vec<String>,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        crate::commands::compose_tests::test::exec(
            ComposeTestLocalConfig::integration(),
            &self.integration,
            self.environment.as_ref(),
            self.retries.unwrap_or_default(),
            self.no_build,
            &self.args,
        )
    }
}
