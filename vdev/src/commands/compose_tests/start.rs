use anyhow::Result;

use crate::testing::{config::ComposeTestConfig, integration::IntegrationTest};

pub(crate) fn exec(integration: &str, path: &str, environment: &Option<String>) -> Result<()> {
    let environment = if let Some(environment) = environment {
        environment.clone()
    } else {
        let (_test_dir, config) = ComposeTestConfig::load(path, integration)?;
        let envs = config.environments();
        let env = envs.keys().next().expect("Integration has no environments");
        env.clone()
    };
    IntegrationTest::new(integration, path, environment, false, 0)?.start()
}
