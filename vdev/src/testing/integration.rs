use std::{path::Path, path::PathBuf, process::Command};

use anyhow::{bail, Context, Result};

use super::runner::{
    ContainerTestRunnerBase as _, IntegrationTestRunner, CONTAINER_TOOL, NETWORK_ENV_VAR,
};
use super::{config::Environment, config::IntegrationTestConfig, state};
use crate::app::CommandExt as _;

fn compose_command(
    test_dir: &Path,
    args: impl IntoIterator<Item = &'static str>,
) -> Result<Command> {
    let compose_path: PathBuf = [test_dir, Path::new("docker-compose.yml")].iter().collect();
    let compose_file = dunce::canonicalize(compose_path)
        .context("Could not canonicalize docker compose path")?
        .display()
        .to_string();

    let mut command = CONTAINER_TOOL.clone();
    command.push("-compose");
    let mut command = Command::new(command);
    command.args(["--file", &compose_file]);
    command.args(args);
    command.current_dir(test_dir);
    Ok(command)
}

fn apply_env_vars(command: &mut Command, config: &Environment, integration: &str) {
    if let Some(version) = config.get("version") {
        let version_env = format!("{}_VERSION", integration.to_uppercase());
        command.env(version_env, version);
    }
}

pub fn start(integration: &str, environment: &str) -> Result<()> {
    let (test_dir, config) = IntegrationTestConfig::load(integration)?;

    let envs_dir = state::EnvsDir::new(integration);
    let runner = IntegrationTestRunner::new(integration.to_owned())?;
    runner.ensure_network()?;

    let mut command = compose_command(&test_dir, ["up", "--detach"])?;
    command.env(NETWORK_ENV_VAR, runner.network_name());

    let environments = config.environments();
    let cmd_config = match environments.get(environment) {
        Some(config) => config,
        None => bail!("unknown environment: {}", environment),
    };

    if envs_dir.exists(environment) {
        bail!("environment is already up");
    }

    if let Some(env_vars) = config.env {
        command.envs(env_vars);
    }

    apply_env_vars(&mut command, cmd_config, integration);

    waiting!("Starting environment {environment}");
    command.run()?;

    envs_dir.save(environment, cmd_config)
}

pub fn stop(integration: &str, environment: &str, force: bool) -> Result<()> {
    let (test_dir, config) = IntegrationTestConfig::load(integration)?;
    let envs_dir = state::EnvsDir::new(integration);
    let runner = IntegrationTestRunner::new(integration.to_owned())?;

    let mut command = compose_command(&test_dir, ["down", "--timeout", "0"])?;
    command.env(NETWORK_ENV_VAR, runner.network_name());

    let cmd_config: Environment = if envs_dir.exists(environment) {
        envs_dir.read_config(environment)?
    } else if force {
        let environments = config.environments();
        if let Some(config) = environments.get(environment) {
            config.clone()
        } else {
            bail!("unknown environment: {environment}");
        }
    } else {
        bail!("environment is not up");
    };

    if let Some(env_vars) = config.env {
        command.envs(env_vars);
    }

    apply_env_vars(&mut command, &cmd_config, integration);

    waiting!("Stopping environment {environment}");
    command.run()?;

    envs_dir.remove(environment)?;
    if envs_dir.list_active()?.is_empty() {
        runner.stop()?;
    }

    Ok(())
}
