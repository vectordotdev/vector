use anyhow::Result;
use clap::Args;

use crate::testing::runner::{ContainerTestRunner, IntegrationTestRunner};
use crate::testing::{config::IntegrationTestConfig, integration::IntegrationTest, state::EnvsDir};

/// Execute integration tests
///
/// If an environment is named, a single test is run. If the environment was not previously started,
/// it is started before the test is run and stopped afterwards.
///
/// If no environment is named, but some have been started already, only those environments are run.
///
/// Otherwise, all environments are started, the test run, and then stopped.
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The desired integration
    integration: String,

    /// The desired environment (optional)
    environment: Option<String>,

    /// Extra test command arguments
    args: Vec<String>,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let (_test_dir, config) = IntegrationTestConfig::load(&self.integration)?;
        let envs = config.environments();

        let env_vars = config.env.unwrap_or_default();

        let mut args = config.args;
        args.extend(self.args);

        if let Some(environment) = &self.environment {
            IntegrationTest::new(&self.integration, environment)?.test(&env_vars, &args)
        } else {
            let runner = IntegrationTestRunner::new(self.integration.clone())?;
            runner.ensure_network()?;

            let active_envs = EnvsDir::new(&self.integration).list_active()?;
            for env_name in envs.keys() {
                if !(active_envs.is_empty() || active_envs.contains(env_name)) {
                    continue;
                }

                IntegrationTest::new(&self.integration, env_name)?.test(&env_vars, &args)?;
            }

            if active_envs.is_empty() {
                runner.stop()?;
            }

            Ok(())
        }
    }
}
