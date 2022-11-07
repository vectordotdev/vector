use anyhow::{bail, Result};
use atty::Stream;
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;

use crate::app;
use crate::testing::config::IntegrationTestConfig;

pub const NETWORK_ENV_VAR: &str = "VECTOR_NETWORK";
const MOUNT_PATH: &str = "/home/vector";
const TARGET_PATH: &str = "/home/target";
const VOLUME_TARGET: &str = "vector_target";
const VOLUME_CARGO_GIT: &str = "vector_cargo_git";
const VOLUME_CARGO_REGISTRY: &str = "vector_cargo_registry";

pub enum RunnerState {
    Running,
    Restarting,
    Created,
    Exited,
    Paused,
    Dead,
    Missing,
    Unknown,
}

pub struct IntegrationTestRunner<'a> {
    integration: &'a String,
    rust_version: &'a String,
}

impl<'a> IntegrationTestRunner<'a> {
    pub fn new(
        integration: &'a String,
        rust_version: &'a String,
    ) -> IntegrationTestRunner {
        IntegrationTestRunner {
            integration,
            rust_version,
        }
    }

    pub fn network_name(&self) -> String {
        format!("vector-integration-tests-{}", self.integration)
    }

    pub fn container_name(&self) -> String {
        format!("vector-test-runner-{}-{}", self.integration, self.rust_version)
    }

    pub fn image_name(&self) -> String {
        format!("{}:latest", self.container_name())
    }

    pub fn ensure_network(&self) -> Result<()> {
        let mut command = Command::new("docker");
        command.args(["network", "ls", "--format", "{{.Name}}"]);

        if String::from_utf8(command.output()?.stdout)?
            .lines()
            .any(|network| network == &self.network_name())
        {
            return Ok(());
        }

        let mut command = Command::new("docker");
        command.args(["network", "create", &self.network_name()]);

        let status = command.status()?;
        if status.success() {
            Ok(())
        } else {
            bail!("failed with exit code: {}", status.code().unwrap());
        }
    }

    pub fn test(&self, config: &IntegrationTestConfig, args: &Option<Vec<String>>) -> Result<()> {
        match self.state()? {
            RunnerState::Running | RunnerState::Restarting => (),
            RunnerState::Created | RunnerState::Exited => self.start()?,
            RunnerState::Paused => self.unpause()?,
            RunnerState::Dead | RunnerState::Unknown => {
                self.remove()?;
                self.create()?;
                self.start()?;
            }
            RunnerState::Missing => {
                self.build()?;
                self.ensure_volumes()?;
                self.create()?;
                self.start()?;
            }
        }

        let mut command = Command::new("docker");
        command.arg("exec");
        if atty::is(Stream::Stdout) {
            command.arg("--tty");
        }

        command.args(["--env", &format!("CARGO_BUILD_TARGET_DIR={TARGET_PATH}")]);
        if let Some(env_vars) = &config.env {
            for (key, value) in env_vars.iter() {
                command.env(key, value);
                command.args(["--env", &format!("{key}={value}")]);
            }
        }

        command.args([
            &self.container_name(),
            "cargo",
            "nextest",
            "run",
            "--no-fail-fast",
            "--no-default-features",
        ]);
        command.args(&config.args);

        if let Some(args) = args {
            command.args(args);
        }

        let status = command.status()?;
        if status.success() {
            Ok(())
        } else {
            bail!("failed with exit code: {}", status.code().unwrap());
        }
    }

    fn state(&self) -> Result<RunnerState> {
        let mut command = Command::new("docker");
        command.args(["ps", "-a", "--format", "{{.Names}} {{.State}}"]);

        for line in String::from_utf8(command.output()?.stdout)?.lines() {
            if let Some((name, state)) = line.split_once(" ") {
                if name != self.container_name() {
                    continue;
                } else if state == "created" {
                    return Ok(RunnerState::Created);
                } else if state == "dead" {
                    return Ok(RunnerState::Dead);
                } else if state == "exited" {
                    return Ok(RunnerState::Exited);
                } else if state == "paused" {
                    return Ok(RunnerState::Paused);
                } else if state == "restarting" {
                    return Ok(RunnerState::Restarting);
                } else if state == "running" {
                    return Ok(RunnerState::Running);
                } else {
                    return Ok(RunnerState::Unknown);
                }
            }
        }

        Ok(RunnerState::Missing)
    }

