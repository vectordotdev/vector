use anyhow::{bail, Result};
use clap::Args;

use crate::testing::integration::{self, IntegrationTest, OldIntegrationTest};
use crate::testing::{config::IntegrationTestConfig, state::EnvsDir};

/// Execute integration tests
///
/// If an environment is named, it is used to run the test. If the environment was not previously started,
/// it is started before the test is run and stopped afterwards.
///
/// If no environment is named, but one has been started already, that environment is used for the test.
///
/// Otherwise, all environments are started, the test run, and then stopped, one by one.
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
        // Temporary hack to run old-style integration tests
        if self.environment.is_none() && integration::old_exists(&self.integration)? {
            let integration = OldIntegrationTest::new(&self.integration);
            integration.build()?;
            return integration.test();
        }

        let (_test_dir, config) = IntegrationTestConfig::load(&self.integration)?;
        let envs = config.environments();

        let env_vars = config.env.unwrap_or_default();

        let mut args = config.args;
        args.extend(self.args);

        let active = EnvsDir::new(&self.integration).active()?;
        match (self.environment, active) {
            (Some(environment), Some(active)) if environment != active => {
                bail!("Requested environment {environment:?} does not match active one {active:?}")
            }
            (Some(environment), _) => {
                IntegrationTest::new(self.integration, environment)?.test(&env_vars, &args)
            }
            (None, Some(active)) => {
                IntegrationTest::new(self.integration, active)?.test(&env_vars, &args)
            }
            (None, None) => {
                for env_name in envs.keys() {
                    IntegrationTest::new(&self.integration, env_name)?.test(&env_vars, &args)?;
                }
                Ok(())
            }
        }
    }
}
