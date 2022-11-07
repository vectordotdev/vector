use anyhow::Result;
use clap::Args;
use std::process::Command;

use crate::app::Application;
use crate::testing::{
    config::{IntegrationTestConfig, RustToolchainConfig},
    runner::{IntegrationTestRunner, NETWORK_ENV_VAR},
    state,
};

/// Start an environment
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
        let test_dir = IntegrationTestConfig::locate_source(&app.repo.path, &self.integration)?;

        let envs_dir = state::envs_dir(&app.platform.data_dir(), &self.integration);
        let config = IntegrationTestConfig::from_source(&test_dir)?;
        let toolchain_config = RustToolchainConfig::parse(&app.repo.path)?;
        let runner = IntegrationTestRunner::new(
            &app,
            &self.integration,
            &toolchain_config.channel,
        );
        runner.ensure_network()?;

        let mut command = Command::new("cargo");
        command.current_dir(&test_dir);
        command.env(NETWORK_ENV_VAR, runner.network_name());
        command.args(["run", "--quiet", "--", "start"]);

        let mut json = "".to_string();
        let environments = config.environments();
        if let Some(config) = environments.get(&self.environment) {
            json = serde_json::to_string(config)?;
            command.arg(&json);
        } else {
            app.abort(format!("Unknown environment: {}", self.environment));
        }

        if state::env_exists(&envs_dir, &self.environment) {
            app.abort("Environment is already up");
        }

        if let Some(env_vars) = config.env {
            command.envs(env_vars);
        }

        let status = command.status()?;
        if !status.success() {
            app.exit(status.code().unwrap());
        }

        state::save_env(&envs_dir, &self.environment, &json)?;
        Ok(())
    }
}
