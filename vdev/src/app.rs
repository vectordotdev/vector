use std::ffi::{OsStr, OsString};
use std::io::{PipeReader, pipe};
use std::{
    borrow::Cow, env, io::Read, path::PathBuf, process::Command, process::ExitStatus,
    process::Stdio, sync::LazyLock, sync::OnceLock, time::Duration,
};

use anyhow::{Context as _, Result, bail};
use indicatif::{ProgressBar, ProgressStyle};
use itertools::Itertools;
use log::LevelFilter;

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
pub static SHELL: LazyLock<OsString> =
    LazyLock::new(|| (env::var_os("SHELL").unwrap_or_else(|| DEFAULT_SHELL.into())));

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
            bail!(
                "On latest release channel and tag {tag:?} is different from Cargo.toml {version:?}. Aborting"
            );
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

pub struct VDevCommand {
    pub inner: Command,
}

impl VDevCommand {
    #[must_use]
    pub fn new<S: AsRef<OsStr>>(program: S) -> Self {
        Self {
            inner: Command::new(program.as_ref()),
        }
    }

    /// Create a new command to execute the named script in the repository `scripts` directory.
    fn script(script: &str) -> Self {
        let path: PathBuf = [path(), "scripts", script].into_iter().collect();
        if cfg!(windows) {
            // On Windows, all scripts must be run through an explicit interpreter.
            Self::new(&*SHELL).arg(path)
        } else {
            // On all other systems, we can run scripts directly.
            Self::new(path)
        }
    }
}

impl From<Command> for VDevCommand {
    fn from(command: Command) -> Self {
        Self { inner: command }
    }
}

impl VDevCommand {
    #[must_use]
    pub fn arg<S: AsRef<OsStr>>(mut self, arg: S) -> Self {
        self.inner.arg(arg);
        self
    }

    #[must_use]
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.inner.args(args);
        self
    }

    #[must_use]
    pub fn features(mut self, features: &[String]) -> Self {
        self = self.arg("--no-default-features").arg("--features");
        if features.is_empty() {
            self = self.arg(platform::default_features());
        } else {
            self = self.arg(features.join(","));
        }
        self
    }

    #[must_use]
    pub fn in_repo(mut self) -> Self {
        self.inner.current_dir(path());
        self
    }

    #[must_use]
    pub fn env<K, V>(mut self, key: K, val: V) -> Self
    where
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.inner.env(key, val);
        self
    }

    #[must_use]
    pub fn envs<I, K, V>(mut self, vars: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.inner.envs(vars);
        self
    }

    /// Run the command and capture its output.
    pub fn check_output(self) -> Result<String> {
        self.check_output_inner().map(|(_, output)| output)
    }

    /// Set up the command's stdout/stderr to be piped to the reader
    fn setup_output(&mut self) -> Result<PipeReader> {
        let (reader, writer) = pipe()?;
        let writer_clone = writer.try_clone()?;

        self.inner.stdout(Stdio::from(writer));
        self.inner.stderr(Stdio::from(writer_clone));
        Ok(reader)
    }

    /// Run the command and capture its output.
    fn check_output_inner(mut self) -> Result<(ExitStatus, String)> {
        let error_info = format!(
            "\"{}\" {}",
            self.inner.get_program().to_string_lossy(),
            self.inner
                .get_args()
                .map(|arg| format!("\"{}\"", arg.to_string_lossy()))
                .join(" ")
        );

        self.pre_exec();

        let mut reader = self.setup_output()?;

        // Spawn the process
        let mut child = self
            .inner
            .spawn()
            .with_context(|| format!("Failed to spawn process {error_info}"))?;

        // Catch the exit code
        let status = child.wait()?;
        // There are commands that might fail with stdout, but we probably do not
        // want to capture
        // If the exit code is non-zero, return an error with the command, exit code, and full output
        drop(self.inner); // Drop inner to prevent deadlock when reading

        let mut buffer = Vec::new();
        reader.read_to_end(&mut buffer).unwrap();
        let output = String::from_utf8_lossy(&buffer);

        if !status.success() {
            bail!(
                "Command: {error_info}\nfailed with exit code: {}\n\noutput:\n{output}",
                status.code().unwrap(),
            );
        }

        // If the command exits successfully, return the output as a string
        Ok((status, output.into_owned()))
    }

    /// Run the command and catch its exit code.
    pub fn run(self) -> Result<ExitStatus> {
        self.pre_exec();
        self.check_output_inner().map(|(status, _)| status)
    }

    pub fn check_run(self) -> Result<()> {
        self.run().map(|_| ())
    }

    /// Run the command, capture its output, and display a progress bar while it's
    /// executing. Intended to be used for long-running processes with little interaction.
    pub fn wait(&mut self, message: impl Into<Cow<'static, str>>) -> Result<()> {
        let error_info = format!(
            "\"{}\" {}",
            self.inner.get_program().to_string_lossy(),
            self.inner
                .get_args()
                .map(|arg| format!("\"{}\"", arg.to_string_lossy()))
                .join(" ")
        );

        self.pre_exec();

        let mut reader = self.setup_output()?;

        let progress_bar = get_progress_bar()?;
        progress_bar.set_message(message);

        // Spawn the process
        let child = self
            .inner
            .spawn()
            .with_context(|| format!("Failed to spawn process {error_info}"));

        if child.is_err() {
            progress_bar.finish_and_clear();
        }
        let status = child?.wait();
        if status.is_err() {
            progress_bar.finish_and_clear();
        }
        let status = status?;

        if !status.success() {
            let mut buffer = Vec::new();
            reader.read_to_end(&mut buffer).unwrap();
            let output = String::from_utf8_lossy(&buffer);

            bail!(
                "Command: {error_info}\nfailed with exit code: {}\n\noutput:\n{output}",
                status.code().unwrap(),
            );
        }
        Ok(())
    }

    /// Print out a pre-execution debug message.
    fn pre_exec(&self) {
        if let Some(cwd) = self.inner.get_current_dir() {
            debug!("  in working directory {cwd:?}");
        }
        for (key, value) in self.inner.get_envs() {
            let key = key.to_string_lossy();
            if let Some(value) = value {
                debug!("  ${key}={:?}", value.to_string_lossy());
            } else {
                debug!("  unset ${key}");
            }
        }
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
        Some(script) => VDevCommand::script(script),
        None => VDevCommand::new(program),
    }
    .args(args);
    if in_repo {
        command = command.in_repo();
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
