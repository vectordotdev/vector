use anyhow::{bail, Result};
use clap::Args;
use std::process::Command;

use crate::app;
use crate::platform;
use crate::testing::{
    config::{IntegrationTestConfig, RustToolchainConfig},
    runner::*,
    state,
};

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
    pub fn exec(&self) -> Result<()> {
        let test_dir = IntegrationTestConfig::locate_source(app::path(), &self.integration)?;
        let envs_dir = state::envs_dir(&platform::data_dir(), &self.integration);
        let config = IntegrationTestConfig::from_source(&test_dir)?;
        let toolchain_config = RustToolchainConfig::parse(app::path())?;
        let runner =
            IntegrationTestRunner::new(self.integration.to_string(), toolchain_config.channel);

        let mut command = Command::new("cargo");
        command.current_dir(&test_dir);
        command.env(NETWORK_ENV_VAR, runner.network_name());
        command.args(["run", "--quiet", "--", "stop"]);

        if state::env_exists(&envs_dir, &self.environment) {
            command.arg(state::read_env_config(&envs_dir, &self.environment)?);
        } else if self.force {
            let environments = config.environments();
            if let Some(config) = environments.get(&self.environment) {
                command.arg(serde_json::to_string(config)?);
            } else {
                bail!("unknown environment: {}", self.environment);
            }
        } else {
            bail!("environment is not up");
        }

        if let Some(env_vars) = config.env {
            command.envs(env_vars);
        }

        waiting!("Stopping environment {}", &self.environment);
        app::run_command(&mut command)?;

        state::remove_env(&envs_dir, &self.environment)?;
        if state::active_envs(&envs_dir)?.is_empty() {
            runner.stop()?;
        }

        Ok(())
    }
}
