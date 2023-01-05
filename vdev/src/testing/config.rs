use anyhow::{bail, Context, Result};
use hashlink::LinkedHashMap;
use itertools::{self, Itertools};
use serde::Deserialize;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::app;

const FILE_NAME: &str = "test.yaml";

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
            .with_context(|| format!("failed to read {}", config_file.display()))?;
        let config: RustToolchainRootConfig = toml::from_str(&contents)
            .with_context(|| format!("failed to parse {}", config_file.display()))?;

        Ok(config.toolchain)
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct IntegrationTestConfig {
    pub args: Vec<String>,
    pub env: Option<BTreeMap<String, String>>,
    matrix: Vec<LinkedHashMap<String, Vec<String>>>,
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

    pub fn environments(&self) -> LinkedHashMap<String, LinkedHashMap<String, String>> {
        let mut environments = LinkedHashMap::new();

        for matrix in &self.matrix {
            for product in matrix.values().multi_cartesian_product() {
                let mut config = LinkedHashMap::new();
                for (variable, &value) in matrix.keys().zip(product.iter()) {
                    config.insert(variable.clone(), value.clone());
                }

                environments.insert(product.iter().join("-"), config);
            }
        }

        environments
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

    #[allow(dead_code)]
    pub fn collect_all(root: &str) -> Result<BTreeMap<String, Self>> {
        let mut configs = BTreeMap::new();
        let tests_dir: PathBuf = [root, "scripts", "integration"].iter().collect();
        for entry in tests_dir.read_dir()? {
            let entry = entry?;
            if !entry.path().is_dir() {
                continue;
            }

            let config_file: PathBuf = [entry.path().to_str().unwrap(), FILE_NAME].iter().collect();
            let config = Self::parse_file(&config_file)?;
            configs.insert(entry.file_name().into_string().unwrap(), config);
        }

        Ok(configs)
    }
}
