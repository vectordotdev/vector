use anyhow::{bail, Result};
use clap::Args;
use serde_json::Value;

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

        let envs_dir = state::EnvsDir::new(&self.integration);
        let runner = IntegrationTestRunner::new(self.integration.clone())?;
        runner.ensure_network()?;

        let mut command = super::compose_command(&test_dir, ["up", "--detach"])?;
        command.env(NETWORK_ENV_VAR, runner.network_name());

        let environments = config.environments();
        let json = match environments.get(&self.environment) {
            Some(config) => serde_json::to_string(config)?,
            None => bail!("unknown environment: {}", self.environment),
        };
        let cmd_config: Value = serde_json::from_str(&json)?;

        if envs_dir.exists(&self.environment) {
            bail!("environment is already up");
        }

        if let Some(env_vars) = config.env {
            command.envs(env_vars);
        }

        super::apply_env_vars(&mut command, &cmd_config, &self.integration);

        waiting!("Starting environment {}", &self.environment);
        command.run()?;

        envs_dir.save(&self.environment, &json)?;
        Ok(())
    }
}
