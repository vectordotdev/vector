use anyhow::Result;
use clap::Args;

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
                let entries = IntegrationTestConfig::collect_all()?;
                let width = entries
                    .keys()
                    .fold(0, |width, entry| width.max(entry.len()));
                for (integration, config) in entries {
                    let environments = config
                        .environments()
                        .keys()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(" ");
                    display!("{integration:width$}  {environments}");
                }
            }
            Some(integration) => {
                let (_test_dir, config) = IntegrationTestConfig::load(&integration)?;
                let envs_dir = state::EnvsDir::new(&integration);
                let active_envs = envs_dir.list_active()?;

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
