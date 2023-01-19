use std::process::Command;

use anyhow::{bail, Result};
use clap::Args;

use crate::app::CommandExt as _;
use crate::testing::{config::IntegrationTestConfig, runner::*, state};

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
    pub fn exec(self) -> Result<()> {
        let (test_dir, config) = IntegrationTestConfig::load(&self.integration)?;

        let envs_dir = state::envs_dir(&self.integration);
        let runner = IntegrationTestRunner::new(self.integration)?;
        runner.ensure_network()?;

        let mut command = Command::new("cargo");
        command.current_dir(&test_dir);
        command.env(NETWORK_ENV_VAR, runner.network_name());
        command.args(["run", "--quiet", "--", "start"]);

        let environments = config.environments();
        let json = match environments.get(&self.environment) {
            Some(config) => serde_json::to_string(config)?,
            None => bail!("unknown environment: {}", self.environment),
        };
        command.arg(&json);

        if state::env_exists(&envs_dir, &self.environment) {
            bail!("environment is already up");
        }

        if let Some(env_vars) = config.env {
            command.envs(env_vars);
        }

        waiting!("Starting environment {}", &self.environment);
        command.check_run()?;

        state::save_env(&envs_dir, &self.environment, &json)?;
        Ok(())
    }
}
