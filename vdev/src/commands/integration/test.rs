use anyhow::{bail, Result};
use clap::Args;
use std::collections::BTreeMap;

use crate::testing::{config::IntegrationTestConfig, integration, runner::*, state};

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
        let (_test_dir, config) = IntegrationTestConfig::load(&self.integration)?;
        let runner = IntegrationTestRunner::new(self.integration.clone())?;
        let envs_dir = state::EnvsDir::new(&self.integration);
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
            if !envs_dir.exists(environment) {
                bail!("environment {environment} is not up");
            }

            return runner.test(&env_vars, &args);
        }

        runner.ensure_network()?;

        let active_envs = envs_dir.list_active()?;
        for env_name in envs.keys() {
            if !(active_envs.is_empty() || active_envs.contains(env_name)) {
                continue;
            }

            let env_active = envs_dir.exists(env_name);
            if !env_active {
                integration::start(&self.integration, env_name)?;
            }

            runner.test(&env_vars, &args)?;

            if !env_active {
                integration::stop(&self.integration, env_name, false)?;
            }
        }

        if active_envs.is_empty() {
            runner.stop()?;
        }

        Ok(())
    }
}
