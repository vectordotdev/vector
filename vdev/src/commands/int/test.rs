use anyhow::{Result, bail};
use clap::Args;

use crate::app;
use crate::platform;
use crate::testing::{
    config::{IntegrationTestConfig, RustToolchainConfig},
    runner::IntegrationTestRunner,
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

    /// Extra test command arguments
    args: Option<Vec<String>>,
}

impl Cli {
    pub fn exec(&self) -> Result<()> {
        let test_dir = IntegrationTestConfig::locate_source(app::path(), &self.integration)?;
        let toolchain_config = RustToolchainConfig::parse(app::path())?;
        let runner = IntegrationTestRunner::new(
            &self.integration,
            &toolchain_config.channel,
        );

        let envs_dir = state::envs_dir(&platform::data_dir(), &self.integration);
        if !state::env_exists(&envs_dir, &self.environment) {
            bail!("environment is not up");
        }

        let config = IntegrationTestConfig::from_source(&test_dir)?;

        runner.test(&config, &self.args)
    }
}
