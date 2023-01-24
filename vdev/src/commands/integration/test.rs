use anyhow::{bail, Result};
use clap::Args;
use std::collections::BTreeMap;
use std::process::Command;

use crate::app::CommandExt as _;
use crate::testing::{config::IntegrationTestConfig, runner::*, state};

/// Execute tests
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
    pub fn exec(self) -> Result<()> {
        let (test_dir, config) = IntegrationTestConfig::load(&self.integration)?;
        let runner = IntegrationTestRunner::new(self.integration.clone())?;
        let envs_dir = state::envs_dir(&self.integration);
        let envs = config.environments();

        let env_vars: BTreeMap<_, _> = config
            .env
            .clone()
            .map_or(BTreeMap::default(), |map| map.into_iter().collect());

        let mut args: Vec<_> = config.args.into_iter().collect();
        if let Some(configured_args) = self.args {
            args.extend(configured_args);
        }

        if let Some(environment) = &self.environment {
            if !state::env_exists(&envs_dir, environment) {
                bail!("environment {environment} is not up");
            }

            return runner.test(&env_vars, &args);
        }

        runner.ensure_network()?;

        let active_envs = state::active_envs(&envs_dir)?;
        for (env_name, env_config) in envs {
            if !(active_envs.is_empty() || active_envs.contains(&env_name)) {
                continue;
            }

            let env_active = state::env_exists(&envs_dir, &env_name);
            if !env_active {
                let mut command = Command::new("cargo");
                command.current_dir(&test_dir);
                command.env(NETWORK_ENV_VAR, runner.network_name());
                command.args(["run", "--quiet", "--", "start"]);

                let json = serde_json::to_string(&env_config)?;
                command.arg(&json);

                if let Some(env_vars) = &config.env {
                    command.envs(env_vars);
                }

                waiting!("Starting environment {}", env_name);
                command.check_run()?;

                state::save_env(&envs_dir, &env_name, &json)?;
            }

            runner.test(&env_vars, &args)?;

            if !env_active {
                let mut command = Command::new("cargo");
                command.current_dir(&test_dir);
                command.env(NETWORK_ENV_VAR, runner.network_name());
                command.args([
                    "run",
                    "--quiet",
                    "--",
                    "stop",
                    &state::read_env_config(&envs_dir, &env_name)?,
                ]);

                if let Some(env_vars) = &config.env {
                    command.envs(env_vars);
                }

                waiting!("Stopping environment {}", env_name);
                command.check_run()?;

                state::remove_env(&envs_dir, &env_name)?;
            }
        }

        if active_envs.is_empty() {
            runner.stop()?;
        }

        Ok(())
    }
}
