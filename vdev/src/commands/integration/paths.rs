use anyhow::Result;
use clap::Args;

use crate::testing::config::IntegrationTestConfig;

/// List file system paths relevant to an integration.
/// If any changes are made to these paths, that integration should be tested.
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The integration to list paths for.
    integration: String,
}

impl Cli {
    pub fn exec(&self) -> Result<()> {
        let (_test_dir, config) = IntegrationTestConfig::load(&self.integration)?;

        for path in config.paths.unwrap_or_default() {
            println!("{path}");
        }

        Ok(())
    }
}
