use std::{collections::BTreeMap, fs, path::Path, path::PathBuf, process::Command};

use anyhow::{bail, Context, Result};
use tempfile::{Builder, NamedTempFile};

use super::config::{
    ComposeConfig, ComposeTestConfig, Environment, E2E_TESTS_DIR, INTEGRATION_TESTS_DIR,
};
use super::runner::{
    ContainerTestRunner as _, IntegrationTestRunner, TestRunner as _, CONTAINER_TOOL, DOCKER_SOCKET,
};
use super::state::EnvsDir;
use crate::app::CommandExt as _;
use crate::testing::config::get_rust_version;

const NETWORK_ENV_VAR: &str = "VECTOR_NETWORK";

const INTEGRATION_FEATURE_FLAG: &str = "all-integration-tests";
const E2E_FEATURE_FLAG: &str = "all-e2e-tests";

pub(crate) struct ComposeTest {
    test_name: String,
    environment: String,
    config: ComposeTestConfig,
    envs_dir: EnvsDir,
    runner: IntegrationTestRunner,
    compose: Option<Compose>,
    env_config: Environment,
    build_all: bool,
    retries: u8,
}

pub(crate) trait ComposeTestT {
    const DIRECTORY: &'static str;

    const FEATURE_FLAG: &'static str;

    fn generate(
        test_name: impl Into<String>,
        environment: impl Into<String>,
        build_all: bool,
        retries: u8,
    ) -> Result<ComposeTest> {
        let test_name = test_name.into();
        let environment = environment.into();
        let (test_dir, config) = ComposeTestConfig::load(Self::DIRECTORY, &test_name)?;
        let envs_dir = EnvsDir::new(&test_name);
        let Some(mut env_config) = config.environments().get(&environment).map(Clone::clone) else {
            bail!("Could not find environment named {environment:?}");
        };

        let network_name = format!("vector-integration-tests-{test_name}");
        let compose = Compose::new(test_dir, env_config.clone(), network_name.clone())?;

        // None if compiling with all integration test feature flag.
        let runner_name = (!build_all).then(|| test_name.clone());

        let runner = IntegrationTestRunner::new(
            runner_name,
            &config.runner,
            compose.is_some().then_some(network_name),
        )?;

        env_config.insert("VECTOR_IMAGE".to_string(), Some(runner.image_name()));

        Ok(ComposeTest {
            test_name,
            environment,
            config,
            envs_dir,
            runner,
            compose,
            env_config,
            build_all,
            retries,
        })
    }

    fn test(compose_test: &ComposeTest, extra_args: Vec<String>) -> Result<()> {
        let active = compose_test
            .envs_dir
            .check_active(&compose_test.environment)?;
        compose_test.config.check_required()?;

        if !active {
            Self::start(compose_test)?;
        }

        let mut env_vars = compose_test.config.env.clone();
        // Make sure the test runner has the same config environment vars as the services do.
        for (key, value) in config_env(&compose_test.env_config) {
            env_vars.insert(key, Some(value));
        }

        env_vars.insert("TEST_LOG".to_string(), Some("info".into()));
        let mut args = compose_test.config.args.clone().unwrap_or_default();

        args.push("--features".to_string());

        args.push(if compose_test.build_all {
            Self::FEATURE_FLAG.to_string()
        } else {
            compose_test.config.features.join(",")
        });

        // If the test field is not present then use the --lib flag
        match compose_test.config.test {
            Some(ref test_arg) => {
                args.push("--test".to_string());
                args.push(test_arg.to_string());
            }
            None => args.push("--lib".to_string()),
        }

        // Ensure the test_filter args are passed as well
        if let Some(ref filter) = compose_test.config.test_filter {
            args.push(filter.to_string());
        }
        args.extend(extra_args);

        // Some arguments are not compatible with the --no-capture arg
        if !args.contains(&"--test-threads".to_string()) {
            args.push("--no-capture".to_string());
        }

        if compose_test.retries > 0 {
            args.push("--retries".to_string());
            args.push(compose_test.retries.to_string());
        }

        compose_test.runner.test(
            &env_vars,
            &compose_test.config.runner.env,
            Some(&compose_test.config.features),
            &args,
            Self::DIRECTORY,
        )?;

        if !active {
            compose_test.runner.remove()?;
            Self::stop(compose_test)?;
        }
        Ok(())
    }

