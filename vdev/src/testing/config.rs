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

    /// Steps vdev runs after the environment starts, before any test runs.
    #[serde(default)]
    pub lifecycle: LifecycleConfig,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LifecycleConfig {
    /// Run in order once per `start`, so tests read complete captures and can be rerun.
    #[serde(default, with = "serde_yaml::with::singleton_map_recursive")]
    pub before_validation: Vec<LifecycleStep>,
}

/// One `before_validation` step, written as a single-key map such as `- stop: {...}`.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleStep {
    /// Wait for a one-shot compose service to exit successfully.
    WaitForSuccess(WaitForSuccess),
    /// Pause, for consumers that have no observable completion signal.
    SleepSecs(u64),
    /// Stop a group of containers together.
    Stop(StopGroup),
    /// Run a compose service, usually kept out of `up` with a profile, to completion.
    Run(RunService),
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WaitForSuccess {
    pub service: String,
    pub timeout_secs: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StopGroup {
    /// Compose service names.
    #[serde(default)]
    pub services: Vec<String>,
    /// Container labels (`key=value`), for groups too large to list by service.
    #[serde(default)]
    pub labels: Vec<String>,
    /// Docker's stop grace period before it kills a container.
    pub timeout_secs: u64,
    /// Fail if a container exited non-zero or was OOM-killed.
    #[serde(default = "default_true")]
    pub require_success: bool,
    /// Fail if any of these strings appear in a container's logs.
    #[serde(default)]
    pub reject_log_messages: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RunService {
    pub service: String,
    pub timeout_secs: u64,
}

const fn default_true() -> bool {
    true
}

impl LifecycleConfig {
    fn validate(&self) -> Result<()> {
        for step in &self.before_validation {
            if let LifecycleStep::Stop(group) = step
                && group.services.is_empty()
                && group.labels.is_empty()
            {
                bail!("a `stop` step must list `services` or `labels`");
            }
        }
        Ok(())
    }
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
        config
            .lifecycle
            .validate()
            .with_context(|| format!("invalid lifecycle in {}", config_file.display()))?;

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
            bail!("unknown integration: {integration}");
        }

        let config_dir = if use_config_subdir {
            test_dir.join(CONFIG_SUBDIR)
        } else {
            test_dir
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

#[cfg(test)]
mod tests {
    use super::*;

    fn lifecycle(yaml: &str) -> Result<LifecycleConfig> {
        let config: LifecycleConfig = serde_yaml::from_str(yaml)?;
        config.validate()?;
        Ok(config)
    }

    #[test]
    fn parses_lifecycle_steps_in_order() {
        let config = lifecycle(indoc::indoc! {"
            before_validation:
              - wait_for_success: {service: client, timeout_secs: 60}
              - sleep_secs: 5
              - stop:
                  labels: [producer=true]
                  timeout_secs: 30
                  require_success: false
              - stop:
                  services: [vector]
                  timeout_secs: 90
                  reject_log_messages: [Failed to gracefully shut down in time]
              - run: {service: drain-check, timeout_secs: 180}
        "})
        .unwrap();
        assert_eq!(
            config.before_validation,
            [
                LifecycleStep::WaitForSuccess(WaitForSuccess {
                    service: "client".into(),
                    timeout_secs: 60,
                }),
                LifecycleStep::SleepSecs(5),
                LifecycleStep::Stop(StopGroup {
                    services: vec![],
                    labels: vec!["producer=true".into()],
                    timeout_secs: 30,
                    require_success: false,
                    reject_log_messages: vec![],
                }),
                LifecycleStep::Stop(StopGroup {
                    services: vec!["vector".into()],
                    labels: vec![],
                    timeout_secs: 90,
                    require_success: true,
                    reject_log_messages: vec!["Failed to gracefully shut down in time".into()],
                }),
                LifecycleStep::Run(RunService {
                    service: "drain-check".into(),
                    timeout_secs: 180,
                }),
            ]
        );
    }

    #[test]
    fn rejects_invalid_lifecycle_steps() {
        for (yaml, expected) in [
            (
                "before_validation: [{stop: {timeout_secs: 30}}]",
                "must list",
            ),
            (
                "before_validation: [{stop: {services: [a]}}]",
                "timeout_secs",
            ),
            (
                "before_validation: [{wait: {service: a, timeout_secs: 1}}]",
                "unknown variant",
            ),
            (
                "before_validation: [{run: {service: a, timeout_secs: 1, extra: 1}}]",
                "unknown field",
            ),
        ] {
            let error = format!("{:#}", lifecycle(yaml).unwrap_err());
            assert!(error.contains(expected), "{yaml}: {error}");
        }
    }
}
