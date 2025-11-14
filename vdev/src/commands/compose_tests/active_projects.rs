use anyhow::{Context, Result};
use std::{collections::HashSet, process::Command};

use crate::testing::{config::ComposeTestConfig, docker::CONTAINER_TOOL};

/// Query Docker Compose for active projects
pub(super) fn load_active_projects() -> Result<HashSet<String>> {
    let output = Command::new(CONTAINER_TOOL.clone())
        .args(["compose", "ls", "--format", "json"])
        .output()
        .with_context(|| "Failed to list compose projects")?;

    if !output.status.success() {
        return Ok(HashSet::new());
    }

    let projects: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout)
        .with_context(|| "Failed to parse docker compose ls output")?;

    Ok(projects
        .iter()
        .filter_map(|project| {
            project
                .get("Name")
                .and_then(|n| n.as_str())
                .map(String::from)
        })
        .collect())
}

/// Find the active environment for a given integration by matching Docker Compose project names
pub(super) fn find_active_environment(
    active_projects: &HashSet<String>,
    prefix: &str,
    config: &ComposeTestConfig,
) -> Option<String> {
    for project_name in active_projects {
        if let Some(sanitized_env_name) = project_name.strip_prefix(prefix) {
            // The project name has dots replaced with hyphens, so we need to check
            // all environments to find a match after applying the same sanitization
            for env_name in config.environments().keys() {
                if env_name.replace('.', "-") == sanitized_env_name {
                    return Some(env_name.clone());
                }
            }
        }
    }
    None
}

/// Find the active environment for a given integration by querying Docker Compose
pub(super) fn find_active_environment_for_integration(
    directory: &str,
    integration: &str,
    config: &ComposeTestConfig,
) -> Result<Option<String>> {
    let active_projects = load_active_projects()?;
    let prefix = format!("vector-{directory}-{integration}-");
    Ok(find_active_environment(&active_projects, &prefix, config))
}
