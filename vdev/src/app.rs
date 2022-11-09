use anyhow::{bail, Result};
use indicatif::{ProgressBar, ProgressStyle};
use log::LevelFilter;
use once_cell::sync::OnceCell;
use std::time::Duration;
use std::{borrow::Cow, process::Command};

use crate::config::{Config, ConfigFile};

static VERBOSITY: OnceCell<LevelFilter> = OnceCell::new();
static CONFIG_FILE: OnceCell<ConfigFile> = OnceCell::new();
static CONFIG: OnceCell<Config> = OnceCell::new();
static PATH: OnceCell<String> = OnceCell::new();

pub fn verbosity() -> &'static LevelFilter {
    VERBOSITY.get().expect("verbosity is not initialized")
}

pub fn config_file() -> &'static ConfigFile {
    CONFIG_FILE.get().expect("config file is not initialized")
}

pub fn config() -> &'static Config {
    CONFIG.get().expect("config is not initialized")
}

pub fn path() -> &'static String {
    PATH.get().expect("path is not initialized")
}

pub fn construct_command(program: &str) -> Command {
    let mut command = Command::new(program);
    command.current_dir(path());

    command
}

pub fn capture_output(command: &mut Command) -> Result<String> {
    Ok(String::from_utf8(command.output()?.stdout)?)
}

pub fn run_command(command: &mut Command) -> Result<()> {
    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        bail!(
            "command: {}\nfailed with exit code: {}",
            render_command(command),
            status.code().unwrap()
        )
    }
}

pub fn wait_for_command(
    command: &mut Command,
    message: impl Into<Cow<'static, str>>,
) -> Result<()> {
    let progress_bar = get_progress_bar()?;
    progress_bar.set_message(message);

    let result = command.output();
    progress_bar.finish_and_clear();
    let output = match result {
        Ok(output) => output,
        Err(_) => bail!("could not run command"),
    };

    if output.status.success() {
        Ok(())
    } else {
        bail!(
            "{}\nfailed with exit code: {}",
            String::from_utf8(output.stdout)?,
            output.status.code().unwrap()
        )
    }
}

fn get_progress_bar() -> Result<ProgressBar> {
    let progress_bar = ProgressBar::new_spinner();
    progress_bar.enable_steady_tick(Duration::from_millis(125));
    progress_bar.set_style(
        ProgressStyle::with_template("{spinner} {msg:.magenta.bold}")?
            // https://github.com/sindresorhus/cli-spinners/blob/master/spinners.json
            .tick_strings(&["∙∙∙", "●∙∙", "∙●∙", "∙∙●", "∙∙∙"]),
    );

    Ok(progress_bar)
}

fn render_command(command: &mut Command) -> String {
    format!(
        "{} {}",
        command.get_program().to_str().unwrap(),
        Vec::from_iter(command.get_args().map(|arg| arg.to_str().unwrap())).join(" ")
    )
}

pub fn set_global_verbosity(verbosity: LevelFilter) {
    VERBOSITY.set(verbosity).unwrap()
}

pub fn set_global_config_file(config_file: ConfigFile) {
    CONFIG_FILE.set(config_file).unwrap()
}

pub fn set_global_config(config: Config) {
    CONFIG.set(config).unwrap()
}

pub fn set_global_path(path: String) {
    PATH.set(path).unwrap()
}
