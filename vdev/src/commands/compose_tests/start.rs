use anyhow::Result;

use crate::testing::{
    config::ComposeTestConfig,
    integration::{ComposeTest, ComposeTestLocalConfig},
};

pub(crate) fn exec(
    local_config: ComposeTestLocalConfig,
    integration: &str,
    environment: Option<&String>,
    build_all: bool,
) -> Result<()> {
    let environment = if let Some(environment) = environment {
        environment.clone()
    } else {
        let (_test_dir, config) = ComposeTestConfig::load(local_config.directory, integration)?;
        let envs = config.environments();
        let env = envs.keys().next().expect("Integration has no environments");
        env.clone()
    };

    ComposeTest::generate(local_config, integration, environment, build_all, 0)?.start()
}
