use anyhow::{bail, Result};
use clap::Args;
use serde_json::Value;

use crate::app::CommandExt as _;
use crate::testing::{config::IntegrationTestConfig, runner::*, state};

/// Stop an environment
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The desired integration
    integration: String,

    /// The desired environment
    environment: String,

    /// Use the currently defined configuration if the environment is not up
    #[arg(short, long)]
    force: bool,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let (test_dir, config) = IntegrationTestConfig::load(&self.integration)?;
        let envs_dir = state::EnvsDir::new(&self.integration);
        let runner = IntegrationTestRunner::new(self.integration.clone())?;

        let mut command = super::compose_command(&test_dir, ["down", "--timeout", "0"])?;
        command.env(NETWORK_ENV_VAR, runner.network_name());

        let json = if envs_dir.exists(&self.environment) {
            envs_dir.read_config(&self.environment)?
        } else if self.force {
            let environments = config.environments();
            if let Some(config) = environments.get(&self.environment) {
                serde_json::to_string(config)?
            } else {
                bail!("unknown environment: {}", self.environment);
            }
        } else {
            bail!("environment is not up");
        };
        let cmd_config: Value = serde_json::from_str(&json)?;

        if let Some(env_vars) = config.env {
            command.envs(env_vars);
        }

        super::apply_env_vars(&mut command, &cmd_config, &self.integration);

        waiting!("Stopping environment {}", &self.environment);
        command.run()?;

        envs_dir.remove(&self.environment)?;
        if envs_dir.list_active()?.is_empty() {
            runner.stop()?;
        }

        Ok(())
    }
}
