use std::{path::Path, path::PathBuf, process::Command};

use anyhow::{Context, Result};
use serde_json::Value;

crate::cli_subcommands! {
    "Manage integration test environments"
    mod show,
    mod start,
    mod stop,
    mod test,
}

pub(self) fn compose_command(
    test_dir: &Path,
    args: impl IntoIterator<Item = &'static str>,
) -> Result<Command> {
    let compose_path: PathBuf = [test_dir, Path::new("docker-compose.yml")].iter().collect();
    let compose_file = dunce::canonicalize(compose_path)
        .context("Could not canonicalize docker compose path")?
        .display()
        .to_string();

    let mut command = Command::new("docker-compose");
    command.args(["--file", &compose_file]);
    command.args(args);
    command.current_dir(test_dir);
    Ok(command)
}

pub(self) fn apply_env_vars(command: &mut Command, config: &Value, integration: &str) {
    let version_env = format!("{}_VERSION", integration.to_uppercase());
    if let Some(number) = config.get("version") {
        command.env(version_env, number.as_str().unwrap());
    }
}
