use anyhow::Result;
use std::{collections::HashSet, env, process::Command};

use super::config::{IntegrationRunnerConfig, RustToolchainConfig};
use crate::{
    app::{self, CommandExt as _},
    testing::{
        build::prepare_build_command,
        docker::{DOCKER_SOCKET, docker_command},
        test_runner_dockerfile,
    },
    utils::{
        IS_A_TTY,
        command::ChainArgs as _,
        environment::{Environment, append_environment_variables},
    },
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
                self.create()?;
                self.start()?;
            }
            RunnerState::Missing => {
                self.build(features, config_environment_variables, build)?;
                self.create()?;
                self.start()?;
            }
        }

        Ok(())
    }

    fn ensure_volumes(&self) -> Result<()> {
        let mut command = docker_command(["volume", "ls", "--format", "{{.Name}}"]);

        let mut volumes = HashSet::new();
        volumes.insert(VOLUME_TARGET);
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
        build: bool,
    ) -> Result<()> {
        let image_name = self.image_name();

        let dockerfile = test_runner_dockerfile();
        let mut command =
            prepare_build_command(&image_name, &dockerfile, features, config_env_vars, build);
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

    fn create(&self) -> Result<()> {
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
        docker_command(
            [
                "create",
                "--name",
                &self.container_name(),
                "--network",
                network_name,
                "--hostname",
                RUNNER_HOSTNAME,
                "--workdir",
                MOUNT_PATH,
                "--volume",
                &format!("{}:{MOUNT_PATH}", app::path()),
                "--volume",
                &format!("{VOLUME_TARGET}:{TARGET_PATH}"),
                "--volume",
                &format!("{VOLUME_CARGO_GIT}:/usr/local/cargo/git"),
                "--volume",
                &format!("{VOLUME_CARGO_REGISTRY}:/usr/local/cargo/registry"),
            ]
            .chain_args(volumes)
            .chain_args(docker_args)
            .chain_args([&self.image_name(), "/bin/sleep", "infinity"]),
        )
        .wait(format!("Creating container {}", self.container_name()))
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
    // The integration is None when compiling the runner image with the `all-integration-tests` feature.
    integration: Option<String>,
    needs_docker_socket: bool,
    network: Option<String>,
    volumes: Vec<String>,
}

impl IntegrationTestRunner {
    pub(super) fn new(
        integration: Option<String>,
        config: &IntegrationRunnerConfig,
        network: Option<String>,
    ) -> Result<Self> {
        let mut volumes: Vec<String> = config
            .volumes
            .iter()
            .map(|(a, b)| format!("{a}:{b}"))
            .collect();

        volumes.push(format!("{VOLUME_TARGET}:/output"));

        Ok(Self {
            integration,
            needs_docker_socket: config.needs_docker_socket,
            network,
            volumes,
        })
    }

    /// Returns true if this runner uses the shared image (with all features)
    pub(super) fn is_shared_runner(&self) -> bool {
        self.integration.is_none()
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
        if let Some(integration) = self.integration.as_ref() {
            format!(
                "vector-test-runner-{}-{}",
                integration,
                RustToolchainConfig::rust_version()
            )
        } else {
            format!("vector-test-runner-{}", RustToolchainConfig::rust_version())
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
