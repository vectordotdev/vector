use anyhow::Result;
use clap::Args;
use std::path::PathBuf;
use std::process::Command;

use crate::app::Application;
use crate::testing::{config::IntegrationTestConfig, state};

/// Stop an environment
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The desired integration
    integration: String,

    /// The desired environment
    environment: String,
}

impl Cli {
    pub fn exec(&self, app: &Application) -> Result<()> {
        let test_dir: PathBuf = [&app.repo.path, "scripts", "integration", &self.integration]
            .iter()
            .collect();
        if !test_dir.is_dir() {
            app.abort(format!("Unknown integration: {}", self.integration));
        }

        let envs_dir = state::envs_dir(&app.platform.data_dir(), &self.integration);
        if !state::env_exists(&envs_dir, &self.environment) {
            app.abort("Environment is not up");
        }

        let config = IntegrationTestConfig::parse_integration(&app.repo.path, &self.integration)?;

        let mut command = Command::new("cargo");
        command.current_dir(test_dir);
        command.args([
            "run",
            "--quiet",
            "--",
            "stop",
            &state::read_env_config(&envs_dir, &self.environment)?,
        ]);

        if let Some(env_vars) = config.env {
            command.envs(env_vars);
        }

        let status = command.status()?;
        if !status.success() {
            app.exit(status.code().unwrap());
        }

        state::remove_env(&envs_dir, &self.environment)?;
        Ok(())
    }
}
