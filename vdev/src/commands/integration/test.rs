use anyhow::{bail, Result};
use clap::Args;

use crate::testing::{config::IntegrationTestConfig, integration::IntegrationTest, state::EnvsDir};

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
        let (_test_dir, config) = IntegrationTestConfig::load(&self.integration)?;
        let envs = config.environments();

        let active = EnvsDir::new(&self.integration).active()?;
        match (self.environment, active) {
            (Some(environment), Some(active)) if environment != active => {
                bail!("Requested environment {environment:?} does not match active one {active:?}")
            }
            (Some(environment), _) => {
                IntegrationTest::new(self.integration, environment)?.test(self.args)
            }
            (None, Some(active)) => IntegrationTest::new(self.integration, active)?.test(self.args),
            (None, None) => {
                for env_name in envs.keys() {
                    IntegrationTest::new(&self.integration, env_name)?.test(self.args.clone())?;
                }
                Ok(())
            }
        }
    }
}