    fn ensure_volumes(&self) -> Result<()> {
        let mut command = Command::new("docker");
        command.args(["volume", "ls", "--format", "{{.Name}}"]);

        let mut volumes = HashSet::new();
        volumes.insert(VOLUME_TARGET);
        volumes.insert(VOLUME_CARGO_GIT);
        volumes.insert(VOLUME_CARGO_REGISTRY);
        for volume in String::from_utf8(command.output()?.stdout)?.lines() {
            volumes.take(volume);
        }

        for volume in &volumes {
            let mut command = Command::new("docker");
            command.args(["volume", "create", volume]);

            let status = command.status()?;
            if !status.success() {
                bail!("failed with exit code: {}", status.code().unwrap());
            }
        }

        Ok(())
    }

    fn build(&self) -> Result<()> {
        let dockerfile: PathBuf = [app::path(), "scripts", "integration", "Dockerfile"]
            .iter()
            .collect();
        let mut command = Command::new("docker");
        command.current_dir(app::path());
        command.arg("build");
        if atty::is(Stream::Stdout) {
            command.args(["--progress", "tty"]);
        }
        command.args([
            "--pull",
            "--tag",
            &self.image_name(),
            "--file",
            dockerfile.to_str().unwrap(),
            "--build-arg",
            &format!("RUST_VERSION={}", self.rust_version),
            ".",
        ]);

        let status = command.status()?;
        if status.success() {
            Ok(())
        } else {
            bail!("failed with exit code: {}", status.code().unwrap());
        }
    }

    fn start(&self) -> Result<()> {
        let mut command = Command::new("docker");
        command.args(["start", &self.container_name()]);

        let status = command.status()?;
        if status.success() {
            Ok(())
        } else {
            bail!("failed with exit code: {}", status.code().unwrap());
        }
    }

    fn stop(&self) -> Result<()> {
        let mut command = Command::new("docker");
        command.args(["stop", "--time", "0", &self.container_name()]);

        let status = command.status()?;
        if status.success() {
            Ok(())
        } else {
            bail!("failed with exit code: {}", status.code().unwrap());
        }
    }

    fn remove(&self) -> Result<()> {
        let mut command = Command::new("docker");
        command.args(["rm", &self.container_name()]);

        let status = command.status()?;
        if status.success() {
            Ok(())
        } else {
            bail!("failed with exit code: {}", status.code().unwrap());
        }
    }

    fn unpause(&self) -> Result<()> {
        let mut command = Command::new("docker");
        command.args(["unpause", &self.container_name()]);

        let status = command.status()?;
        if status.success() {
            Ok(())
        } else {
            bail!("failed with exit code: {}", status.code().unwrap());
        }
    }

    fn create(&self) -> Result<()> {
        let mut command = Command::new("docker");
        command.arg("create");
        if atty::is(Stream::Stdout) {
            command.arg("--tty");
        }
        command.args([
            "--name",
            &self.container_name(),
            "--network",
            &self.network_name(),
            "--workdir",
            MOUNT_PATH,
            "--volume",
            &format!("{}:{}", app::path(), MOUNT_PATH),
            "--volume",
            &format!("{VOLUME_TARGET}:{TARGET_PATH}"),
            "--volume",
            &format!("{VOLUME_CARGO_GIT}:/usr/local/cargo/git"),
            "--volume",
            &format!("{VOLUME_CARGO_REGISTRY}:/usr/local/cargo/registry"),
            &self.image_name(),
            "/bin/sleep",
            "infinity",
        ]);

        let status = command.status()?;
        if status.success() {
            Ok(())
        } else {
            bail!("failed with exit code: {}", status.code().unwrap());
        }
    }
}
