use anyhow::Result;
use std::{collections::HashSet, env, process::Command};

use super::config::{IntegrationRunnerConfig, RustToolchainConfig};
use super::integration::ComposeTestKind;
use crate::testing::test_runner_dockerfile;
use crate::utils::IS_A_TTY;
use crate::{
    app::{self, CommandExt as _},
    testing::{
        build::prepare_build_command,
        docker::{DOCKER_SOCKET, docker_command},
    },
    utils::environment::{Environment, append_environment_variables},
};

const MOUNT_PATH: &str = "/home/vector";
const TARGET_PATH: &str = "/home/target";
const VOLUME_TARGET: &str = "vector_target";
const VOLUME_CARGO_GIT: &str = "vector_cargo_git";
const VOLUME_CARGO_REGISTRY: &str = "vector_cargo_registry";
const RUNNER_HOSTNAME: &str = "runner";
const TEST_COMMAND: &[&str] = &[
    "cargo",
    "nextest",
    "run",
    "--no-fail-fast",
    "--no-default-features",
];
// The upstream container we publish artifacts to on a successful master build.
const UPSTREAM_IMAGE: &str = "docker.io/timberio/vector-dev:latest";

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

pub fn get_agent_test_runner(container: bool) -> Result<Box<dyn TestRunner>> {
    if container {
        Ok(Box::new(DockerTestRunner))
    } else {
        Ok(Box::new(LocalTestRunner))
    }
}

pub trait TestRunner {
    fn test(
        &self,
        outer_env: &Environment,
        inner_env: &Environment,
        features: Option<&[String]>,
        args: &[String],
        build: bool,
    ) -> Result<()>;
}

pub trait ContainerTestRunner: TestRunner {
    fn container_name(&self) -> String;

    fn image_name(&self) -> String;

    fn network_name(&self) -> Option<&str>;

    fn needs_docker_socket(&self) -> bool;

    fn volumes(&self) -> Vec<String>;

    fn state(&self) -> Result<RunnerState> {
        let mut command = docker_command(["ps", "-a", "--format", "{{.Names}} {{.State}}"]);
        let container_name = self.container_name();

        for line in command.check_output()?.lines() {
            if let Some((name, state)) = line.split_once(' ')
                && name == container_name
            {
                return Ok(if state == "created" {
                    RunnerState::Created
                } else if state == "dead" {
                    RunnerState::Dead
                } else if state == "exited" || state.starts_with("Exited ") {
                    RunnerState::Exited
                } else if state == "paused" {
                    RunnerState::Paused
                } else if state == "restarting" {
                    RunnerState::Restarting
                } else if state == "running" || state.starts_with("Up ") {
                    RunnerState::Running
                } else {
                    RunnerState::Unknown
                });
            }
        }

        Ok(RunnerState::Missing)
    }

    fn image_exists(&self) -> Result<bool> {
        let mut command = docker_command(["images", "-q", &self.image_name()]);
        let output = command.check_output()?;
        Ok(!output.trim().is_empty())
    }

    fn ensure_running(
        &self,
        features: Option<&[String]>,
        config_environment_variables: &Environment,
        build: bool,
    ) -> Result<()> {
        match self.state()? {
            RunnerState::Running | RunnerState::Restarting => (),
            RunnerState::Created | RunnerState::Exited => self.start()?,
            RunnerState::Paused => self.unpause()?,
            RunnerState::Dead | RunnerState::Unknown => {
                self.remove()?;
                self.create(build)?; // Mount source when building
                self.start()?;
            }
            RunnerState::Missing => {
                if !build {
                    // Check if the image exists
                    if !self.image_exists()? {
                        anyhow::bail!(
                            "Test runner image '{}' does not exist. Build it first with 'vdev e2e build' or 'vdev int build'.",
                            self.image_name()
                        );
                    }
                    // Image exists but container doesn't - create it without mounting source
                    self.create(false)?;
                    self.start()?;
                } else {
                    // Build mode: build the image, then create and start the container with source mounted
                    self.build(features, config_environment_variables, true)?;
                    self.create(true)?;
                    self.start()?;
                }
            }
        }

        Ok(())
    }

