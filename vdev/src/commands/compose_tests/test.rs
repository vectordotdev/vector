use std::{iter::once, process::Command};

use anyhow::{Context, Result, bail};

use crate::testing::{
    config::ComposeTestConfig,
    docker::CONTAINER_TOOL,
    integration::{ComposeTest, ComposeTestLocalConfig},
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

    let active = find_active_environment(local_config, integration, &config)?;
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
        ComposeTest::generate(local_config, integration, environment, build_all, retries)?
            .test(args.to_owned())?;
    }
    Ok(())
}

fn find_active_environment(
    local_config: ComposeTestLocalConfig,
    integration: &str,
    config: &ComposeTestConfig,
) -> Result<Option<String>> {
    let prefix = format!("vector-{}-{}-", local_config.directory, integration);

    let output = Command::new(CONTAINER_TOOL.clone())
        .args(["compose", "ls", "--format", "json"])
        .output()
        .with_context(|| "Failed to list compose projects")?;

    if !output.status.success() {
        return Ok(None);
    }

    let projects: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout)
        .with_context(|| "Failed to parse docker compose ls output")?;

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
