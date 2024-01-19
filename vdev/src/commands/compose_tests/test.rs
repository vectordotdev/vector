use anyhow::{bail, Result};

use crate::testing::{config::ComposeTestConfig, integration::IntegrationTest, state::EnvsDir};

pub fn exec(
    integration: &str,
    path: &str,
    environment: &Option<String>,
    build_all: bool,
    retries: u8,
    args: &[String],
) -> Result<()> {
    let (_test_dir, config) = ComposeTestConfig::load(path, integration)?;
    let envs = config.environments();

    let active = EnvsDir::new(integration).active()?;

    match (&environment, &active) {
        (Some(environment), Some(active)) if environment != active => {
            bail!("Requested environment {environment:?} does not match active one {active:?}")
        }
        (Some(environment), _) => {
            IntegrationTest::new(integration, path, environment, build_all, retries)?
                .test(args.to_owned())
        }
        (None, Some(active)) => {
            IntegrationTest::new(integration, path, active, build_all, retries)?
                .test(args.to_owned())
        }
        (None, None) => {
            for env_name in envs.keys() {
                IntegrationTest::new(integration, path, env_name, build_all, retries)?
                    .test(args.to_owned())?;
            }
            Ok(())
        }
    }
}