    fn ensure_volumes(&self) -> Result<()> {
        let mut command = docker_command(["volume", "ls", "--format", "{{.Name}}"]);

        let mut volumes = HashSet::new();
        // Don't pre-create VOLUME_TARGET - let Docker initialize it from the image
        volumes.insert(VOLUME_CARGO_GIT);
        volumes.insert(VOLUME_CARGO_REGISTRY);
        for volume in command.check_output()?.lines() {
            volumes.take(volume);
        }

        for volume in &volumes {
            docker_command(["volume", "create", volume])
                .wait(format!("Creating volume {volume}"))?;
        }

        Ok(())
    }

    fn build(
        &self,
        features: Option<&[String]>,
        config_env_vars: &Environment,
        build_tests: bool,
    ) -> Result<()> {
        let image_name = self.image_name();

        let dockerfile = test_runner_dockerfile();
        let mut command = prepare_build_command(
            &image_name,
            &dockerfile,
            features,
            config_env_vars,
            build_tests,
        );
        waiting!("Building image {}", image_name);
        command.check_run()
    }

    fn start(&self) -> Result<()> {
        self.ensure_volumes()?;
        docker_command(["start", &self.container_name()])
            .wait(format!("Starting container {}", self.container_name()))
    }

    fn remove(&self) -> Result<()> {
        if matches!(self.state()?, RunnerState::Missing) {
            Ok(())
        } else {
            docker_command(["rm", "--force", "--volumes", &self.container_name()])
                .wait(format!("Removing container {}", self.container_name()))
        }
    }

    fn unpause(&self) -> Result<()> {
        docker_command(["unpause", &self.container_name()])
            .wait(format!("Unpausing container {}", self.container_name()))
    }

    fn create(&self, mount_source: bool) -> Result<()> {
        let network_name = self.network_name().unwrap_or("host");

        let docker_socket = format!("{}:/var/run/docker.sock", DOCKER_SOCKET.display());
        let docker_args = if self.needs_docker_socket() {
            vec!["--volume", &docker_socket]
        } else {
            vec![]
        };

        let volumes = self.volumes();
        let volumes: Vec<_> = volumes
            .iter()
            .flat_map(|volume| ["--volume", volume])
            .collect();

        self.ensure_volumes()?;

        // Use /vector (image's source location) when not mounting, /home/vector when mounting
        let workdir = if mount_source { MOUNT_PATH } else { "/vector" };

        // Build docker command conditionally based on whether source should be mounted
        let mut command = docker_command([
            "create",
            "--name",
            &self.container_name(),
            "--network",
            network_name,
            "--hostname",
            RUNNER_HOSTNAME,
            "--workdir",
            workdir,
        ]);

        // Conditionally add source volume mount
        if mount_source {
            command.args(["--volume", &format!("{}:{MOUNT_PATH}", app::path())]);
        }

        // Always mount target volume (initialized from image if empty)
        command.args(["--volume", &format!("{VOLUME_TARGET}:{TARGET_PATH}")]);

        // Only mount cargo volumes when building (to cache downloads across builds)
        // When using pre-built artifacts, use cargo registry/git from the image
        if mount_source {
            command.args([
                "--volume",
                &format!("{VOLUME_CARGO_GIT}:/usr/local/cargo/git"),
                "--volume",
                &format!("{VOLUME_CARGO_REGISTRY}:/usr/local/cargo/registry"),
            ]);
        }

        // Add additional volumes
        for volume_arg in volumes {
            command.arg(volume_arg);
        }

        // Add docker socket if needed
        for docker_arg in docker_args {
            command.arg(docker_arg);
        }

        command.args([&self.image_name(), "/bin/sleep", "infinity"]);
        command.wait(format!("Creating container {}", self.container_name()))
    }
}

impl<T> TestRunner for T
where
    T: ContainerTestRunner,
{
    fn test(
        &self,
        outer_env: &Environment,
        config_environment_variables: &Environment,
        features: Option<&[String]>,
        args: &[String],
        build: bool,
    ) -> Result<()> {
        self.ensure_running(features, config_environment_variables, build)?;

        let mut command = docker_command(["exec"]);
        if *IS_A_TTY {
            command.arg("--tty");
        }

        command.args(["--env", "RUST_BACKTRACE=1"]);
        command.args(["--env", &format!("CARGO_BUILD_TARGET_DIR={TARGET_PATH}")]);
        for (key, value) in outer_env {
            if let Some(value) = value {
                command.env(key, value);
            }
            command.args(["--env", key]);
        }
        append_environment_variables(&mut command, config_environment_variables);

        command.arg(self.container_name());
        command.args(TEST_COMMAND);
        command.args(args);

        command.check_run()
    }
}

