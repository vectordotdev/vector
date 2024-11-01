use anyhow::Result;

use crate::testing::{config::ComposeTestConfig, config::Environment, state};

pub fn exec(integration: &Option<String>, path: &str) -> Result<()> {
    match integration {
        None => {
            let entries = ComposeTestConfig::collect_all(path)?;
            let width = entries
                .keys()
                .fold(16, |width, entry| width.max(entry.len()));
            println!("{:width$}  Environment Name(s)", "Integration Name");
            println!("{:width$}  -------------------", "----------------");
            for (integration, config) in entries {
                let envs_dir = state::EnvsDir::new(&integration);
                let active_env = envs_dir.active()?;
                let environments = config
                    .environments()
                    .keys()
                    .map(|environment| format(&active_env, environment))
                    .collect::<Vec<_>>()
                    .join("  ");
                println!("{integration:width$}  {environments}");
            }
        }
        Some(integration) => {
            let (_test_dir, config) = ComposeTestConfig::load(path, integration)?;
            let envs_dir = state::EnvsDir::new(integration);
            let active_env = envs_dir.active()?;

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
                println!("  {}", format(&active_env, environment));
            }
        }
    }
    Ok(())
}

fn format(active_env: &Option<String>, environment: &str) -> String {
    match active_env {
        Some(active) if active == environment => format!("{environment} (active)"),
        _ => environment.into(),
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
