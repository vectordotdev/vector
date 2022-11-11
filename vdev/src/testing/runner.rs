use anyhow::Result;
use atty::Stream;
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::process::Command;

use crate::app;

pub const NETWORK_ENV_VAR: &str = "VECTOR_NETWORK";
const MOUNT_PATH: &str = "/home/vector";
const TARGET_PATH: &str = "/home/target";
const VOLUME_TARGET: &str = "vector_target";
const VOLUME_CARGO_GIT: &str = "vector_cargo_git";
const VOLUME_CARGO_REGISTRY: &str = "vector_cargo_registry";
const TEST_COMMAND: &[&str] = &[
    "cargo",
    "nextest",
    "run",
    "--no-fail-fast",
    "--no-default-features",
];

enum RunnerState {
    Running,
    Restarting,
    Created,
    Exited,
    Paused,
    Dead,
    Missing,
    Unknown,
}

pub fn get_agent_test_runner(container: bool, rust_version: String) -> Box<dyn TestRunner> {
    if container {
        Box::new(DockerTestRunner::new(rust_version))
    } else {
        Box::new(LocalTestRunner::new())
    }
}

pub trait TestRunner {
    fn test(&self, env_vars: &BTreeMap<String, String>, args: &Vec<String>) -> Result<()>;
}

pub trait ContainerTestRunnerBase: TestRunner {
    fn container_name(&self) -> String;

    fn image_name(&self) -> String;

    fn network_name(&self) -> String {
        "host".to_string()
    }

    fn stop(&self) -> Result<()> {
        let mut command = Command::new("docker");
        command.args(["stop", "--time", "0", &self.container_name()]);

        app::wait_for_command(
            &mut command,
            format!("Stopping container {}", self.container_name()),
        )
    }
}

trait ContainerTestRunner: ContainerTestRunnerBase {
    fn get_rust_version(&self) -> &String;

    fn state(&self) -> Result<RunnerState> {
        let mut command = Command::new("docker");
        command.args(["ps", "-a", "--format", "{{.Names}} {{.State}}"]);

        for line in app::capture_output(&mut command)?.lines() {
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

    fn verify_state(&self) -> Result<()> {
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

        Ok(())
    }

    fn ensure_volumes(&self) -> Result<()> {
        let mut command = Command::new("docker");
        command.args(["volume", "ls", "--format", "{{.Name}}"]);

        let mut volumes = HashSet::new();
        volumes.insert(VOLUME_TARGET);
        volumes.insert(VOLUME_CARGO_GIT);
        volumes.insert(VOLUME_CARGO_REGISTRY);
        for volume in app::capture_output(&mut command)?.lines() {
            volumes.take(volume);
        }

        for volume in &volumes {
            let mut command = Command::new("docker");
            command.args(["volume", "create", volume]);

            app::wait_for_command(&mut command, format!("Creating volume {volume}"))?;
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
            &format!("RUST_VERSION={}", self.get_rust_version()),
            ".",
        ]);

        waiting!("Building image {}", self.image_name());
        app::run_command(&mut command)
    }

    fn start(&self) -> Result<()> {
        let mut command = Command::new("docker");
        command.args(["start", &self.container_name()]);

        app::wait_for_command(
            &mut command,
            format!("Starting container {}", self.container_name()),
        )
    }

    fn remove(&self) -> Result<()> {
        let mut command = Command::new("docker");
        command.args(["rm", &self.container_name()]);

        app::wait_for_command(
            &mut command,
            format!("Removing container {}", self.container_name()),
        )
    }

    fn unpause(&self) -> Result<()> {
        let mut command = Command::new("docker");
        command.args(["unpause", &self.container_name()]);

        app::wait_for_command(
            &mut command,
            format!("Unpausing container {}", self.container_name()),
        )
    }

    fn create(&self) -> Result<()> {
        let mut command = Command::new("docker");
        command.arg("create");
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

        app::wait_for_command(
            &mut command,
            format!("Creating container {}", self.container_name()),
        )
    }
}

pub struct IntegrationTestRunner {
    integration: String,
    rust_version: String,
}

impl IntegrationTestRunner {
    pub fn new(integration: String, rust_version: String) -> Self {
        Self {
            integration,
            rust_version,
        }
    }

