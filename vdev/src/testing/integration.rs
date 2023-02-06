use std::{path::Path, path::PathBuf, process::Command};

use anyhow::{bail, Context, Result};

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
    test_dir: PathBuf,
    config: IntegrationTestConfig,
    envs_dir: EnvsDir,
    runner: IntegrationTestRunner,
    compose_path: Option<PathBuf>,
    env_config: Environment,
}

impl IntegrationTest {
    pub fn new(integration: impl Into<String>, environment: impl Into<String>) -> Result<Self> {
        let integration = integration.into();
        let environment = environment.into();
        let (test_dir, config) = IntegrationTestConfig::load(&integration)?;
        let envs_dir = EnvsDir::new(&integration);
        let compose_path: PathBuf = [&test_dir, Path::new("compose.yaml")].iter().collect();
        let Some(env_config) = config.environments().get(&environment).map(Clone::clone) else {
            bail!("Could not find environment named {environment:?}");
        };
        // TODO: Wrap up the optional compose logic in another type
        let compose_path = compose_path
            .try_exists()
            .with_context(|| format!("Could not lookup {compose_path:?}"))?
            .then_some(compose_path);
        let runner = IntegrationTestRunner::new(
            integration.clone(),
            &config.runner,
            compose_path.is_some(),
        )?;

        Ok(Self {
            integration,
            environment,
            test_dir,
            config,
            envs_dir,
            runner,
            compose_path,
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
        if let Some((key, value)) = self.config_env(&self.env_config) {
            env_vars.insert(key, Some(value));
        }
        let mut args = self.config.args.clone();
        args.extend(extra_args);
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
        if self.compose_path.is_some() {
            self.runner.ensure_network()?;

            if self.envs_dir.check_active(&self.environment)? {
                bail!("environment is already up");
            }

            self.run_compose("Starting", &["up", "--detach"], &self.env_config)?;

            self.envs_dir.save(&self.environment, &self.env_config)
        } else {
            Ok(())
        }
    }

    pub fn stop(&self) -> Result<()> {
        if self.compose_path.is_some() {
            let Some(state) = self.envs_dir.load()? else {
                bail!("No environment for {} is up.",self.integration);
            };

            self.runner.remove()?;
            self.run_compose(
                "Stopping",
                &["down", "--timeout", "0", "--volumes"],
                &state.config,
            )?;
            self.envs_dir.remove()?;
        }

        Ok(())
    }

    fn run_compose(&self, action: &str, args: &[&'static str], config: &Environment) -> Result<()> {
        if let Some(compose_path) = &self.compose_path {
            #[cfg(unix)]
            if args[0] == "up" {
                // This preparation step is safe to do every time compose is run, but is only really
                // necessary when bring up the volumes.
                unix::prepare_compose_volumes(compose_path, &self.test_dir)?;
            }

            let mut command = CONTAINER_TOOL.clone();
            command.push("-compose");
            let mut command = Command::new(command);
            let compose_arg = compose_path.display().to_string();
            command.args(["--file", &compose_arg]);
            command.args(args);

            command.current_dir(&self.test_dir);

            command.env("DOCKER_SOCKET", &*DOCKER_SOCKET);
            if let Some(network_name) = self.runner.network_name() {
                command.env(NETWORK_ENV_VAR, network_name);
            }
            for (key, value) in &self.config.env {
                if let Some(value) = value {
                    command.env(key, value);
                }
            }
            command.envs(self.config_env(config));

            waiting!("{action} environment {}", self.environment);
            command.check_run()
        } else {
            Ok(())
        }
    }

    fn config_env(&self, config: &Environment) -> Option<(String, String)> {
        // TODO: Export all config variables, not just `version`
        match config.get("version") {
            Some(Some(version)) => Some((
                format!(
                    "{}_VERSION",
                    self.integration.replace('-', "_").to_uppercase()
                ),
                version.to_string(),
            )),
            _ => None,
        }
    }
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
    pub fn prepare_compose_volumes(path: &Path, test_dir: &Path) -> Result<()> {
        let compose_config = ComposeConfig::parse(path)?;
        for service in compose_config.services.values() {
            // Make sure all volume files are world readable
            if let Some(volumes) = &service.volumes {
                for volume in volumes {
                    let source = volume
                        .split_once(':')
                        .expect("Invalid volume in compose file")
                        .0;
                    // Only fixup relative paths, i.e. within our source tree.
                    if !compose_config.volumes.contains_key(source)
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
