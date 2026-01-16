use std::iter::once;

use anyhow::{Result, bail};

use crate::testing::{
    config::ComposeTestConfig,
    integration::{ComposeTest, ComposeTestLocalConfig},
};

use super::active_projects::find_active_environment_for_integration;

pub fn exec(
    local_config: ComposeTestLocalConfig,
    integration: &str,
    environment: Option<&String>,
    retries: u8,
    args: &[String],
) -> Result<()> {
    let (_test_dir, config) = ComposeTestConfig::load(local_config.directory, integration)?;
    let envs = config.environments();

    let active =
        find_active_environment_for_integration(local_config.directory, integration, &config)?;
    debug!("Active environment: {active:#?}");

    let environments: Box<dyn Iterator<Item = &String>> = match (environment, &active) {
        (Some(environment), Some(active)) if environment != active => {
            bail!("Requested environment {environment:?} does not match active one {active:?}")
        }
        (Some(environment), _) => Box::new(once(environment)),
        (None, Some(active)) => Box::new(once(active)),
        (None, None) => Box::new(envs.keys()),
    };

    for environment in environments {
        ComposeTest::generate(local_config, integration, environment, retries)?
            .test(args.to_owned())?;
    }
    Ok(())
}
