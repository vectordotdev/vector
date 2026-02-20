use std::{
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};

use super::{
    config::{
        ComposeConfig, ComposeTestConfig, E2E_TESTS_DIR, INTEGRATION_TESTS_DIR, RustToolchainConfig,
    },
    runner::{ContainerTestRunner as _, IntegrationTestRunner, TestRunner as _},
};
use crate::{
    app::CommandExt as _,
    testing::{
        build::ALL_INTEGRATIONS_FEATURE_FLAG,
        docker::{CONTAINER_TOOL, DOCKER_SOCKET},
    },
    utils::environment::{Environment, extract_present, rename_environment_keys},
};

const NETWORK_ENV_VAR: &str = "VECTOR_NETWORK";
const E2E_FEATURE_FLAG: &str = "all-e2e-tests";

/// Check if a Docker image exists locally
fn docker_image_exists(image_name: &str) -> Result<bool> {
    use crate::testing::docker::docker_command;
    let output =
        docker_command(["images", "--format", "{{.Repository}}:{{.Tag}}"]).check_output()?;
    Ok(output.lines().any(|line| line == image_name))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ComposeTestKind {
    E2E,
    Integration,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ComposeTestLocalConfig {
    pub(crate) kind: ComposeTestKind,
    pub(crate) directory: &'static str,
    pub(crate) feature_flag: &'static str,
}

impl ComposeTestLocalConfig {
    /// Integration tests are located in the `tests/integration` dir,
    /// and are the full feature flag is `all-integration-tests`.
    pub(crate) fn integration() -> Self {
        Self {
            kind: ComposeTestKind::Integration,
            directory: INTEGRATION_TESTS_DIR,
            feature_flag: ALL_INTEGRATIONS_FEATURE_FLAG,
        }
    }

    /// E2E tests are located in the `tests/e2e` dir,
    /// and the full feature flag is `all-e2e-tests`.
    pub(crate) fn e2e() -> Self {
        Self {
            kind: ComposeTestKind::E2E,
            directory: E2E_TESTS_DIR,
            feature_flag: E2E_FEATURE_FLAG,
        }
    }
}

#[derive(Debug)]
pub(crate) struct ComposeTest {
    local_config: ComposeTestLocalConfig,
    test_name: String,
    environment: String,
    config: ComposeTestConfig,
    runner: IntegrationTestRunner,
    compose: Option<Compose>,
    env_config: Environment,
    retries: u8,
}

impl ComposeTest {
    pub(crate) fn generate(
        local_config: ComposeTestLocalConfig,
        test_name: impl Into<String>,
        environment: impl Into<String>,
        retries: u8,
    ) -> Result<ComposeTest> {
        let test_name: String = test_name.into();
        let environment = environment.into();
        let (test_dir, config) = ComposeTestConfig::load(local_config.directory, &test_name)?;
        let Some(mut env_config) = config.environments().get(&environment).cloned() else {
            bail!("Could not find environment named {environment:?}");
        };

        let network_name = format!("vector-integration-tests-{test_name}");
        let compose = Compose::new(test_dir, env_config.clone(), network_name.clone())?;

        // Auto-detect: If shared image exists, use it. Otherwise use per-test image.
        // Shared image: vector-test-runner-1.90:latest (compiled with all-integration-tests)
        // Per-test image: vector-test-runner-clickhouse-1.90:latest (compiled with specific features)
        let shared_image_name = format!(
            "vector-test-runner-{}:latest",
            RustToolchainConfig::rust_version()
        );
        let runner_name = if docker_image_exists(&shared_image_name).unwrap_or(false) {
            info!("Using shared runner image: {shared_image_name}");
            None
        } else {
            info!("Shared runner image not found, will build image for: {test_name}");
            Some(test_name.clone())
        };

        let runner = IntegrationTestRunner::new(
            runner_name,
            &config.runner,
            compose.is_some().then_some(network_name),
        )?;

        env_config.insert("VECTOR_IMAGE".to_string(), Some(runner.image_name()));

        let compose_test = ComposeTest {
            local_config,
            test_name,
            environment,
            config,
            runner,
            compose,
            env_config: rename_environment_keys(&env_config),
            retries,
        };
        trace!("Generated {compose_test:#?}");
        Ok(compose_test)
    }

    fn project_name(&self) -> String {
        // Docker Compose project names must consist only of lowercase alphanumeric characters,
        // hyphens, and underscores. Replace any dots with hyphens.
        let sanitized_env = self.environment.replace('.', "-");
        format!(
            "vector-{}-{}-{}",
            self.local_config.directory, self.test_name, sanitized_env
        )
    }

    fn is_running(&self) -> Result<bool> {
        let Some(compose) = &self.compose else {
            return Ok(false);
        };

        let output = Command::new(CONTAINER_TOOL.clone())
            .args([
                "compose",
                "--project-name",
                &self.project_name(),
                "ps",
                "--format",
                "json",
                "--status",
                "running",
            ])
            .current_dir(&compose.test_dir)
            .envs(
                compose
                    .env
                    .iter()
                    .filter_map(|(k, v)| v.as_ref().map(|val| (k, val))),
            )
            .output()
            .with_context(|| "Failed to check if compose environment is running")?;

        // If stdout is empty or "[]", no containers are running
        Ok(!output.stdout.is_empty() && output.stdout != b"[]\n" && output.stdout != b"[]")
    }

    pub(crate) fn test(&self, extra_args: Vec<String>) -> Result<()> {
        let was_running = self.is_running()?;
        self.config.check_required()?;

        if !was_running {
            self.start()?;
        }

        let mut env_vars = self.config.env.clone();
        // Make sure the test runner has the same config environment vars as the services do.
        for (key, value) in self.env_config.clone() {
            env_vars.insert(key, value);
        }

        env_vars.insert("VECTOR_LOG".to_string(), Some("info".into()));
        let mut args = self.config.args.clone().unwrap_or_default();

        args.push("--features".to_string());

        // If using shared runner: use 'all-integration-tests' or 'all-e2e-tests'
        // If using per-test runner: use test-specific features from test.yaml
        args.push(if self.runner.is_shared_runner() {
            self.local_config.feature_flag.to_string()
        } else {
            self.config.features.join(",")
        });

        // If the test field is not present then use the --lib flag
        match self.config.test {
            Some(ref test_arg) => {
                args.push("--test".to_string());
                args.push(test_arg.clone());
            }
            None => args.push("--lib".to_string()),
        }

        // Ensure the test_filter args are passed as well
        if let Some(ref filter) = self.config.test_filter {
            args.push(filter.clone());
        }
        args.extend(extra_args);

        // Some arguments are not compatible with the --no-capture arg
        if !args.contains(&"--test-threads".to_string()) {
            args.push("--no-capture".to_string());
        }

        if self.retries > 0 {
            args.push("--retries".to_string());
            args.push(self.retries.to_string());
        }

        self.runner.test(
            &env_vars,
            &self.config.runner.env,
            Some(&self.config.features),
            &args,
            self.local_config.kind == ComposeTestKind::E2E,
        )?;

        Ok(())
    }

    pub(crate) fn start(&self) -> Result<()> {
        // For end-to-end tests, we want to run vector as a service, leveraging the
        // image for the runner. So we must build that image before starting the
        // compose so that it is available.
        if self.local_config.kind == ComposeTestKind::E2E {
            self.runner.build(
                Some(&self.config.features),
                &self.env_config,
                true, // E2E tests build Vector in the image
            )?;
        }

        self.config.check_required()?;
        if let Some(compose) = &self.compose {
            self.runner.ensure_network()?;
            self.runner.ensure_external_volumes()?;

            if self.is_running()? {
                bail!("environment is already up");
            }

            let project_name = self.project_name();
            compose.start(&self.env_config, &project_name)?;
        }
        Ok(())
    }

    pub(crate) fn stop(&self) -> Result<()> {
        if let Some(compose) = &self.compose {
            if !self.is_running()? {
                bail!("No environment for {} is up.", self.test_name);
            }

            let project_name = self.project_name();
            compose.stop(&self.env_config, &project_name)?;
        }

        self.runner.remove()?;

        Ok(())
    }
}

#[derive(Debug)]
struct Compose {
    yaml_path: PathBuf,
    test_dir: PathBuf,
    env: Environment,
    #[cfg_attr(target_family = "windows", allow(dead_code))]
    config: ComposeConfig,
    network: String,
}

impl Compose {
    fn new(test_dir: PathBuf, env: Environment, network: String) -> Result<Option<Self>> {
        let yaml_path: PathBuf = [&test_dir, Path::new("compose.yaml")].iter().collect();

        match yaml_path.try_exists() {
            Err(error) => {
                Err(error).with_context(|| format!("Could not lookup {}", yaml_path.display()))
            }
            Ok(false) => Ok(None),
            Ok(true) => {
                // Parse config only for unix volume permission checking
                let config = ComposeConfig::parse(&yaml_path)?;

                Ok(Some(Self {
                    yaml_path,
                    test_dir,
                    env,
                    config,
                    network,
                }))
            }
        }
    }

    fn start(&self, environment: &Environment, project_name: &str) -> Result<()> {
        #[cfg(unix)]
        unix::prepare_compose_volumes(&self.config, &self.test_dir, environment)?;

        self.run(
            "Starting",
            &["up", "--detach"],
            Some(environment),
            project_name,
        )
    }

    fn stop(&self, environment: &Environment, project_name: &str) -> Result<()> {
        self.run(
            "Stopping",
            &["down", "--timeout", "0", "--volumes", "--remove-orphans"],
            Some(environment),
            project_name,
        )
    }

    fn run(
        &self,
        action: &str,
        args: &[&'static str],
        environment: Option<&Environment>,
        project_name: &str,
    ) -> Result<()> {
        let mut command = Command::new(CONTAINER_TOOL.clone());
        command.arg("compose");
        command.arg("--project-name");
        command.arg(project_name);
        command.arg("--file");
        command.arg(&self.yaml_path);

        command.args(args);

        command.current_dir(&self.test_dir);

        command.env("DOCKER_SOCKET", &*DOCKER_SOCKET);
        command.env(NETWORK_ENV_VAR, &self.network);

        // some services require this in order to build Vector
        command.env("RUST_VERSION", RustToolchainConfig::rust_version());

        for (key, value) in &self.env {
            if let Some(value) = value {
                command.env(key, value);
            }
        }
        if let Some(environment) = environment {
            command.envs(extract_present(environment));
        }

        waiting!("{action} service environment");
        command.check_run()
    }
}

#[cfg(unix)]
mod unix {
    use std::{
        fs::{self, Metadata, Permissions},
        os::unix::fs::PermissionsExt as _,
        path::{Path, PathBuf},
    };

    use anyhow::{Context, Result};

    use super::super::config::ComposeConfig;
    use crate::{
        testing::config::VolumeMount,
        utils::environment::{Environment, resolve_placeholders},
    };

    /// Unix permissions mask to allow everybody to read a file
    const ALL_READ: u32 = 0o444;
    /// Unix permissions mask to allow everybody to read a directory
    const ALL_READ_DIR: u32 = 0o555;

    /// Fix up potential issues before starting a compose container
    pub fn prepare_compose_volumes(
        config: &ComposeConfig,
        test_dir: &Path,
        environment: &Environment,
    ) -> Result<()> {
        for service in config.services.values() {
            if let Some(volumes) = &service.volumes {
                for volume in volumes {
                    let source = match volume {
                        VolumeMount::Short(s) => {
                            s.split_once(':').map(|(s, _)| s).ok_or_else(|| {
                                anyhow::anyhow!("Invalid short volume mount format: {s}")
                            })?
                        }
                        VolumeMount::Long { source, .. } => source,
                    };
                    let source = resolve_placeholders(source, environment);
                    if !config.volumes.contains_key(&source)
                        && !source.starts_with('/')
                        && !source.starts_with('$')
                    {
                        let path: PathBuf = [test_dir, Path::new(&source)].iter().collect();
                        add_read_permission(&path)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Recursively add read permissions to the
    fn add_read_permission(path: &Path) -> Result<()> {
        let metadata = path
            .metadata()
            .with_context(|| format!("Could not get permissions on {}", path.display()))?;

        if metadata.is_file() {
            add_permission(path, &metadata, ALL_READ)
        } else {
            if metadata.is_dir() {
                add_permission(path, &metadata, ALL_READ_DIR)?;
                for entry in fs::read_dir(path)
                    .with_context(|| format!("Could not read directory {}", path.display()))?
                {
                    let entry = entry.with_context(|| {
                        format!("Could not read directory entry in {}", path.display())
                    })?;
                    add_read_permission(&entry.path())?;
                }
            }
            Ok(())
        }
    }

    fn add_permission(path: &Path, metadata: &Metadata, bits: u32) -> Result<()> {
        let perms = metadata.permissions();
        let new_perms = Permissions::from_mode(perms.mode() | bits);
        if new_perms != perms {
            fs::set_permissions(path, new_perms)
                .with_context(|| format!("Could not set permissions on {}", path.display()))?;
        }
        Ok(())
    }
}
