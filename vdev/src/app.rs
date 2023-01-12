use std::{borrow::Cow, ffi::OsStr, path::Path, process::Command, time::Duration};

use anyhow::{anyhow, bail, Context as _, Result};
use indicatif::{ProgressBar, ProgressStyle};
use log::LevelFilter;
use once_cell::sync::OnceCell;

use crate::config::Config;

static VERBOSITY: OnceCell<LevelFilter> = OnceCell::new();
static CONFIG: OnceCell<Config> = OnceCell::new();
static PATH: OnceCell<String> = OnceCell::new();

pub fn verbosity() -> &'static LevelFilter {
    VERBOSITY.get().expect("verbosity is not initialized")
}

pub fn config() -> &'static Config {
    CONFIG.get().expect("config is not initialized")
}

pub fn path() -> &'static String {
    PATH.get().expect("path is not initialized")
}

/// Overlay some extra helper functions onto `std::process::Command`
pub trait CommandExt {
    fn with_path(program: &str) -> Self;
    fn capture_output(&mut self) -> Result<String>;
    fn run(&mut self) -> Result<()>;
    fn wait(&mut self, message: impl Into<Cow<'static, str>>) -> Result<()>;
}

impl CommandExt for Command {
    fn with_path(program: &str) -> Self {
        let mut command = Command::new(program);
        command.current_dir(path());
        command
    }

    fn capture_output(&mut self) -> Result<String> {
        Ok(String::from_utf8(self.output()?.stdout)?)
    }

    fn run(&mut self) -> Result<()> {
        let status = self.status()?;
        if status.success() {
            Ok(())
        } else {
            bail!(
                "command: {} {}\nfailed with exit code: {}",
                self.get_program().to_str().expect("Invalid program name"),
                self.get_args()
                    .map(|arg| arg.to_str().expect("Invalid command argument"))
                    .collect::<Vec<_>>()
                    .join(" "),
                status.code().unwrap()
            )
        }
    }

    fn wait(&mut self, message: impl Into<Cow<'static, str>>) -> Result<()> {
        let progress_bar = get_progress_bar()?;
        progress_bar.set_message(message);

        let result = self.output();
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
}

pub fn exec<T: AsRef<OsStr>>(command: &Path, args: impl IntoIterator<Item = T>) -> Result<()> {
    let mut command = Command::new(command);
    command.args(args);
    match command
        .spawn()
        .with_context(|| format!("Could not spawn {command:?}"))?
        .wait()
        .context("Could not wait for program exit")?
    {
        status if status.success() => Ok(()),
        status => Err(anyhow!("Command failed, exit code {status}")),
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

pub fn set_global_verbosity(verbosity: LevelFilter) {
    VERBOSITY.set(verbosity).expect("could not set verbosity");
}

pub fn set_global_config(config: Config) {
    CONFIG.set(config).expect("could not set config");
}

pub fn set_global_path(path: String) {
    PATH.set(path).expect("could not set path");
}
