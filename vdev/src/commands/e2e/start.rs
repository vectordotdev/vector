use anyhow::Result;
use clap::Args;

use crate::testing::integration::ComposeTestLocalConfig;

/// Start an environment
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The e2e test name
    test: String,

    /// Whether to compile the test runner with all integration test features
    #[arg(short = 'a', long)]
    build_all: bool,

    /// The desired environment name to start. If omitted, the first environment name is used.
    environment: Option<String>,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        crate::commands::compose_tests::start::exec(
            ComposeTestLocalConfig::e2e(),
            &self.test,
            self.environment.as_ref(),
            self.build_all,
        )
    }
}
