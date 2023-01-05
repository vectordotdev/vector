use anyhow::{Context, Result};
use clap::Args;
use std::path::PathBuf;

use crate::app;
use crate::testing::{config::IntegrationTestConfig, state};

/// Show information about integrations
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The desired integration
    integration: Option<String>,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        match self.integration {
            None => {
                let mut entries = vec![];
                let root_dir: PathBuf = [app::path(), "scripts", "integration"].iter().collect();
                for entry in root_dir
                    .read_dir()
                    .with_context(|| format!("failed to read directory {}", root_dir.display()))?
                {
                    let entry = entry?;
                    if entry.path().is_dir() {
                        entries.push(entry.file_name().into_string().unwrap());
                    }
                }
                entries.sort();

                for integration in &entries {
                    display!("{integration}");
                }
            }
            Some(integration) => {
                let (_test_dir, config) = IntegrationTestConfig::load(&integration)?;
                let envs_dir = state::envs_dir(&integration);
                let active_envs = state::active_envs(&envs_dir)?;

                display!("Test args: {}", config.args.join(" "));

                display!("Environments:");
                for environment in config.environments().keys() {
                    if active_envs.contains(environment) {
                        display!("  {} (active)", environment);
                    } else {
                        display!("  {}", environment);
                    }
                }
            }
        }
        Ok(())
    }
}
