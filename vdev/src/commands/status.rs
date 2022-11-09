use anyhow::Result;
use clap::Args;

use crate::app;
use crate::git;
use crate::testing::config::IntegrationTestConfig;

/// Show information about the current environment
#[derive(Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(&self) -> Result<()> {
        display!("Branch: {}", git::current_branch()?);

        let configs = IntegrationTestConfig::collect_all(app::path())?;
        let mut changed = vec![];
        for (integration, config) in configs.iter() {
            if config.triggered(git::changed_files()?)? {
                changed.push(integration.to_string());
            }
        }
        if !changed.is_empty() {
            display!("Changed:");
            for integration in changed.iter() {
                display!("  {}", integration);
            }
        }

        Ok(())
    }
}
