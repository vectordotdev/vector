use anyhow::Result;
use clap::Args;

use crate::testing::integration::IntegrationTest;

/// Start an environment
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The integration name
    integration: String,

    /// Whether to compile the test runner with all integration test features
    #[arg(short = 'a', long)]
    build_all: bool,

    /// The desired environment name to start. If omitted, the first environment name is used.
    environment: Option<String>,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        crate::commands::compose_tests::start::exec::<IntegrationTest>(
            &self.integration,
            &self.environment,
            self.build_all,
        )
    }
}
