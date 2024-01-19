use anyhow::Result;
use clap::Args;

use crate::testing::config::E2E_TESTS_DIR;

/// Start an environment
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The e2e-test name
    e2e_test: String,

    /// The desired environment name to start. If omitted, the first environment name is used.
    environment: Option<String>,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        crate::commands::compose_tests::start::exec(
            &self.e2e_test,
            E2E_TESTS_DIR,
            &self.environment,
        )
    }
}
