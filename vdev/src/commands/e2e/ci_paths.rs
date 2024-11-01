use anyhow::Result;
use clap::Args;

use crate::testing::config::E2E_TESTS_DIR;

/// Output paths in the repository that are associated with an integration.
/// If any changes are made to these paths, that integration should be tested.
#[derive(Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(&self) -> Result<()> {
        crate::commands::compose_tests::ci_paths::exec(E2E_TESTS_DIR)
    }
}
