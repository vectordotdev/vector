use anyhow::Result;

use crate::testing::{
    config::ComposeTestConfig,
    integration::{ComposeTest, ComposeTestLocalConfig},
};

use super::active_projects::find_active_environment_for_integration;

pub(crate) fn exec(local_config: ComposeTestLocalConfig, test_name: &str) -> Result<()> {
    let (_test_dir, config) = ComposeTestConfig::load(local_config.directory, test_name)?;
    let active_environment =
        find_active_environment_for_integration(local_config.directory, test_name, &config)?;

    if let Some(environment) = active_environment {
        // all_features doesn't matter for stop since we always use the same container name now
        ComposeTest::generate(local_config, test_name, environment, false, 0)?.stop()
    } else {
        println!("No environment for {test_name} is active.");
        Ok(())
    }
}
