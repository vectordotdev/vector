use anyhow::Result;

use crate::testing::{
    config::ComposeTestConfig,
    integration::{ComposeTest, ComposeTestLocalConfig},
};

pub(crate) fn exec(
    local_config: ComposeTestLocalConfig,
    integration: &str,
    environment: Option<&String>,
) -> Result<()> {
    let environment = if let Some(environment) = environment {
        environment.clone()
    } else {
        let (_test_dir, config) = ComposeTestConfig::load(local_config.directory, integration)?;
        let envs = config.environments();
        trace!("Available environments: {envs:#?}");
        let env = envs.keys().next().expect("Integration has no environments");
        env.clone()
    };
    debug!("Selected environment: {environment:#?}");
    ComposeTest::generate(local_config, integration, environment, 0)?.start()
}
