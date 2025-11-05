use anyhow::Result;

use crate::testing::{
    config::ComposeTestConfig,
    integration::{ComposeTest, ComposeTestLocalConfig},
};

/// Start an integration test environment
/// Integration tests don't build the image during start - they build lazily during test
pub(crate) fn exec_integration(integration: &str, environment: Option<&String>) -> Result<()> {
    let environment = select_environment(
        ComposeTestLocalConfig::integration(),
        integration,
        environment,
    )?;
    debug!("Selected environment: {environment:#?}");
    ComposeTest::generate(
        ComposeTestLocalConfig::integration(),
        integration,
        environment,
        0,
    )?
    .start(false) // Integration tests never build during start
}

/// Start an E2E test environment
/// E2E tests build the image during start because Vector runs as a service in compose
/// Builds with test-specific features for faster builds. Use `vdev e2e build` to pre-build
/// a shared image with all E2E features, then pass `no_build=true` to skip the build.
pub(crate) fn exec_e2e(test: &str, environment: Option<&String>, no_build: bool) -> Result<()> {
    let environment = select_environment(ComposeTestLocalConfig::e2e(), test, environment)?;
    debug!("Selected environment: {environment:#?}");
    ComposeTest::generate(ComposeTestLocalConfig::e2e(), test, environment, 0)?.start(no_build)
}

fn select_environment(
    local_config: ComposeTestLocalConfig,
    test_name: &str,
    environment: Option<&String>,
) -> Result<String> {
    if let Some(environment) = environment {
        Ok(environment.clone())
    } else {
        let (_test_dir, config) = ComposeTestConfig::load(local_config.directory, test_name)?;
        let envs = config.environments();
        trace!("Available environments: {envs:#?}");
        let env = envs.keys().next().expect("Test has no environments");
        Ok(env.clone())
    }
}
