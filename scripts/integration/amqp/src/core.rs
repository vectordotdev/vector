use anyhow::{bail, Result};
use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::Duration;

pub fn start(config: Value) -> Result<()> {
    let mut command = compose_command();
    command.args(["up", "-d"]);

    apply_env_vars(&mut command, &config);

    let status = command.status()?;
    if status.success() {
        thread::sleep(Duration::from_secs(20));
        return Ok(());
    } else {
        bail!("failed to execute: {}", render_command(&mut command));
    }
}

pub fn stop(config: Value) -> Result<()> {
    let mut command = compose_command();
    command.args(["down", "-t", "0"]);

    apply_env_vars(&mut command, &config);

    let status = command.status()?;
    if status.success() {
        return Ok(());
    } else {
        bail!("failed to execute: {}", render_command(&mut command));
    }
}

fn compose_command() -> Command {
    let path = PathBuf::from_iter(["data", "docker-compose.yml"].iter());
    let compose_file = match dunce::canonicalize(&path) {
        Ok(p) => p.display().to_string(),
        Err(_) => path.display().to_string(),
    };

    let mut command = Command::new("docker");
    command.args(["compose", "-f", &compose_file]);
    command
}

fn apply_env_vars(command: &mut Command, config: &Value) {
    if let Some(version) = config.get("version") {
        command.env("AMQP_VERSION", version.as_str().unwrap());
    }
}

fn render_command(command: &mut Command) -> String {
    format!(
        "{} {}",
        command.get_program().to_str().unwrap(),
        Vec::from_iter(command.get_args().map(|arg| arg.to_str().unwrap())).join(" ")
    )
}
