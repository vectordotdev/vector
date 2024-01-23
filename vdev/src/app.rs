use std::ffi::{OsStr, OsString};
pub use std::process::Command;
use std::{
    borrow::Cow, env, io::Read, path::PathBuf, process::ExitStatus, process::Stdio, sync::OnceLock,
    time::Duration,
};

use anyhow::{bail, Context as _, Result};
use indicatif::{ProgressBar, ProgressStyle};
use log::LevelFilter;
use once_cell::sync::Lazy;

use crate::{config::Config, git, platform, util};

// Use the `bash` interpreter included as part of the standard `git` install for our default shell
// if nothing is specified in the environment.
#[cfg(windows)]
const DEFAULT_SHELL: &str = "C:\\Program Files\\Git\\bin\\bash.EXE";

// This default is not currently used on non-Windows, so this is just a placeholder for now.
#[cfg(not(windows))]
const DEFAULT_SHELL: &str = "/bin/sh";

// Extract the shell from the environment variable `$SHELL` and substitute the above default value
// if it isn't set.
pub static SHELL: Lazy<OsString> =
    Lazy::new(|| (env::var_os("SHELL").unwrap_or_else(|| DEFAULT_SHELL.into())));

static VERBOSITY: OnceLock<LevelFilter> = OnceLock::new();
static CONFIG: OnceLock<Config> = OnceLock::new();
static PATH: OnceLock<String> = OnceLock::new();

pub fn verbosity() -> &'static LevelFilter {
    VERBOSITY.get().expect("verbosity is not initialized")
}

pub fn config() -> &'static Config {
    CONFIG.get().expect("config is not initialized")
}

pub fn path() -> &'static String {
    PATH.get().expect("path is not initialized")
}

pub fn set_repo_dir() -> Result<()> {
    env::set_current_dir(path()).context("Could not change directory")
}

pub fn version() -> Result<String> {
    let mut version = util::get_version()?;

    let channel = util::get_channel();

    if channel == "release" {
        let head = util::git_head()?;
        if !head.status.success() {
            let error = String::from_utf8_lossy(&head.stderr);
            bail!("Error running `git describe`:\n{error}");
        }
        let tag = String::from_utf8_lossy(&head.stdout).trim().to_string();
        if tag != format!("v{version}") {
            bail!("On latest release channel and tag {tag:?} is different from Cargo.toml {version:?}. Aborting");
        }

    // extend version for custom builds if not already
    } else if channel == "custom" && !version.contains("custom") {
        let sha = git::get_git_sha()?;

        // use '.' instead of '-' or '_' to avoid issues with rpm and deb package naming
        // format requirements.
        version = format!("{version}.custom.{sha}");
    }

    Ok(version)
}

/// Overlay some extra helper functions onto `std::process::Command`
pub trait CommandExt {
    fn script(script: &str) -> Self;
    fn in_repo(&mut self) -> &mut Self;
    fn check_output(&mut self) -> Result<String>;
    fn check_run(&mut self) -> Result<()>;
    fn run(&mut self) -> Result<ExitStatus>;
    fn wait(&mut self, message: impl Into<Cow<'static, str>>) -> Result<()>;
    fn pre_exec(&self);
    fn features(&mut self, features: &[String]) -> &mut Self;
}

impl CommandExt for Command {
    /// Create a new command to execute the named script in the repository `scripts` directory.
    fn script(script: &str) -> Self {
        let path: PathBuf = [path(), "scripts", script].into_iter().collect();
        if cfg!(windows) {
            // On Windows, all scripts must be run through an explicit interpreter.
            let mut command = Command::new(&*SHELL);
            command.arg(path);
            command
        } else {
            // On all other systems, we can run scripts directly.
            Command::new(path)
        }
    }

    /// Set the command's working directory to the repository directory.
    fn in_repo(&mut self) -> &mut Self {
        self.current_dir(path())
    }

    /// Run the command and capture its output.
    fn check_output(&mut self) -> Result<String> {
        // Set up the command's stdout to be piped, so we can capture it
        self.pre_exec();
        self.stdout(Stdio::piped());

        // Spawn the process
        let mut child = self.spawn()?;

        // Read the output from child.stdout into a buffer
        let mut buffer = Vec::new();
        child.stdout.take().unwrap().read_to_end(&mut buffer)?;

        // Catch the exit code
        let status = child.wait()?;
        // There are commands that might fail with stdout, but we probably do not
        // want to capture
        // If the exit code is non-zero, return an error with the command, exit code, and stderr output
        if !status.success() {
            let stdout = String::from_utf8_lossy(&buffer);
            bail!(
                "Command: {:?}\nfailed with exit code: {}\n\noutput:\n{}",
                self,
                status.code().unwrap(),
                stdout
            );
        }

        // If the command exits successfully, return the output as a string
        Ok(String::from_utf8(buffer)?)
    }

    /// Run the command and catch its exit code.
    fn run(&mut self) -> Result<ExitStatus> {
        self.pre_exec();
        self.status().map_err(Into::into)
    }

    fn check_run(&mut self) -> Result<()> {
        let status = self.run()?;
        if status.success() {
            Ok(())
        } else {
            let exit = status.code().unwrap();
            bail!("command: {self:?}\n  failed with exit code: {exit}")
        }
    }

    /// Run the command, capture its output, and display a progress bar while it's
    /// executing. Intended to be used for long-running processes with little interaction.
    fn wait(&mut self, message: impl Into<Cow<'static, str>>) -> Result<()> {
        self.pre_exec();

        let progress_bar = get_progress_bar()?;
        progress_bar.set_message(message);

        let result = self.output();
        progress_bar.finish_and_clear();

        let Ok(output) = result else {
            bail!("could not run command")
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

    /// Print out a pre-execution debug message.
    fn pre_exec(&self) {
        debug!("Executing: {self:?}");
        if let Some(cwd) = self.get_current_dir() {
            debug!("  in working directory {cwd:?}");
        }
        for (key, value) in self.get_envs() {
            let key = key.to_string_lossy();
            if let Some(value) = value {
                debug!("  ${key}={:?}", value.to_string_lossy());
            } else {
                debug!("  unset ${key}");
            }
        }
    }

    fn features(&mut self, features: &[String]) -> &mut Self {
        self.arg("--no-default-features");
        self.arg("--features");
        if features.is_empty() {
            self.arg(platform::default_features());
        } else {
            self.arg(features.join(","));
        }
        self
    }
}

/// Short-cut wrapper to create a new command, feed in the args, set the working directory, and then
/// run it, checking the resulting exit code.
pub fn exec<T: AsRef<OsStr>>(
    program: &str,
    args: impl IntoIterator<Item = T>,
    in_repo: bool,
) -> Result<()> {
    let mut command = match program.strip_prefix("scripts/") {
        Some(script) => Command::script(script),
        None => Command::new(program),
    };
    command.args(args);
    if in_repo {
        command.in_repo();
    }
    command.check_run()
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