    pub fn ensure_network(&self) -> Result<()> {
        let mut command = Command::new("docker");
        command.args(["network", "ls", "--format", "{{.Name}}"]);

        if app::capture_output(&mut command)?
            .lines()
            .any(|network| network == &self.network_name())
        {
            return Ok(());
        }

        let mut command = Command::new("docker");
        command.args(["network", "create", &self.network_name()]);

        app::wait_for_command(&mut command, "Creating network")
    }
}

impl TestRunner for IntegrationTestRunner {
    fn test(&self, env_vars: &BTreeMap<String, String>, args: &Vec<String>) -> Result<()> {
        self.verify_state()?;

        let mut command = Command::new("docker");
        command.arg("exec");
        if atty::is(Stream::Stdout) {
            command.arg("--tty");
        }

        command.args(["--env", &format!("CARGO_BUILD_TARGET_DIR={TARGET_PATH}")]);
        for (key, value) in env_vars {
            command.env(key, value);
            command.args(["--env", &key]);
        }

        command.arg(&self.container_name());
        command.args(TEST_COMMAND);
        command.args(args);

        app::run_command(&mut command)
    }
}

impl ContainerTestRunnerBase for IntegrationTestRunner {
    fn network_name(&self) -> String {
        format!("vector-integration-tests-{}", self.integration)
    }

    fn container_name(&self) -> String {
        format!(
            "vector-test-runner-{}-{}",
            self.integration, self.rust_version
        )
    }

    fn image_name(&self) -> String {
        format!("{}:latest", self.container_name())
    }
}

impl ContainerTestRunner for IntegrationTestRunner {
    fn get_rust_version(&self) -> &String {
        &self.rust_version
    }
}

pub struct DockerTestRunner {
    rust_version: String,
}

impl DockerTestRunner {
    pub fn new(rust_version: String) -> Self {
        Self { rust_version }
    }
}

impl TestRunner for DockerTestRunner {
    fn test(&self, env_vars: &BTreeMap<String, String>, args: &Vec<String>) -> Result<()> {
        self.verify_state()?;

        let mut command = Command::new("docker");
        command.arg("exec");
        if atty::is(Stream::Stdout) {
            command.arg("--tty");
        }

        command.args(["--env", &format!("CARGO_BUILD_TARGET_DIR={TARGET_PATH}")]);
        for (key, value) in env_vars {
            command.env(key, value);
            command.args(["--env", &key]);
        }

        command.arg(&self.container_name());
        command.args(TEST_COMMAND);
        command.args(args);

        app::run_command(&mut command)
    }
}

impl ContainerTestRunnerBase for DockerTestRunner {
    fn container_name(&self) -> String {
        format!("vector-test-runner-{}", self.rust_version)
    }

    fn image_name(&self) -> String {
        // The upstream container we publish artifacts to on a successful master build.
        "docker.io/timberio/vector-dev:sha-3eadc96742a33754a5859203b58249f6a806972a".to_string()
    }
}

impl ContainerTestRunner for DockerTestRunner {
    fn get_rust_version(&self) -> &String {
        &self.rust_version
    }
}

pub struct LocalTestRunner {}

impl LocalTestRunner {
    pub fn new() -> Self {
        Self {}
    }
}

impl TestRunner for LocalTestRunner {
    fn test(&self, env_vars: &BTreeMap<String, String>, args: &Vec<String>) -> Result<()> {
        let mut command = app::construct_command(TEST_COMMAND[0]);
        command.args(&TEST_COMMAND[1..]);
        command.args(args);

        for (key, value) in env_vars {
            command.env(key, value);
        }

        app::run_command(&mut command)
    }
}
