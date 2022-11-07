use anyhow::Result;
use clap::Args;
use std::path::PathBuf;

use crate::app::Application;
use crate::testing::{config::IntegrationTestConfig, state};

/// Show information about integrations
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The desired integration
    integration: Option<String>,
}

impl Cli {
    pub fn exec(&self, app: &Application) -> Result<()> {
        if self.integration.is_none() {
            let mut entries = vec![];
            let root_dir: PathBuf = [&app.repo.path, "scripts", "integration"].iter().collect();
            for entry in root_dir.read_dir()? {
                if let Ok(entry) = entry {
                    if entry.path().is_dir() {
                        entries.push(entry.file_name().into_string().unwrap());
                    }
                }
            }
            entries.sort();

            for integration in entries.iter() {
                app.display(integration);
            }

            return Ok(());
        }

        let test_dir = IntegrationTestConfig::locate_source(
            &app.repo.path,
            &self.integration.as_ref().unwrap(),
        )?;
        let config = IntegrationTestConfig::from_source(&test_dir)?;
        let envs_dir = state::envs_dir(
            &app.platform.data_dir(),
            &self.integration.as_ref().unwrap(),
        );
        let active_envs = state::active_envs(&envs_dir)?;

        app.display(format!(
            "Tests triggered: {}",
            config.triggered(app.repo.git.changed_files()?)?
        ));
        app.display(format!("Test args: {}", config.args.join(" ")));

        app.display("Environments:");
        for environment in config.environments().keys() {
            if active_envs.contains(environment) {
                app.display(format!("  {} (active)", environment));
            } else {
                app.display(format!("  {}", environment));
            }
        }

        Ok(())
    }
}
