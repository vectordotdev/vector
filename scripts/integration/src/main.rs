use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::Duration;

use anyhow::{bail, Result};
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
    Start { name: String, json: String },
    Stop { name: String, json: String },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Start { name, json } => start(&name, serde_json::from_str(&json)?),
        Commands::Stop { name, json } => stop(&name, serde_json::from_str(&json)?),
    }
}

fn start(name: &str, config: Value) -> Result<()> {
    let mut command = compose_command();
    command.args(["up", "-d"]);

    apply_env_vars(&mut command, &config, name);

    let status = command.status()?;
    if status.success() {
        thread::sleep(Duration::from_secs(20));
        return Ok(());
    } else {
        bail!("failed to execute: {}", render_command(&mut command));
    }
}

fn stop(name: &str, config: Value) -> Result<()> {
    let mut command = compose_command();
    command.args(["down", "-t", "0"]);

    apply_env_vars(&mut command, &config, name);

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

fn apply_env_vars(command: &mut Command, config: &Value, integration: &str) {
    let version_env = format!("{}_VERSION", integration.to_uppercase());
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
