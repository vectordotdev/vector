use std::iter::once;

use anyhow::{Result, bail};

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
    args: Vec<String>,
) -> Result<()> {
    let (_test_dir, config) = ComposeTestConfig::load(local_config.directory, integration)?;
    let envs = config.environments();

    let active = EnvsDir::new(integration).active()?;
    debug!("Active environment: {environment:#?}");

    let environments: Box<dyn Iterator<Item = &String>> = match (environment, &active) {
        (Some(environment), Some(active)) if environment != active => {
            bail!("Requested environment {environment:?} does not match active one {active:?}")
        }
        (Some(environment), _) => Box::new(once(environment)),
        (None, Some(active)) => Box::new(once(active)),
        (None, None) => Box::new(envs.keys()),
    };

    for environment in environments {
        ComposeTest::generate(local_config, integration, environment, build_all, retries)?
            .test(args.clone())?;
    }
    Ok(())
}
