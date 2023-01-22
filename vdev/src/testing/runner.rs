use std::collections::{BTreeMap, HashSet};
use std::process::{Command, Stdio};
use std::{env, ffi::OsString, path::PathBuf};

use anyhow::Result;
use atty::Stream;
use once_cell::sync::Lazy;

use super::config::RustToolchainConfig;
use crate::app::{self, CommandExt as _};

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
// The upstream container we publish artifacts to on a successful master build.
const UPSTREAM_IMAGE: &str =
    "docker.io/timberio/vector-dev:sha-3eadc96742a33754a5859203b58249f6a806972a";

static CONTAINER_TOOL: Lazy<OsString> =
    Lazy::new(|| env::var_os("CONTAINER_TOOL").unwrap_or_else(detect_container_tool));

fn detect_container_tool() -> OsString {
    for tool in ["docker", "podman"] {
        if Command::new(tool)
            .arg("version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .and_then(|mut child| child.wait())
            .map_or(false, |status| status.success())
        {
            return OsString::from(String::from(tool));
        }
    }
    critical!("No container tool could be detected.");
    std::process::exit(1);
}

fn dockercmd<'a>(args: impl IntoIterator<Item = &'a str>) -> Command {
    let mut command = Command::new(&*CONTAINER_TOOL);
    command.args(args);
    command
}

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
    fn test(&self, env_vars: &BTreeMap<String, String>, args: &[String]) -> Result<()>;
}

pub trait ContainerTestRunnerBase: TestRunner {
    fn container_name(&self) -> String;

    fn image_name(&self) -> String;

    fn network_name(&self) -> String {
        "host".to_string()
    }

    fn stop(&self) -> Result<()> {
        dockercmd(["stop", "--time", "0", &self.container_name()])
            .wait(format!("Stopping container {}", self.container_name()))
    }
}

trait ContainerTestRunner: ContainerTestRunnerBase {
    fn get_rust_version(&self) -> &str;

    fn state(&self) -> Result<RunnerState> {
        let mut command = dockercmd(["ps", "-a", "--format", "{{.Names}} {{.State}}"]);

        for line in command.capture_output()?.lines() {
            if let Some((name, state)) = line.split_once(' ') {
                if name == self.container_name() {
                    return Ok(match state {
                        "created" => RunnerState::Created,
                        "dead" => RunnerState::Dead,
                        "exited" => RunnerState::Exited,
                        "paused" => RunnerState::Paused,
                        "restarting" => RunnerState::Restarting,
                        "running" => RunnerState::Running,
                        _ => RunnerState::Unknown,
                    });
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
        let mut command = dockercmd(["volume", "ls", "--format", "{{.Name}}"]);

        let mut volumes = HashSet::new();
        volumes.insert(VOLUME_TARGET);
        volumes.insert(VOLUME_CARGO_GIT);
        volumes.insert(VOLUME_CARGO_REGISTRY);
        for volume in command.capture_output()?.lines() {
            volumes.take(volume);
        }

        for volume in &volumes {
            dockercmd(["volume", "create", volume]).wait(format!("Creating volume {volume}"))?;
        }

        Ok(())
    }

    fn build(&self) -> Result<()> {
        let dockerfile: PathBuf = [app::path(), "scripts", "integration", "Dockerfile"]
            .iter()
            .collect();
        let mut command = dockercmd(["build"]);
        command.current_dir(app::path());
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
        command.check_run()
    }

    fn start(&self) -> Result<()> {
        dockercmd(["start", &self.container_name()])
            .wait(format!("Starting container {}", self.container_name()))
    }

    fn remove(&self) -> Result<()> {
        dockercmd(["rm", &self.container_name()])
            .wait(format!("Removing container {}", self.container_name()))
    }

    fn unpause(&self) -> Result<()> {
        dockercmd(["unpause", &self.container_name()])
            .wait(format!("Unpausing container {}", self.container_name()))
    }

    fn create(&self) -> Result<()> {
        dockercmd([
            "create",
            "--name",
            &self.container_name(),
            "--network",
            &self.network_name(),
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
            &self.image_name(),
            "/bin/sleep",
            "infinity",
        ])
        .wait(format!("Creating container {}", self.container_name()))
    }
}

pub struct IntegrationTestRunner {
    integration: String,
    rust_version: String,
}

impl IntegrationTestRunner {
    pub fn new(integration: String) -> Result<Self> {
        let rust_version = RustToolchainConfig::parse()?.channel;
        Ok(Self {
            integration,
            rust_version,
        })
    }

    pub fn ensure_network(&self) -> Result<()> {
        let mut command = dockercmd(["network", "ls", "--format", "{{.Name}}"]);

        if command
            .capture_output()?
            .lines()
            .any(|network| network == self.network_name())
        {
            return Ok(());
        }

        dockercmd(["network", "create", &self.network_name()]).wait("Creating network")
    }
}

impl TestRunner for IntegrationTestRunner {
    fn test(&self, env_vars: &BTreeMap<String, String>, args: &[String]) -> Result<()> {
        self.verify_state()?;

        let mut command = dockercmd(["exec"]);
        if atty::is(Stream::Stdout) {
            command.arg("--tty");
        }

        command.args(["--env", &format!("CARGO_BUILD_TARGET_DIR={TARGET_PATH}")]);
        for (key, value) in env_vars {
            command.env(key, value);
            command.args(["--env", key]);
        }

        command.arg(&self.container_name());
        command.args(TEST_COMMAND);
        command.args(args);

        command.check_run()
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
    fn get_rust_version(&self) -> &str {
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
    fn test(&self, env_vars: &BTreeMap<String, String>, args: &[String]) -> Result<()> {
        self.verify_state()?;

        let mut command = dockercmd(["exec"]);
        if atty::is(Stream::Stdout) {
            command.arg("--tty");
        }

        command.args(["--env", &format!("CARGO_BUILD_TARGET_DIR={TARGET_PATH}")]);
        for (key, value) in env_vars {
            command.env(key, value);
            command.args(["--env", key]);
        }

        command.arg(&self.container_name());
        command.args(TEST_COMMAND);
        command.args(args);

        command.check_run()
    }
}

impl ContainerTestRunnerBase for DockerTestRunner {
    fn container_name(&self) -> String {
        format!("vector-test-runner-{}", self.rust_version)
    }

    fn image_name(&self) -> String {
        env::var("ENVIRONMENT_UPSTREAM").unwrap_or_else(|_| UPSTREAM_IMAGE.to_string())
    }
}

impl ContainerTestRunner for DockerTestRunner {
    fn get_rust_version(&self) -> &str {
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
    fn test(&self, env_vars: &BTreeMap<String, String>, args: &[String]) -> Result<()> {
        let mut command = Command::new(TEST_COMMAND[0]);
        command.args(&TEST_COMMAND[1..]);
        command.args(args);

        for (key, value) in env_vars {
            command.env(key, value);
        }

        command.check_run()
    }
}
