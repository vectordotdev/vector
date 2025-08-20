use crate::commands::compose_tests::show;
use crate::testing::config::E2E_TESTS_DIR;
use anyhow::Result;
use clap::Args;

/// Show information about e2e-tests
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The desired e2e test name
    test: Option<String>,

    /// Show only the available environments (newline separated)
    #[arg(short = 'e', long)]
    environments_only: bool,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        if self.environments_only {
            show::show_environments_only(&self.test.expect("test name is required"), E2E_TESTS_DIR)
        } else {
            show::exec(self.test.as_ref(), E2E_TESTS_DIR)
        }
    }
}
