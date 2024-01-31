use anyhow::Result;
use clap::Args;

use crate::testing::config::E2E_TESTS_DIR;

/// Show information about e2e-tests
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The desired e2e test name
    test: Option<String>,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        crate::commands::compose_tests::show::exec(&self.test, E2E_TESTS_DIR)
    }
}