    fn start(compose_test: &ComposeTest) -> Result<()> {
        // For end-to-end tests, we want to run vector as a service, leveraging the
        // image for the runner. So we must build that image before starting the
        // compose so that it is available.
        if Self::DIRECTORY == E2E_TESTS_DIR {
            compose_test
                .runner
                .build(Some(&compose_test.config.features), Self::DIRECTORY)?;
        }

        compose_test.config.check_required()?;
        if let Some(compose) = &compose_test.compose {
            compose_test.runner.ensure_network()?;

            if compose_test
                .envs_dir
                .check_active(&compose_test.environment)?
            {
                bail!("environment is already up");
            }

            compose.start(&compose_test.env_config)?;

            compose_test
                .envs_dir
                .save(&compose_test.environment, &compose_test.env_config)
        } else {
            Ok(())
        }
    }

    fn stop(compose_test: &ComposeTest) -> Result<()> {
        if let Some(compose) = &compose_test.compose {
            // TODO: Is this check really needed?
            if compose_test.envs_dir.load()?.is_none() {
                bail!("No environment for {} is up.", compose_test.test_name);
            }

            compose_test.runner.remove()?;
            compose.stop()?;
            compose_test.envs_dir.remove()?;
        }

        Ok(())
    }
}

/// Integration tests are located in the `scripts/integration` dir,
/// and are the full feature flag is `all-integration-tests`.
pub(crate) struct IntegrationTest;

impl ComposeTestT for IntegrationTest {
    const DIRECTORY: &'static str = INTEGRATION_TESTS_DIR;

    const FEATURE_FLAG: &'static str = INTEGRATION_FEATURE_FLAG;
}

/// E2E tests are located in the `scripts/e2e` dir,
/// and are the full feature flag is `all-e2e-tests`.
pub(crate) struct E2ETest;

impl ComposeTestT for E2ETest {
    const DIRECTORY: &'static str = E2E_TESTS_DIR;

    const FEATURE_FLAG: &'static str = E2E_FEATURE_FLAG;
}

struct Compose {
    original_path: PathBuf,
    test_dir: PathBuf,
    env: Environment,
    #[cfg_attr(target_family = "windows", allow(dead_code))]
    config: ComposeConfig,
    network: String,
    temp_file: NamedTempFile,
}

impl Compose {
    fn new(test_dir: PathBuf, env: Environment, network: String) -> Result<Option<Self>> {
        let original_path: PathBuf = [&test_dir, Path::new("compose.yaml")].iter().collect();

        match original_path.try_exists() {
            Err(error) => Err(error).with_context(|| format!("Could not lookup {original_path:?}")),
            Ok(false) => Ok(None),
            Ok(true) => {
                let mut config = ComposeConfig::parse(&original_path)?;
                // Inject the networks block
                config.networks.insert(
                    "default".to_string(),
                    BTreeMap::from_iter([
                        ("name".to_string(), network.clone()),
                        ("external".to_string(), "true".to_string()),
                    ]),
                );

                // Create a named tempfile, there may be resource leakage here in case of SIGINT
                // Tried tempfile::tempfile() but this returns a File object without a usable path
                // https://docs.rs/tempfile/latest/tempfile/#resource-leaking
                let temp_file = Builder::new()
                    .prefix("compose-temp-")
                    .suffix(".yaml")
                    .tempfile_in(&test_dir)
                    .with_context(|| "Failed to create temporary compose file")?;

                fs::write(
                    temp_file.path(),
                    serde_yaml::to_string(&config)
                        .with_context(|| "Failed to serialize modified compose.yaml")?,
                )?;

                Ok(Some(Self {
                    original_path,
                    test_dir,
                    env,
                    config,
                    network,
                    temp_file,
                }))
            }
        }
    }

