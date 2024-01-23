use anyhow::Result;
use clap::Args;

use crate::testing::{config::IntegrationTestConfig, integration::IntegrationTest};

/// Start an environment
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// The integration name
    integration: String,

    /// Whether to compile the test runner with all integration test features
    #[arg(short = 'a', long)]
    build_all: bool,

    /// The desired environment name to start. If omitted, the first environment name is used.
    environment: Option<String>,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let environment = if let Some(environment) = self.environment {
            environment
        } else {
            let (_test_dir, config) = IntegrationTestConfig::load(&self.integration)?;
            let envs = config.environments();
            let env = envs.keys().next().expect("Integration has no environments");
            env.clone()
        };
        IntegrationTest::new(self.integration, environment, self.build_all, 0)?.start()
    }
}
