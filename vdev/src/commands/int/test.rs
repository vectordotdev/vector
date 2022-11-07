use anyhow::Result;
use clap::Args;

use crate::app::Application;
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
    pub fn exec(&self, app: &Application) -> Result<()> {
        let test_dir = IntegrationTestConfig::locate_source(&app.repo.path, &self.integration)?;
        let toolchain_config = RustToolchainConfig::parse(&app.repo.path)?;
        let runner = IntegrationTestRunner::new(
            &app,
            &self.integration,
            &toolchain_config.channel,
        );

        let envs_dir = state::envs_dir(&app.platform.data_dir(), &self.integration);
        if !state::env_exists(&envs_dir, &self.environment) {
            app.abort("Environment is not up");
        }

        let config = IntegrationTestConfig::from_source(&test_dir)?;

        runner.test(&config, &self.args)
    }
}
