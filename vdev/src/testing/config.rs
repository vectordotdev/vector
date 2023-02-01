use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{env, fs};

use anyhow::{bail, Context, Result};
use hashlink::LinkedHashMap;
use itertools::{self, Itertools};
use serde::Deserialize;

use crate::{app, util};

const FILE_NAME: &str = "test.yaml";

pub type Environment = BTreeMap<String, Option<String>>;

#[derive(Deserialize, Debug)]
pub struct RustToolchainRootConfig {
    pub toolchain: RustToolchainConfig,
}

#[derive(Deserialize, Debug)]
pub struct RustToolchainConfig {
    pub channel: String,
}

impl RustToolchainConfig {
    pub fn parse() -> Result<Self> {
        let repo_path = app::path();
        let config_file: PathBuf = [repo_path, "rust-toolchain.toml"].iter().collect();
        let contents = fs::read_to_string(&config_file)
            .with_context(|| format!("failed to read {config_file:?}"))?;
        let config: RustToolchainRootConfig = toml::from_str(&contents)
            .with_context(|| format!("failed to parse {config_file:?}"))?;

        Ok(config.toolchain)
    }
}

#[derive(Debug, Deserialize)]
pub struct ComposeConfig {
    pub services: BTreeMap<String, ComposeService>,
}

#[derive(Debug, Deserialize)]
pub struct ComposeService {
    pub volumes: Option<Vec<String>>,
}

impl ComposeConfig {
    #[cfg(unix)]
    pub fn parse(path: &Path) -> Result<Self> {
        let contents =
            fs::read_to_string(path).with_context(|| format!("failed to read {path:?}"))?;
        serde_yaml::from_str(&contents).with_context(|| format!("failed to parse {path:?}"))
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct IntegrationTestConfig {
    /// The list of arguments to add to the docker command line for the runner
    pub args: Vec<String>,
    /// The set of environment variables to set in both the services and the runner. Variables with
    /// no value are treated as "passthrough" -- they must be set by the caller of `vdev` and are
    /// passed into the containers.
    #[serde(default)]
    pub env: Environment,
    /// The set of environment variables to set in just the runner. This is used for settings that
    /// might otherwise affect the operation of either docker or docker-compose but are needed in
    /// the runner.
    #[serde(default)]
    pub runner_env: Environment,
    /// The matrix of environment configurations values.
    matrix: LinkedHashMap<String, Vec<String>>,
    /// Does the test runner need access to the host's docker socket?
    #[serde(default)]
    pub needs_docker_sock: bool,
}

impl IntegrationTestConfig {
    fn parse_file(config_file: &Path) -> Result<Self> {
        let contents = fs::read_to_string(config_file)
            .with_context(|| format!("failed to read {}", config_file.display()))?;
        let config: IntegrationTestConfig = serde_yaml::from_str(&contents).with_context(|| {
            format!(
                "failed to parse integration test configuration file {}",
                config_file.display()
            )
        })?;

        Ok(config)
    }

    pub fn environments(&self) -> LinkedHashMap<String, Environment> {
        self.matrix
            .values()
            .multi_cartesian_product()
            .map(|product| {
                let key = product.iter().join("-");
                let config: Environment = self
                    .matrix
                    .keys()
                    .zip(product.into_iter())
                    .map(|(variable, value)| (variable.clone(), Some(value.clone())))
                    .collect();
                (key, config)
            })
            .collect()
    }

    pub fn load(integration: &str) -> Result<(PathBuf, Self)> {
        let test_dir: PathBuf = [app::path(), "scripts", "integration", integration]
            .iter()
            .collect();
        if !test_dir.is_dir() {
            bail!("unknown integration: {}", integration);
        }

        let config = Self::parse_file(&test_dir.join(FILE_NAME))?;
        Ok((test_dir, config))
    }

    pub fn collect_all() -> Result<BTreeMap<String, Self>> {
        let mut configs = BTreeMap::new();
        let tests_dir: PathBuf = [app::path(), "scripts", "integration"].iter().collect();
        for entry in tests_dir.read_dir()? {
            let entry = entry?;
            if entry.path().is_dir() {
                let config_file: PathBuf =
                    [entry.path().to_str().unwrap(), FILE_NAME].iter().collect();
                if util::exists(&config_file)? {
                    let config = Self::parse_file(&config_file)?;
                    configs.insert(entry.file_name().into_string().unwrap(), config);
                }
            }
        }

        Ok(configs)
    }

    /// Ensure that all passthrough environment variables are set.
    pub fn check_required(&self) -> Result<()> {
        let missing: Vec<_> = self
            .env
            .iter()
            .chain(self.runner_env.iter())
            .filter_map(|(key, value)| value.is_none().then_some(key))
            .filter(|var| env::var(var).is_err())
            .collect();
        if missing.is_empty() {
            Ok(())
        } else {
            let missing = missing.into_iter().join(", ");
            bail!("Required environment variables are not set: {missing}");
        }
    }
}
