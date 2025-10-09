use anyhow::{Context, Result};
use std::process::Command;

use crate::testing::{
    config::ComposeTestConfig,
    docker::CONTAINER_TOOL,
    integration::{ComposeTest, ComposeTestLocalConfig},
};

pub(crate) fn exec(
    local_config: ComposeTestLocalConfig,
    test_name: &str,
    all_features: bool,
) -> Result<()> {
    // Find which environment is running by checking docker compose ls
    let active_environment = find_active_environment(local_config, test_name)?;

    if let Some(environment) = active_environment {
        ComposeTest::generate(local_config, test_name, environment, all_features, 0)?.stop()
    } else {
        println!("No environment for {test_name} is active.");
        Ok(())
    }
}

fn find_active_environment(
    local_config: ComposeTestLocalConfig,
    test_name: &str,
) -> Result<Option<String>> {
    let (_test_dir, config) = ComposeTestConfig::load(local_config.directory, test_name)?;
    let prefix = format!("vector-{}-{}-", local_config.directory, test_name);

    let output = Command::new(CONTAINER_TOOL.clone())
        .args(["compose", "ls", "--format", "json"])
        .output()
        .with_context(|| "Failed to list compose projects")?;

    if !output.status.success() {
        return Ok(None);
    }

    let projects: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout)
        .with_context(|| "Failed to parse docker compose ls output")?;

    // Find a project that matches our naming pattern
    for project in projects {
        if let Some(name) = project.get("Name").and_then(|n| n.as_str()) {
            if let Some(sanitized_env_name) = name.strip_prefix(&prefix) {
                // The project name has dots replaced with hyphens, so we need to check
                // all environments to find a match after applying the same sanitization
                for env_name in config.environments().keys() {
                    if env_name.replace('.', "-") == sanitized_env_name {
                        return Ok(Some(env_name.to_string()));
                    }
                }
            }
        }
    }

    Ok(None)
}