#[derive(Debug)]
pub(super) struct IntegrationTestRunner {
    //If None, the runner image needs to be built with all test features.
    test_name: Option<String>,
    needs_docker_socket: bool,
    network: Option<String>,
    volumes: Vec<String>,
    test_kind: ComposeTestKind,
}

impl IntegrationTestRunner {
    pub(super) fn new(
        test_name: Option<String>,
        config: &IntegrationRunnerConfig,
        network: Option<String>,
        test_kind: ComposeTestKind,
    ) -> Result<Self> {
        let mut volumes: Vec<String> = config
            .volumes
            .iter()
            .map(|(a, b)| format!("{a}:{b}"))
            .collect();

        volumes.push(format!("{VOLUME_TARGET}:/output"));

        Ok(Self {
            test_name,
            needs_docker_socket: config.needs_docker_socket,
            network,
            volumes,
            test_kind,
        })
    }

    pub(super) fn ensure_network(&self) -> Result<()> {
        if let Some(network_name) = &self.network {
            let mut command = docker_command(["network", "ls", "--format", "{{.Name}}"]);

            if command
                .check_output()?
                .lines()
                .any(|network| network == network_name)
            {
                return Ok(());
            }

            docker_command(["network", "create", network_name]).wait("Creating network")
        } else {
            Ok(())
        }
    }

    pub(super) fn ensure_external_volumes(&self) -> Result<()> {
        // Get list of existing volumes
        let mut command = docker_command(["volume", "ls", "--format", "{{.Name}}"]);
        let existing_volumes: HashSet<String> =
            command.check_output()?.lines().map(String::from).collect();

        // Extract volume names from self.volumes (format is "volume_name:/mount/path")
        for volume_spec in &self.volumes {
            if let Some((volume_name, _)) = volume_spec.split_once(':') {
                // Only create named volumes (not paths like /host/path)
                if !volume_name.starts_with('/') && !existing_volumes.contains(volume_name) {
                    docker_command(["volume", "create", volume_name])
                        .wait(format!("Creating volume {volume_name}"))?;
                }
            }
        }

        Ok(())
    }
}

impl ContainerTestRunner for IntegrationTestRunner {
    fn network_name(&self) -> Option<&str> {
        self.network.as_deref()
    }

    fn container_name(&self) -> String {
        if let Some(test_name) = self.test_name.as_ref() {
            format!("{}-{}", self.test_kind.image_name(), test_name)
        } else {
            self.test_kind.image_name().to_string()
        }
    }

    fn image_name(&self) -> String {
        format!("{}:latest", self.container_name())
    }

    fn needs_docker_socket(&self) -> bool {
        self.needs_docker_socket
    }

    fn volumes(&self) -> Vec<String> {
        self.volumes.clone()
    }
}

pub struct DockerTestRunner;

impl ContainerTestRunner for DockerTestRunner {
    fn network_name(&self) -> Option<&str> {
        None
    }

    fn container_name(&self) -> String {
        format!("vector-test-runner-{}", RustToolchainConfig::rust_version())
    }

    fn image_name(&self) -> String {
        env::var("ENVIRONMENT_UPSTREAM").unwrap_or_else(|_| UPSTREAM_IMAGE.to_string())
    }

    fn needs_docker_socket(&self) -> bool {
        false
    }

    fn volumes(&self) -> Vec<String> {
        Vec::default()
    }
}

pub struct LocalTestRunner;

impl TestRunner for LocalTestRunner {
    fn test(
        &self,
        outer_env: &Environment,
        inner_env: &Environment,
        _features: Option<&[String]>,
        args: &[String],
        _build: bool,
    ) -> Result<()> {
        let mut command = Command::new(TEST_COMMAND[0]);
        command.args(&TEST_COMMAND[1..]);
        command.args(args);

        for (key, value) in outer_env {
            if let Some(value) = value {
                command.env(key, value);
            }
        }
        for (key, value) in inner_env {
            if let Some(value) = value {
                command.env(key, value);
            }
        }

        command.check_run()
    }
}
