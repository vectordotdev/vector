use anyhow::{bail, Result};

use crate::testing::{config::ComposeTestConfig, integration::ComposeTestT, state::EnvsDir};

pub fn exec<T: ComposeTestT>(
    integration: &str,
    environment: &Option<String>,
    build_all: bool,
    retries: u8,
    args: &[String],
) -> Result<()> {
    let (_test_dir, config) = ComposeTestConfig::load(T::DIRECTORY, integration)?;
    let envs = config.environments();

    let active = EnvsDir::new(integration).active()?;

    match (&environment, &active) {
        (Some(environment), Some(active)) if environment != active => {
            bail!("Requested environment {environment:?} does not match active one {active:?}")
        }
        (Some(environment), _) => T::test(
            &T::generate(integration, environment, build_all, retries)?,
            args.to_owned(),
        ),
        (None, Some(active)) => T::test(
            &T::generate(integration, active, build_all, retries)?,
            args.to_owned(),
        ),
        (None, None) => {
            for env_name in envs.keys() {
                T::test(
                    &T::generate(integration, env_name, build_all, retries)?,
                    args.to_owned(),
                )?;
            }
            Ok(())
        }
    }
}
