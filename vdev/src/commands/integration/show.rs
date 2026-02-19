use crate::{
    commands::compose_tests::show::{exec, exec_environments_only},
    testing::config::INTEGRATION_TESTS_DIR,
};
use anyhow::Result;
use clap::Args;

/// Show information about integrations
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The desired integration
    integration: Option<String>,

    /// Show only the available environments (newline separated)
    #[arg(short = 'e', long)]
    environments_only: bool,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        if self.environments_only {
            exec_environments_only(
                &self.integration.expect("test name is required"),
                INTEGRATION_TESTS_DIR,
            )
        } else {
            exec(self.integration.as_ref(), INTEGRATION_TESTS_DIR)
        }
    }
}
