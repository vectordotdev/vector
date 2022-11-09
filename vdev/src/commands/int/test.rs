use anyhow::{bail, Result};
use clap::Args;
use std::process::Command;

use crate::app;
use crate::platform;
use crate::testing::{
    config::{IntegrationTestConfig, RustToolchainConfig},
    runner::{IntegrationTestRunner, NETWORK_ENV_VAR},
    state,
};

/// Stop an environment
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The desired integration
    integration: String,

    /// The desired environment
    environment: Option<String>,

    /// Extra test command arguments
    args: Option<Vec<String>>,
}

impl Cli {
    pub fn exec(&self) -> Result<()> {
        let test_dir = IntegrationTestConfig::locate_source(app::path(), &self.integration)?;
        let toolchain_config = RustToolchainConfig::parse(app::path())?;
        let runner = IntegrationTestRunner::new(&self.integration, &toolchain_config.channel);
        let envs_dir = state::envs_dir(&platform::data_dir(), &self.integration);
        let config = IntegrationTestConfig::from_source(&test_dir)?;

        if let Some(environment) = &self.environment {
            if !state::env_exists(&envs_dir, environment) {
                bail!("environment {environment} is not up");
            }

            return runner.test(&config, &self.args);
        }

        runner.ensure_network()?;

        let active_envs = state::active_envs(&envs_dir)?;
        for (env_name, env_config) in config.environments().iter() {
            if !(active_envs.is_empty() || active_envs.contains(env_name)) {
                continue;
            }

            let env_active = state::env_exists(&envs_dir, &env_name);
            if !env_active {
                let mut command = Command::new("cargo");
                command.current_dir(&test_dir);
                command.env(NETWORK_ENV_VAR, runner.network_name());
                command.args(["run", "--quiet", "--", "start"]);

                let json = serde_json::to_string(env_config)?;
                command.arg(&json);

                if let Some(env_vars) = &config.env {
                    command.envs(env_vars);
                }

                waiting!("Starting environment {}", env_name);
                app::run_command(&mut command)?;

                state::save_env(&envs_dir, env_name, &json)?;
            }

            runner.test(&config, &self.args)?;

            if !env_active {
                let mut command = Command::new("cargo");
                command.current_dir(&test_dir);
                command.env(NETWORK_ENV_VAR, runner.network_name());
                command.args([
                    "run",
                    "--quiet",
                    "--",
                    "stop",
                    &state::read_env_config(&envs_dir, env_name)?,
                ]);

                if let Some(env_vars) = &config.env {
                    command.envs(env_vars);
                }

                waiting!("Stopping environment {}", env_name);
                app::run_command(&mut command)?;

                state::remove_env(&envs_dir, env_name)?;
            }
        }

        if active_envs.is_empty() {
            runner.stop()?;
        }

        Ok(())
    }
}
