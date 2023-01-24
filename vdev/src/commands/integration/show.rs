use std::collections::HashSet;

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
                    .fold(16, |width, entry| width.max(entry.len()));
                println!("{:width$}  Environment Name(s)", "Integration Name");
                println!("{:width$}  -------------------", "----------------");
                for (integration, config) in entries {
                    let envs_dir = state::EnvsDir::new(&integration);
                    let active_envs = envs_dir.list_active()?;
                    let environments = config
                        .environments()
                        .keys()
                        .map(|environment| format(&active_envs, environment))
                        .collect::<Vec<_>>()
                        .join("  ");
                    println!("{integration:width$}  {environments}");
                }
            }
            Some(integration) => {
                let (_test_dir, config) = IntegrationTestConfig::load(&integration)?;
                let envs_dir = state::EnvsDir::new(&integration);
                let active_envs = envs_dir.list_active()?;

                println!("Test args: {}", config.args.join(" "));

                println!("Environments:");
                for environment in config.environments().keys() {
                    let active = if active_envs.contains(environment) {
                        " (active)"
                    } else {
                        ""
                    };
                    println!("  {environment}{active}");
                }
            }
        }
        Ok(())
    }
}

fn format(active_envs: &HashSet<String>, environment: &str) -> String {
    if active_envs.contains(environment) {
        format!("{environment} (active)")
    } else {
        environment.into()
    }
}
