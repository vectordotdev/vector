use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::Duration;

use anyhow::bail;
pub use anyhow::Result;
use clap::{Parser, Subcommand};
use serde_json::Value;

#[derive(Parser, Debug)]
#[command(disable_help_subcommand = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Start { json: String },
    Stop { json: String },
}

pub fn docker_main(name: &str) -> Result<()> {
    let version_env = format!("{}_VERSION", name.to_uppercase());

    let cli = Cli::parse();

    match &cli.command {
        Commands::Start { json } => docker_start(serde_json::from_str(&json)?, &version_env),
        Commands::Stop { json } => docker_stop(serde_json::from_str(&json)?, &version_env),
    }
}

fn docker_start(config: Value, version_env: &str) -> Result<()> {
    let mut command = compose_command();
    command.args(["up", "-d"]);

    apply_env_vars(&mut command, &config, version_env);

    let status = command.status()?;
    if status.success() {
        thread::sleep(Duration::from_secs(20));
        return Ok(());
    } else {
        bail!("failed to execute: {}", render_command(&mut command));
    }
}

fn docker_stop(config: Value, version_env: &str) -> Result<()> {
    let mut command = compose_command();
    command.args(["down", "-t", "0"]);

    apply_env_vars(&mut command, &config, version_env);

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

fn apply_env_vars(command: &mut Command, config: &Value, version_env: &str) {
    if let Some(number) = config.get("version") {
        command.env(version_env, number.as_str().unwrap());
    }
}

fn render_command(command: &mut Command) -> String {
    format!(
        "{} {}",
        command.get_program().to_str().unwrap(),
        Vec::from_iter(command.get_args().map(|arg| arg.to_str().unwrap())).join(" ")
    )
}
