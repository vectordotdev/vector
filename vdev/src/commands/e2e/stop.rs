use anyhow::Result;
use clap::Args;

use crate::testing::config::E2E_TESTS_DIR;

/// Stop an e2e-test environment
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The e2e-test name to stop
    e2e_test: String,

    /// If true, remove the runner container compiled with all integration test features
    #[arg(short = 'a', long)]
    all_features: bool,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        crate::commands::compose_tests::stop::exec(&self.e2e_test, E2E_TESTS_DIR, self.all_features)
    }
}
