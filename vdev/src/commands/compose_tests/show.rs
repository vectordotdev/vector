use anyhow::{Context, Result};
use std::{collections::HashMap, process::Command};

use crate::{
    environment::Environment,
    testing::{config::ComposeTestConfig, docker::CONTAINER_TOOL},
};

pub fn exec(integration: Option<&String>, path: &str) -> Result<()> {
    let show = Show::new(path)?;
    match integration {
        None => show.show_all(),
        Some(integration) => show.show_one(integration),
    }
}

/// Print only the environment names for a single integration, one per line.
pub fn exec_environments_only(integration: &str, path: &str) -> Result<()> {
    let (_test_dir, config) = ComposeTestConfig::load(path, integration)?;
    for environment in config.environments().keys() {
        println!("{environment}");
    }
    Ok(())
}

struct Show {
    path: String,
    active_projects: HashMap<String, bool>,
}

impl Show {
    fn new(path: &str) -> Result<Self> {
        let output = Command::new(CONTAINER_TOOL.clone())
            .args(["compose", "ls", "--format", "json"])
            .output()
            .with_context(|| "Failed to list compose projects")?;

        let active_projects = if output.status.success() {
            let projects: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout)
                .with_context(|| "Failed to parse docker compose ls output")?;

            let mut map = HashMap::new();
            for project in projects {
                if let Some(name) = project.get("Name").and_then(|n| n.as_str()) {
                    map.insert(name.to_string(), true);
                }
            }
            map
        } else {
            HashMap::new()
        };

        Ok(Self {
            path: path.to_string(),
            active_projects,
        })
    }

    fn show_all(&self) -> Result<()> {
        let entries = ComposeTestConfig::collect_all(&self.path)?;

        let width = entries
            .keys()
            .fold(16, |width, entry| width.max(entry.len()));
        println!("{:width$}  Environment Name(s)", "Integration Name");
        println!("{:width$}  -------------------", "----------------");
        for (integration, config) in entries {
            let prefix = format!("vector-{}-{integration}-", self.path);
            let active_env = self.find_active_environment(&prefix, &config);
            let environments = config
                .environments()
                .keys()
                .map(|environment| format_env(active_env.as_ref(), environment))
                .collect::<Vec<_>>()
                .join("  ");
            println!("{integration:width$}  {environments}");
        }
        Ok(())
    }

    fn show_one(&self, integration: &str) -> Result<()> {
        let (_test_dir, config) = ComposeTestConfig::load(&self.path, integration)?;
        let prefix = format!("vector-{}-{integration}-", self.path);
        let active_env = self.find_active_environment(&prefix, &config);

        if let Some(args) = &config.args {
            println!("Test args: {}", args.join(" "));
        } else {
            println!("Test args: N/A");
        }

        if config.features.is_empty() {
            println!("Features: N/A");
        } else {
            println!("Features: {}", config.features.join(","));
        }

        println!(
            "Test filter: {}",
            config.test_filter.as_deref().unwrap_or("N/A")
        );

        println!("Environment:");
        print_env("  ", &config.env);
        println!("Runner:");
        println!("  Environment:");
        print_env("    ", &config.runner.env);
        println!("  Volumes:");
        if config.runner.volumes.is_empty() {
            println!("    N/A");
        } else {
            for (target, mount) in &config.runner.volumes {
                println!("    {target} => {mount}");
            }
        }
        println!(
            "  Needs docker socket: {}",
            config.runner.needs_docker_socket
        );

        println!("Environments:");
        for environment in config.environments().keys() {
            println!("  {}", format_env(active_env.as_ref(), environment));
        }

        Ok(())
    }

    fn find_active_environment(
        &self,
        prefix: &str,
        config: &ComposeTestConfig,
    ) -> Option<String> {
        for project_name in self.active_projects.keys() {
            if let Some(sanitized_env_name) = project_name.strip_prefix(prefix) {
                // The project name has dots replaced with hyphens, so we need to check
                // all environments to find a match after applying the same sanitization
                for env_name in config.environments().keys() {
                    if env_name.replace('.', "-") == sanitized_env_name {
                        return Some(env_name.to_string());
                    }
                }
            }
        }
        None
    }
}

fn print_env(prefix: &str, environment: &Environment) {
    if environment.is_empty() {
        println!("{prefix}N/A");
    } else {
        for (key, value) in environment {
            match value {
                Some(value) => println!("{prefix}{key}={value:?}"),
                None => println!("{prefix}{key} (passthrough)"),
            }
        }
    }
}

fn format_env(active_env: Option<&String>, environment: &str) -> String {
    match active_env {
        Some(active) if active == environment => format!("{environment} (active)"),
        _ => environment.into(),
    }
}
