use std::{collections::BTreeMap, fs, path::Path, path::PathBuf, process::Command};

use anyhow::{bail, Context, Result};
use tempfile::{Builder, NamedTempFile};

use super::config::ComposeConfig;
use super::config::{Environment, IntegrationTestConfig};
use super::runner::{
    ContainerTestRunner as _, IntegrationTestRunner, TestRunner as _, CONTAINER_TOOL, DOCKER_SOCKET,
};
use super::state::EnvsDir;
use crate::app::CommandExt as _;

const NETWORK_ENV_VAR: &str = "VECTOR_NETWORK";

pub struct IntegrationTest {
    integration: String,
    environment: String,
    config: IntegrationTestConfig,
    envs_dir: EnvsDir,
    runner: IntegrationTestRunner,
    compose: Option<Compose>,
    env_config: Environment,
}

impl IntegrationTest {
    pub fn new(integration: impl Into<String>, environment: impl Into<String>) -> Result<Self> {
        let integration = integration.into();
        let environment = environment.into();
        let (test_dir, config) = IntegrationTestConfig::load(&integration)?;
        let envs_dir = EnvsDir::new(&integration);
        let Some(env_config) = config.environments().get(&environment).map(Clone::clone) else {
            bail!("Could not find environment named {environment:?}");
        };
        let network_name = format!("vector-integration-tests-{integration}");
        let compose = Compose::new(test_dir, env_config.clone(), network_name.clone())?;
        let runner = IntegrationTestRunner::new(
            integration.clone(),
            &config.runner,
            compose.is_some().then_some(network_name),
        )?;

        Ok(Self {
            integration,
            environment,
            config,
            envs_dir,
            runner,
            compose,
            env_config,
        })
    }

    pub fn test(self, extra_args: Vec<String>) -> Result<()> {
        let active = self.envs_dir.check_active(&self.environment)?;
        self.config.check_required()?;

        if !active {
            self.start()?;
        }

        let mut env_vars = self.config.env.clone();
        // Make sure the test runner has the same config environment vars as the services do.
        for (key, value) in config_env(&self.env_config) {
            env_vars.insert(key, Some(value));
        }

        env_vars.insert("TEST_LOG".to_string(), Some("info".into()));
        let mut args = self.config.args.clone().unwrap_or_default();

        args.push("--features".to_string());
        args.push(self.config.features.join(","));

        // If the test field is not present then use the --lib flag
        match self.config.test {
            Some(ref test_arg) => {
                args.push("--test".to_string());
                args.push(test_arg.to_string());
            }
            None => args.push("--lib".to_string()),
        }

        // Ensure the test_filter args are passed as well
        if let Some(ref filter) = self.config.test_filter {
            args.push(filter.to_string());
        }
        args.extend(extra_args);

        // Some arguments are not compatible with the --no-capture arg
        if !args.contains(&"--test-threads".to_string()) {
            args.push("--no-capture".to_string());
        }

        self.runner
            .test(&env_vars, &self.config.runner.env, &args)?;

        if !active {
            self.runner.remove()?;
            self.stop()?;
        }
        Ok(())
    }

    pub fn start(&self) -> Result<()> {
        self.config.check_required()?;
        if let Some(compose) = &self.compose {
            self.runner.ensure_network()?;

            if self.envs_dir.check_active(&self.environment)? {
                bail!("environment is already up");
            }

            compose.start(&self.env_config)?;

            self.envs_dir.save(&self.environment, &self.env_config)
        } else {
            Ok(())
        }
    }

    pub fn stop(&self) -> Result<()> {
        if let Some(compose) = &self.compose {
            // TODO: Is this check really needed?
            if self.envs_dir.load()?.is_none() {
                bail!("No environment for {} is up.", self.integration);
            }

            self.runner.remove()?;
            compose.stop()?;
            self.envs_dir.remove()?;
        }

        Ok(())
    }
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
                    BTreeMap::from_iter([("name".to_string(), network.clone())]),
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
