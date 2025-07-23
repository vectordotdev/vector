use anyhow::{bail, Result};

use crate::testing::{
    config::ComposeTestConfig,
    integration::{ComposeTest, ComposeTestT},
    state::EnvsDir,
};

pub fn exec<T: ComposeTestT>(
    integration: &str,
    environment: Option<&String>,
    build_all: bool,
    retries: u8,
    args: &[String],
) -> Result<()> {
    let (_test_dir, config) = ComposeTestConfig::load(T::DIRECTORY, integration)?;
    let envs = config.environments();

    let active = EnvsDir::new(integration).active()?;

    match (environment, &active) {
        (Some(environment), Some(active)) if environment != active => {
            bail!("Requested environment {environment:?} does not match active one {active:?}")
        }
        (Some(environment), _) => {
            ComposeTest::<T>::generate(integration, environment, build_all, retries)?
                .test(args.to_owned())
        }
        (None, Some(active)) => {
            ComposeTest::<T>::generate(integration, active, build_all, retries)?
                .test(args.to_owned())
        }
        (None, None) => {
            for env_name in envs.keys() {
                ComposeTest::<T>::generate(integration, env_name, build_all, retries)?
                    .test(args.to_owned())?;
            }
            Ok(())
        }
    }
}
