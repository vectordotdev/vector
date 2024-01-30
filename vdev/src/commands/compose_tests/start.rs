use anyhow::Result;

use crate::testing::{config::ComposeTestConfig, integration::ComposeTestT};

pub(crate) fn exec<T: ComposeTestT>(
    integration: &str,
    environment: &Option<String>,
    build_all: bool,
) -> Result<()> {
    let environment = if let Some(environment) = environment {
        environment.clone()
    } else {
        let (_test_dir, config) = ComposeTestConfig::load(T::DIRECTORY, integration)?;
        let envs = config.environments();
        let env = envs.keys().next().expect("Integration has no environments");
        env.clone()
    };

    T::start(&T::generate(integration, environment, build_all, 0)?)
}
