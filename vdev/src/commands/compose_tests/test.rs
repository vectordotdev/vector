use anyhow::{bail, Result};

use crate::testing::{
    config::ComposeTestConfig,
    integration::{ComposeTest, ComposeTestLocalConfig},
    state::EnvsDir,
};

pub fn exec(
    local_config: ComposeTestLocalConfig,
    integration: &str,
    environment: Option<&String>,
    build_all: bool,
    retries: u8,
    args: &[String],
) -> Result<()> {
    let (_test_dir, config) = ComposeTestConfig::load(local_config.directory, integration)?;
    let envs = config.environments();

    let active = EnvsDir::new(integration).active()?;

    match (environment, &active) {
        (Some(environment), Some(active)) if environment != active => {
            bail!("Requested environment {environment:?} does not match active one {active:?}")
        }
        (Some(environment), _) => {
            ComposeTest::generate(local_config, integration, environment, build_all, retries)?
                .test(args.to_owned())
        }
        (None, Some(active)) => {
            ComposeTest::generate(local_config, integration, active, build_all, retries)?
                .test(args.to_owned())
        }
        (None, None) => {
            for env_name in envs.keys() {
                ComposeTest::generate(local_config, integration, env_name, build_all, retries)?
                    .test(args.to_owned())?;
            }
            Ok(())
        }
    }
}
