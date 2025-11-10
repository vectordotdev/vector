use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use indexmap::IndexMap;
use itertools::{self, Itertools};
use serde::{Deserialize, Serialize};
use serde_yaml::Value;

use crate::{
    app,
    utils::{environment::Environment, paths},
};

const FILE_NAME: &str = "test.yaml";
const CONFIG_SUBDIR: &str = "config";

pub const INTEGRATION_TESTS_DIR: &str = "integration";
pub const E2E_TESTS_DIR: &str = "e2e";

/// Returns the base directory and whether to use config subdirectory for the given test type.
/// All tests (integration and E2E) are now in tests/ with config/ subdirectories.
fn test_dir_config(_root_dir: &str) -> (&'static str, bool) {
    ("tests", true)
}

#[derive(Deserialize, Debug)]
pub struct RustToolchainRootConfig {
    pub toolchain: RustToolchainConfig,
}

#[derive(Deserialize, Debug)]
pub struct RustToolchainConfig {
    pub channel: String,
}

impl RustToolchainConfig {
    fn parse() -> Result<Self> {
        let repo_path = app::path();
        let config_file: PathBuf = [repo_path, "rust-toolchain.toml"].iter().collect();
        let contents = fs::read_to_string(&config_file)
            .with_context(|| format!("failed to read {}", config_file.display()))?;
        let config: RustToolchainRootConfig = toml::from_str(&contents)
            .with_context(|| format!("failed to parse {}", config_file.display()))?;

        Ok(config.toolchain)
    }

    pub fn rust_version() -> String {
        match RustToolchainConfig::parse() {
            Ok(config) => config.channel,
            Err(error) => fatal!("Could not read `rust-toolchain.toml` file: {error}"),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum VolumeMount {
    Short(String),
    Long {
        #[serde(default)]
        r#type: Option<String>,
        source: String,
        target: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        read_only: Option<bool>,
    },
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum VolumeDefinition {
    Empty,
    WithOptions(BTreeMap<String, Value>),
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ComposeConfig {
    pub services: BTreeMap<String, ComposeService>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub volumes: BTreeMap<String, VolumeDefinition>,
    #[serde(default)]
    pub networks: BTreeMap<String, BTreeMap<String, Value>>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum DependsOn {
    Simple(Vec<String>),
    Conditional(BTreeMap<String, DependencyCondition>),
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DependencyCondition {
    pub condition: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ComposeService {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<Command>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ports: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env_file: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volumes: Option<Vec<VolumeMount>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depends_on: Option<DependsOn>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub healthcheck: Option<Value>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Command {
    Single(String),
    Multiple(Vec<String>),
}

impl ComposeConfig {
    pub fn parse(path: &Path) -> Result<Self> {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        serde_yaml::from_str(&contents)
            .with_context(|| format!("failed to parse {}", path.display()))
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComposeTestConfig {
    /// The list of arguments to add to the command line for the test runner
    pub args: Option<Vec<String>>,
    /// The set of environment variables to set in both the services and the runner. Variables with
    /// no value are treated as "passthrough" -- they must be set by the caller of `vdev` and are
    /// passed into the containers.
    #[serde(default)]
    pub env: Environment,
    /// The matrix of environment configurations values.
    matrix: IndexMap<String, Vec<String>>,
    /// Configuration specific to the compose services.
    #[serde(default)]
    pub runner: IntegrationRunnerConfig,

    pub features: Vec<String>,

    pub test: Option<String>,

    pub test_filter: Option<String>,

    pub paths: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationRunnerConfig {
    /// The set of environment variables to set in just the runner. This is used for settings that
    /// might otherwise affect the operation of docker but are needed in the runner.
    #[serde(default)]
    pub env: Environment,
    /// The set of volumes that need to be mounted into the runner.
    #[serde(default)]
    pub volumes: BTreeMap<String, String>,
    /// Does the test runner need access to the host's docker socket?
    #[serde(default)]
    pub needs_docker_socket: bool,
}

impl ComposeTestConfig {
    fn parse_file(config_file: &Path) -> Result<Self> {
        let contents = fs::read_to_string(config_file)
            .with_context(|| format!("failed to read {}", config_file.display()))?;
        let config: Self = serde_yaml::from_str(&contents).with_context(|| {
            format!(
                "failed to parse integration test configuration file {}",
                config_file.display()
            )
        })?;

        Ok(config)
    }

    pub fn environments(&self) -> IndexMap<String, Environment> {
        self.matrix
            .values()
            .multi_cartesian_product()
            .map(|product| {
                let key = product.iter().join("-");
                let config: Environment = self
                    .matrix
                    .keys()
                    .zip(product)
                    .map(|(variable, value)| (variable.clone(), Some(value.clone())))
                    .collect();
                (key, config)
            })
            .collect()
    }

    pub fn load(root_dir: &str, integration: &str) -> Result<(PathBuf, Self)> {
        let (base_dir, use_config_subdir) = test_dir_config(root_dir);

        let test_dir: PathBuf = [app::path(), base_dir, root_dir, integration]
            .iter()
            .collect();

        if !test_dir.is_dir() {
            bail!("unknown integration: {}", integration);
        }

        let config_dir = if use_config_subdir {
            test_dir.join(CONFIG_SUBDIR)
        } else {
            test_dir.clone()
        };
        let config = Self::parse_file(&config_dir.join(FILE_NAME))?;
        Ok((config_dir, config))
    }

    fn collect_all_dir(
        tests_dir: &Path,
        configs: &mut BTreeMap<String, Self>,
        use_config_subdir: bool,
    ) -> Result<()> {
        for entry in tests_dir.read_dir()? {
            let entry = entry?;
            if entry.path().is_dir() {
                let config_file: PathBuf = if use_config_subdir {
                    [entry.path().to_str().unwrap(), CONFIG_SUBDIR, FILE_NAME]
                        .iter()
                        .collect()
                } else {
                    [entry.path().to_str().unwrap(), FILE_NAME].iter().collect()
                };
                if paths::exists(&config_file)? {
                    let config = Self::parse_file(&config_file)?;
                    configs.insert(entry.file_name().into_string().unwrap(), config);
                }
            }
        }
        Ok(())
    }

    pub fn collect_all(root_dir: &str) -> Result<BTreeMap<String, Self>> {
        let mut configs = BTreeMap::new();

        let (base_dir, use_config_subdir) = test_dir_config(root_dir);
        let tests_dir: PathBuf = [app::path(), base_dir, root_dir].iter().collect();

        Self::collect_all_dir(&tests_dir, &mut configs, use_config_subdir)?;

        Ok(configs)
    }

    /// Ensure that all passthrough environment variables are set.
    pub fn check_required(&self) -> Result<()> {
        let missing: Vec<_> = self
            .env
            .iter()
            .chain(self.runner.env.iter())
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