    fn start(&self, config: &Environment) -> Result<()> {
        self.prepare()?;
        self.run("Starting", &["up", "--detach"], Some(config))
    }

    fn stop(&self) -> Result<()> {
        // The config settings are not needed when stopping a compose setup.
        self.run("Stopping", &["down", "--timeout", "0", "--volumes"], None)
    }

    fn run(&self, action: &str, args: &[&'static str], config: Option<&Environment>) -> Result<()> {
        let mut command = CONTAINER_TOOL.clone();
        command.push("-compose");
        let mut command = Command::new(command);
        // When the integration test environment is already active, the tempfile path does not
        // exist because `Compose::new()` has not been called. In this case, the `stop` command
        // needs to use the calculated path from the integration name instead of the nonexistent
        // tempfile path. This is because `stop` doesn't go through the same logic as `start`
        // and doesn't create a new tempfile before calling docker compose.
        // If stop command needs to use some of the injected bits then we need to rebuild it
        command.arg("--file");
        if config.is_none() {
            command.arg(&self.original_path);
        } else {
            command.arg(self.temp_file.path());
        }

        command.args(args);

        command.current_dir(&self.test_dir);

        command.env("DOCKER_SOCKET", &*DOCKER_SOCKET);
        command.env(NETWORK_ENV_VAR, &self.network);

        // some services require this in order to build Vector
        command.env("RUST_VERSION", get_rust_version());

        for (key, value) in &self.env {
            if let Some(value) = value {
                command.env(key, value);
            }
        }
        if let Some(config) = config {
            command.envs(config_env(config));
        }

        waiting!("{action} service environment");
        command.check_run()
    }

    fn prepare(&self) -> Result<()> {
        #[cfg(unix)]
        unix::prepare_compose_volumes(&self.config, &self.test_dir)?;
        Ok(())
    }
}

fn config_env(config: &Environment) -> impl Iterator<Item = (String, String)> + '_ {
    config.iter().filter_map(|(var, value)| {
        value.as_ref().map(|value| {
            (
                format!("CONFIG_{}", var.replace('-', "_").to_uppercase()),
                value.to_string(),
            )
        })
    })
}

#[cfg(unix)]
mod unix {
    use std::fs::{self, Metadata, Permissions};
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::{Path, PathBuf};

    use anyhow::{Context, Result};

    use super::super::config::ComposeConfig;

    /// Unix permissions mask to allow everybody to read a file
    const ALL_READ: u32 = 0o444;
    /// Unix permissions mask to allow everybody to read a directory
    const ALL_READ_DIR: u32 = 0o555;

    /// Fix up potential issues before starting a compose container
    pub fn prepare_compose_volumes(config: &ComposeConfig, test_dir: &Path) -> Result<()> {
        for service in config.services.values() {
            // Make sure all volume files are world readable
            if let Some(volumes) = &service.volumes {
                for volume in volumes {
                    let source = volume
                        .split_once(':')
                        .expect("Invalid volume in compose file")
                        .0;
                    // Only fixup relative paths, i.e. within our source tree.
                    if !config.volumes.contains_key(source)
                        && !source.starts_with('/')
                        && !source.starts_with('$')
                    {
                        let path: PathBuf = [test_dir, Path::new(source)].iter().collect();
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
            .with_context(|| format!("Could not get permissions on {path:?}"))?;

        if metadata.is_file() {
            add_permission(path, &metadata, ALL_READ)
        } else {
            if metadata.is_dir() {
                add_permission(path, &metadata, ALL_READ_DIR)?;
                for entry in fs::read_dir(path)
                    .with_context(|| format!("Could not read directory {path:?}"))?
                {
                    let entry = entry
                        .with_context(|| format!("Could not read directory entry in {path:?}"))?;
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
                .with_context(|| format!("Could not set permissions on {path:?}"))?;
        }
        Ok(())
    }
}
