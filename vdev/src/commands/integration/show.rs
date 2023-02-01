use anyhow::Result;
use clap::Args;

use crate::testing::{config::Environment, config::IntegrationTestConfig, state};

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
                    let active_env = envs_dir.active()?;
                    let environments = config
                        .environments()
                        .keys()
                        .map(|environment| format(&active_env, environment))
                        .collect::<Vec<_>>()
                        .join("  ");
                    println!("{integration:width$}  {environments}");
                }
            }
            Some(integration) => {
                let (_test_dir, config) = IntegrationTestConfig::load(&integration)?;
                let envs_dir = state::EnvsDir::new(&integration);
                let active_env = envs_dir.active()?;

                println!("Test args: {}", config.args.join(" "));

                println!("Environment:");
                print_env(&config.env);
                println!("Runner environment:");
                print_env(&config.runner_env);

                println!("Environments:");
                for environment in config.environments().keys() {
                    println!("  {}", format(&active_env, environment));
                }
            }
        }
        Ok(())
    }
}

fn format(active_env: &Option<String>, environment: &str) -> String {
    match active_env {
        Some(active) if active == environment => format!("{environment} (active)"),
        _ => environment.into(),
    }
}

fn print_env(environment: &Environment) {
    if environment.is_empty() {
        println!("  N/A");
    } else {
        for (key, value) in environment {
            match value {
                Some(value) => println!("  {key}={value:?}"),
                None => println!("  {key} (passthrough)"),
            }
        }
    }
}
