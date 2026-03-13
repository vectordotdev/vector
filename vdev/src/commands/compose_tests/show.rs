use anyhow::Result;
use std::collections::HashSet;

use crate::{testing::config::ComposeTestConfig, utils::environment::Environment};

use super::active_projects::{find_active_environment, load_active_projects};

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
    active_projects: HashSet<String>,
}

impl Show {
    fn new(path: &str) -> Result<Self> {
        Ok(Self {
            path: path.to_string(),
            active_projects: load_active_projects()?,
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
            let active_env = find_active_environment(&self.active_projects, &prefix, &config);
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
        let active_env = find_active_environment(&self.active_projects, &prefix, &config);

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
}

fn print_env(prefix: &str, environment: &Environment) {
    if environment.is_empty() {
        println!("{prefix} N/A");
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
